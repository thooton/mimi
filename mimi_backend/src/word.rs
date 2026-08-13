// Everything the server knows about one word for one user: where it stands
// on the **ladder**, the counters that move it along, and one FSRS card per
// retrieval mode.
//
// The ladder decides *whether* a mode may be served for a word; FSRS (see
// card.rs) decides *when*. Keeping the two apart is what makes word banks safe
// to serve at all: a bank success is weak evidence, so it stays in its own
// card and only earns the word a rung, never a place in the harder modes'
// scheduling.
//
// This module owns both halves of a verdict — the card it lands on and the
// rung it moves — so `record` is the single place a word changes. The user
// (see user.rs) routes verdicts here and never touches a stage or a counter
// itself.

use serde::{Deserialize, Serialize};

use crate::card::Card;
use crate::exercise::Mode;

// consecutive bank successes to graduate Scaffolding → Recognition
const STREAK_GRADUATE_S: u32 = 2;
// consecutive typed-recognition successes to graduate Recognition →
// Recognition + Production
const STREAK_GRADUATE_R: u32 = 2;
// relearning is faster than learning: after a demotion, this many bank
// successes re-promote S → R...
const STREAK_REPROMOTE_S: u32 = 2;
// ...and this many typed-recognition successes re-promote R → RP
const STREAK_REPROMOTE_R: u32 = 2;
// consecutive failures at a word's top mode before it's demoted a rung
const LAPSE_DEMOTE: u32 = 2;
// a mode whose card's retrievability falls below this demotes its word a
// rung — clearly below FSRS's normal 0.7–0.9 operating band, so merely-due
// cards don't bounce
const DEMOTE_R: f64 = 0.45;
// the derived success probability of a word's FIRST exercise in a freshly
// unlocked mode (a pick probability only, never written to state): this
// times the retrievability of the card one rung below
const C_RECOGNITION: f64 = 0.75;
const C_PRODUCTION: f64 = 0.60;

// Where a word stands on the ladder: which modes the lesson builder may
// serve for it. Graduation retires exactly one mode — scaffolding — and
// recognition is never retired: understanding and producing are both real
// skills, and retiring recognition would halve the builder's candidate pool
// for exactly the user's best-known words.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    // only word banks are legal
    Scaffolding,
    // only typed es->en is legal
    Recognition,
    // typed es->en and typed en->es are both legal, forever
    RecognitionProduction,
}

impl Stage {
    // may the builder serve an exercise of this mode for a word here?
    pub fn allows(self, mode: Mode) -> bool {
        match self {
            Stage::Scaffolding => mode == Mode::Scaffolding,
            Stage::Recognition => mode == Mode::Recognition,
            Stage::RecognitionProduction => mode != Mode::Scaffolding,
        }
    }

    // the hardest mode this stage serves: where successes count toward the
    // next rung, and where failures count toward demotion
    fn top_mode(self) -> Mode {
        match self {
            Stage::Scaffolding => Mode::Scaffolding,
            Stage::Recognition => Mode::Recognition,
            Stage::RecognitionProduction => Mode::Production,
        }
    }

    // The stage a word introduced by an exercise of this mode starts at.
    //
    // In practice this is always Scaffolding: a lesson introduces a word with
    // a word bank over a sentence that uses nothing else, which is the
    // gentlest possible first contact. The other arms are defensive — being
    // asked to produce a word you have never seen isn't teaching.
    pub fn introduced_by(mode: Mode) -> Stage {
        match mode {
            Mode::Scaffolding => Stage::Scaffolding,
            Mode::Recognition => Stage::Recognition,
            Mode::Production => Stage::RecognitionProduction,
        }
    }
}

// One word's state for one user.
//
// Three cards, not one shared one and not cross-updated ones: recognition and
// recall are different retrieval tasks with different difficulties, and a
// verdict updates only the card of the mode that produced it, ever. FSRS's
// per-card difficulty is literally its item-difficulty knob, so the three
// cards self-separate by task — and a guessable bank success can't inflate
// the state that drives scheduling for the harder modes.
//
// The counters climb the ladder: `streak` consecutive successes toward the
// next rung (a success at the word's top mode *or a harder one* counts —
// proving you can type it counts at least as proving you can tap it), and
// `lapses` consecutive failures at the top mode. Graduation is deliberately
// easy (the ladder is optimistic, so an engaged user unlocks harder material
// quickly), and demotion is the corrector: `LAPSE_DEMOTE` lapses, or a card's
// retrievability decaying below `DEMOTE_R`, drops the word a rung and sets
// `repromoting`, which shortens the streak needed to climb back — relearning
// is faster than learning. A demoted mode's card is left alone: its
// honestly-decayed R is the right prior, and on re-promotion it is maximally
// urgent, so the builder's first phase resurfaces it promptly.
#[derive(Clone, Serialize, Deserialize)]
pub struct WordState {
    pub stage: Stage,
    // consecutive successes toward the next rung (unused at the top)
    pub streak: u32,
    // consecutive failures at the current top mode
    pub lapses: u32,
    // demoted: use the short re-promotion streaks
    pub repromoting: bool,
    pub bank: Option<Card>,
    pub recognition: Option<Card>,
    pub production: Option<Card>,
}

impl WordState {
    // a word fresh onto the ladder, at the stage its introduction chose
    // (all counters zero, no cards yet); the harder modes' cards are born
    // from their first real verdicts
    pub fn new(stage: Stage) -> WordState {
        WordState {
            stage,
            streak: 0,
            lapses: 0,
            repromoting: false,
            bank: None,
            recognition: None,
            production: None,
        }
    }

    // a word sitting at `stage` with `card` as the card of that stage's top
    // mode — the shape most tests want to set a word up in
    #[cfg(test)]
    pub fn at(stage: Stage, card: Card) -> WordState {
        let mut state = WordState::new(stage);
        state.set_card(stage.top_mode(), card);
        state
    }

    // a word at the bottom of the ladder, answered right when it was
    // introduced — what a lesson's introducing word bank leaves behind
    #[cfg(test)]
    pub fn scaffolded(timestamp: u64) -> WordState {
        WordState::at(Stage::Scaffolding, Card::new().good(timestamp))
    }

    // the card for this mode, if the word has one (a mode's card is born
    // from its first real verdict — no seeding, no fake reviews)
    pub fn card(&self, mode: Mode) -> Option<Card> {
        match mode {
            Mode::Scaffolding => self.bank,
            Mode::Recognition => self.recognition,
            Mode::Production => self.production,
        }
    }

    pub fn set_card(&mut self, mode: Mode, card: Card) {
        match mode {
            Mode::Scaffolding => self.bank = Some(card),
            Mode::Recognition => self.recognition = Some(card),
            Mode::Production => self.production = Some(card),
        }
    }

    // may an exercise of this mode be served for this word?
    pub fn allows(&self, mode: Mode) -> bool {
        self.stage.allows(mode)
    }

    // One verdict, applied to the card it came from and then to the ladder.
    //
    // The card first: a verdict is evidence about the retrieval task that
    // produced it and nothing else, so it lands on that mode's card, creating
    // it if this is the mode's first real attempt.
    //
    // Then the rung. Successes at the word's top mode or a harder one count
    // toward the next one; failures at any legal mode break the streak, and
    // failures at the top mode count toward demotion. A *failure* at a mode
    // the stage forbids proves nothing about the rung being climbed, so it
    // moves no counter.
    //
    // In an ordinary lesson a verdict at a forbidden mode cannot happen at
    // all: the builder's legality gate sees to that. A castle is the one
    // place it can, because a castle asks what it asks — and there the caller
    // uses `record_test`, which is careful about the card as well as the
    // counters.
    pub fn record(&mut self, mode: Mode, correct: bool, timestamp: u64) {
        let card = self.card(mode).unwrap_or_else(Card::new);
        self.set_card(
            mode,
            if correct {
                card.good(timestamp)
            } else {
                card.again(timestamp)
            },
        );

        let top = self.stage.top_mode();
        if correct {
            if mode == top {
                self.lapses = 0;
            }
            // `Mode` is ordered by difficulty, so this is "at least as hard as
            // the rung being climbed". `streak_needed` is None at the top,
            // where there is no next rung to climb to.
            if mode >= top
                && let Some(needed) = self.streak_needed()
            {
                self.streak += 1;
                if self.streak >= needed {
                    self.promote();
                }
            }
        } else if self.allows(mode) {
            self.streak = 0;
            if mode == top {
                self.lapses += 1;
                if self.lapses >= LAPSE_DEMOTE && self.stage != Stage::Scaffolding {
                    self.demote();
                }
            }
        }
    }

    // One verdict from a **castle**, where the ladder is bypassed on the way
    // in: the test asks for recognition and production regardless of what
    // stage a word has reached, because a test restricted to what the ladder
    // already permits would not be testing the thing worth testing.
    //
    // The rule is asymmetric on purpose. **A castle can help a learner and can
    // never hurt one for material they weren't expected to know**: producing a
    // word you were only ever scaffolded on demonstrates something real and is
    // credited in full (the card is born, and `record`'s "a success at a
    // harder mode counts at least as much" rule advances the rung), while
    // failing one proves nothing and is dropped entirely.
    //
    // Dropping it has to mean dropping the *card* too, not just the counters.
    // `record` writes the card before it looks at legality, so letting a
    // failure through here would mint a fresh production card in the `again`
    // state for a word still at Scaffolding. It would never be served — `due`
    // only yields legal modes — but it would sit there, and when the word
    // finally graduated weeks later it would inherit a damaged card instead of
    // FSRS's first-review initialization. That is the punishment this rule
    // exists to prevent, merely deferred.
    pub fn record_test(&mut self, mode: Mode, correct: bool, timestamp: u64) {
        if correct || self.allows(mode) {
            self.record(mode, correct, timestamp);
        }
    }

    // The deep-forgetting path: a top-mode card that has decayed below
    // DEMOTE_R slides the word down a rung without any verdict at all —
    // possibly two, if both cards are gone. This is what makes "back from
    // vacation" work on its own: R decays past the threshold, the next lesson
    // is warm-ups, and urgency floods the higher modes back in over the
    // following lessons. A mode with no card yet can't be forgotten — it is
    // simply unattempted — so missing cards are skipped.
    pub fn demote_if_decayed(&mut self, timestamp: u64) {
        let decayed =
            |card: Option<Card>| card.is_some_and(|card| card.retrievability(timestamp) < DEMOTE_R);
        if self.stage == Stage::RecognitionProduction && decayed(self.production) {
            self.demote();
        }
        if self.stage == Stage::Recognition && decayed(self.recognition) {
            self.demote();
        }
    }

    // The estimated probability that the user gets this word right in this
    // mode, right now.
    //
    // A mode the stage allows but that has no card yet contributes a derived
    // first-attempt probability — a constant times the retrievability of the
    // card one rung below — which the builder uses both to *target* (how
    // likely this exercise is to go right) and to *schedule* (`due` sorts an
    // unattempted mode by it, so a just-graduated word is urgent). This
    // is a pick probability only; it is never written to state.
    pub fn probability(&self, mode: Mode, timestamp: u64) -> f64 {
        let r = |card: Option<Card>| card.map_or(0.0, |card| card.retrievability(timestamp));
        match self.card(mode) {
            Some(card) => card.retrievability(timestamp),
            None => match mode {
                Mode::Scaffolding => 0.0, // never studied
                Mode::Recognition => C_RECOGNITION * r(self.bank),
                Mode::Production => C_PRODUCTION * r(self.recognition),
            },
        }
    }

    // How urgently each of this word's legal modes wants serving, as
    // (success probability, mode) pairs — lowest is most urgent.
    //
    // One entry per mode the stage allows: urgency is per (word, mode),
    // because an RP word with a fresh recognition card and a decayed
    // production one must be served a production exercise or its production
    // decay would never be addressed. A mode with a card sorts by its
    // retrievability; a mode with no card yet sorts by its derived
    // first-attempt probability (see `probability`), so a freshly unlocked
    // mode is *urgent*: the builder's first phase serves it, and the verdict
    // births its card. Merely making it *pickable* in the fill phase instead
    // starves it forever — the derived probability loses every comparison
    // there, so the mode never gets a card, so it is never due.
    pub fn due(&self, timestamp: u64) -> impl Iterator<Item = (f64, Mode)> {
        [Mode::Scaffolding, Mode::Recognition, Mode::Production]
            .into_iter()
            .filter(|&mode| self.allows(mode))
            .map(move |mode| (self.probability(mode, timestamp), mode))
    }

    // how long a streak this word needs to climb, or None at the top, where
    // there is nowhere left to climb to
    fn streak_needed(&self) -> Option<u32> {
        match (self.stage, self.repromoting) {
            (Stage::Scaffolding, false) => Some(STREAK_GRADUATE_S),
            (Stage::Scaffolding, true) => Some(STREAK_REPROMOTE_S),
            (Stage::Recognition, false) => Some(STREAK_GRADUATE_R),
            (Stage::Recognition, true) => Some(STREAK_REPROMOTE_R),
            (Stage::RecognitionProduction, _) => None,
        }
    }

    // up a rung, counters cleared: graduation and re-promotion alike
    pub fn promote(&mut self) {
        self.stage = match self.stage {
            Stage::Scaffolding => Stage::Recognition,
            Stage::Recognition => Stage::RecognitionProduction,
            Stage::RecognitionProduction => Stage::RecognitionProduction,
        };
        self.streak = 0;
        self.lapses = 0;
        self.repromoting = false;
    }

    // down a rung (no further than the bottom), counters cleared and the
    // short re-promotion streaks armed. The demoted mode's card is left
    // alone — no fake ratings, no clock resets.
    fn demote(&mut self) {
        self.stage = match self.stage {
            Stage::Scaffolding => Stage::Scaffolding,
            Stage::Recognition => Stage::Scaffolding,
            Stage::RecognitionProduction => Stage::Recognition,
        };
        self.streak = 0;
        self.lapses = 0;
        self.repromoting = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    const DAY: u64 = 86400;

    fn now() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }

    // --- climbing ---

    // three bank successes in a row and the word is ready for unaided
    // recognition: graduation is deliberately easy to reach
    #[test]
    fn a_streak_of_bank_successes_graduates_to_recognition() {
        let mut state = WordState::new(Stage::Scaffolding);
        let t0 = now();
        for i in 0..STREAK_GRADUATE_S {
            state.record(Mode::Scaffolding, true, t0 + u64::from(i));
        }
        assert_eq!(state.stage, Stage::Recognition);
        // the counters start fresh on the new rung
        assert_eq!(state.streak, 0);
        assert_eq!(state.lapses, 0);
        assert!(!state.repromoting);
        // and the bank card came with it: honest, and never served again
        assert!(state.bank.is_some());
    }

    #[test]
    fn a_streak_of_recognition_successes_graduates_to_production() {
        let mut state = WordState::new(Stage::Recognition);
        let t0 = now();
        for i in 0..STREAK_GRADUATE_R {
            state.record(Mode::Recognition, true, t0 + u64::from(i));
        }
        assert_eq!(state.stage, Stage::RecognitionProduction);
    }

    // a word that almost graduated must start its streak over when it
    // fails — the streak is *consecutive* successes
    #[test]
    fn a_failure_resets_the_streak() {
        let mut state = WordState::new(Stage::Scaffolding);
        let t0 = now();
        for i in 0..STREAK_GRADUATE_S - 1 {
            state.record(Mode::Scaffolding, true, t0 + u64::from(i));
        }
        state.record(Mode::Scaffolding, false, t0 + 10);
        assert_eq!(state.streak, 0);
        assert_eq!(state.stage, Stage::Scaffolding);
        // and two more successes don't make three: it starts from zero
        for i in 0..STREAK_GRADUATE_S - 1 {
            state.record(Mode::Scaffolding, true, t0 + 20 + u64::from(i));
        }
        assert_eq!(state.stage, Stage::Scaffolding);
        state.record(Mode::Scaffolding, true, t0 + 30);
        assert_eq!(state.stage, Stage::Recognition);
    }

    // proving you can recognize a word counts at least as much as proving you
    // can tap it: a success at a harder mode than the word's current one
    // advances its streak
    #[test]
    fn a_harder_mode_success_advances_the_streak() {
        let t0 = now();
        let mut state = WordState::scaffolded(t0);
        for i in 0..STREAK_GRADUATE_S {
            state.record(Mode::Recognition, true, t0 + u64::from(i));
        }
        assert_eq!(state.stage, Stage::Recognition);
    }

    // --- falling ---

    // two consecutive failures at the top mode drop the word a rung; two
    // consecutive successes bring it back
    #[test]
    fn lapses_demote_and_arm_a_quick_repromotion() {
        let t0 = now();
        let mut state = WordState::at(Stage::RecognitionProduction, Card::new().good(t0 - 3 * DAY));
        state.record(Mode::Production, false, t0);
        assert_eq!(state.stage, Stage::RecognitionProduction);
        assert_eq!(state.lapses, 1);
        // a success at the top mode breaks the lapse streak...
        state.record(Mode::Production, true, t0 + 1);
        assert_eq!(state.lapses, 0);
        // ...but two in a row demote
        state.record(Mode::Production, false, t0 + 2);
        state.record(Mode::Production, false, t0 + 3);
        assert_eq!(state.stage, Stage::Recognition);
        assert!(state.repromoting);
        assert_eq!(state.lapses, 0);
        // and re-promotion takes two recognition successes
        state.record(Mode::Recognition, true, t0 + 4);
        assert_eq!(state.stage, Stage::Recognition);
        state.record(Mode::Recognition, true, t0 + 5);
        assert_eq!(state.stage, Stage::RecognitionProduction);
        assert!(!state.repromoting);
    }

    // the deep-forgetting path: a card that has decayed well past FSRS's
    // operating band demotes its word without any verdict at all — this is
    // what makes "back from vacation" slide words down the ladder
    #[test]
    fn decay_below_the_threshold_demotes_a_rung_at_a_time() {
        let t0 = now();
        // a word at the top of the ladder whose production card is long
        // decayed (a good review a year ago retrieves far below 0.45)
        let mut state = WordState::at(
            Stage::RecognitionProduction,
            Card::new().good(t0 - 365 * DAY),
        );
        state.recognition = Some(Card::new().good(t0 - 365 * DAY));
        state.demote_if_decayed(t0);
        // both cards are gone, so it slides straight to the bottom
        assert_eq!(state.stage, Stage::Scaffolding);
        assert!(state.repromoting);
    }

    // ...but a mode nobody has attempted yet can't be "forgotten": a
    // just-graduated word has no production card at all, and the decay
    // sweep must not bounce it straight back down
    #[test]
    fn the_decay_sweep_leaves_unattempted_modes_alone() {
        let t0 = now();
        let mut state = WordState::new(Stage::Scaffolding);
        for i in 0..STREAK_GRADUATE_S {
            state.record(Mode::Scaffolding, true, t0 + u64::from(i));
        }
        assert_eq!(state.stage, Stage::Recognition);
        state.demote_if_decayed(t0);
        assert_eq!(state.stage, Stage::Recognition);
    }

    // after a demotion to Scaffolding, relearning moves faster than learning:
    // two bank successes (not three) re-promote
    #[test]
    fn re_promotion_from_scaffolding_is_quick() {
        let t0 = now();
        // a Recognition-stage word whose recognition card has decayed away
        // slides to the bottom
        let mut state = WordState::at(Stage::Recognition, Card::new().good(t0 - 365 * DAY));
        state.demote_if_decayed(t0);
        assert_eq!(state.stage, Stage::Scaffolding);
        // and climbs back in two
        state.record(Mode::Scaffolding, true, t0 + 1);
        assert_eq!(state.stage, Stage::Scaffolding);
        state.record(Mode::Scaffolding, true, t0 + 2);
        assert_eq!(state.stage, Stage::Recognition);
        assert!(!state.repromoting);
    }

    // a failure at a mode the stage forbids proves nothing about the rung
    // being climbed: a bank is too guessable to count against a word that has
    // moved past it
    #[test]
    fn a_verdict_at_a_forbidden_mode_moves_no_counter() {
        let t0 = now();
        let mut state = WordState::at(Stage::Recognition, Card::new().good(t0));
        state.record(Mode::Scaffolding, false, t0 + 1);
        assert_eq!(state.stage, Stage::Recognition);
        assert_eq!(state.streak, 0);
        assert_eq!(state.lapses, 0);
        // the card still took the verdict, though — it is honest evidence
        // about the bank, just not about the rung
        assert_eq!(state.bank.unwrap().last_reviewed, t0 + 1);
    }

    // --- castles ---

    // A castle asks for production regardless of where a word sits, and
    // getting it right is real evidence: the card is born and the rung moves.
    // This is how a castle can *accelerate* a strong learner's tree.
    #[test]
    fn a_castle_success_above_the_ladder_counts_in_full() {
        let t0 = now();
        let mut state = WordState::scaffolded(t0);
        state.record_test(Mode::Production, true, t0 + 1);
        assert_eq!(state.production.unwrap().last_reviewed, t0 + 1);
        assert_eq!(state.streak, 1);
    }

    // ...and getting it wrong is dropped entirely, card included. Letting the
    // card through would mint a damaged production card for a word still at
    // Scaffolding, which it would then inherit on graduating weeks later
    // instead of FSRS's first-review initialization.
    #[test]
    fn a_castle_failure_above_the_ladder_leaves_no_trace() {
        let t0 = now();
        let mut state = WordState::scaffolded(t0);
        state.streak = 2;
        state.record_test(Mode::Production, false, t0 + 1);
        assert!(state.production.is_none());
        assert_eq!(state.streak, 2); // not even the streak breaks
        assert_eq!(state.stage, Stage::Scaffolding);
    }

    // a verdict at a mode the word *has* reached is ordinary evidence,
    // however it came about
    #[test]
    fn a_castle_verdict_at_a_legal_mode_is_recorded_normally() {
        let t0 = now();
        let mut state = WordState::at(Stage::RecognitionProduction, Card::new().good(t0 - DAY));
        state.record_test(Mode::Production, false, t0);
        assert_eq!(state.lapses, 1);
        assert_eq!(state.production.unwrap().last_reviewed, t0);
    }

    // --- cards ---

    // a verdict is evidence about the retrieval task that produced it, and
    // nothing else: the other two cards must not move
    #[test]
    fn a_verdict_touches_only_its_own_modes_card() {
        let t0 = now();
        let mut state = WordState::at(Stage::RecognitionProduction, Card::new().good(t0 - 3 * DAY));
        let production_before = state.production.unwrap();
        state.record(Mode::Recognition, true, t0);
        assert_eq!(state.recognition.unwrap().last_reviewed, t0);
        assert_eq!(state.production.unwrap(), production_before);
        assert!(state.bank.is_none());
    }

    // material tells, it doesn't ask: the seed lands on the scaffolding card,
    // and the harder modes stay empty until a real verdict creates them
    #[test]
    fn a_scaffolded_word_starts_at_the_bottom_with_one_card() {
        let t0 = now();
        let state = WordState::scaffolded(t0);
        assert_eq!(state.stage, Stage::Scaffolding);
        let card = state.bank.unwrap();
        assert_eq!(card.last_reviewed, t0);
        // just been shown the answer, so recall is (near-)certain right now
        assert!(card.retrievability(t0) > 0.99);
        assert!(state.recognition.is_none());
        assert!(state.production.is_none());
    }

    // --- probabilities and urgency ---

    // a just-graduated word has no card in its newly unlocked mode, so the
    // probability is derived from the rung below — hard enough to be
    // interesting, never written to state
    #[test]
    fn a_new_modes_probability_is_derived_from_the_rung_below() {
        let t0 = now();
        let mut state = WordState::scaffolded(t0 - DAY);
        state.promote(); // Recognition, with only a bank card
        let bank_r = state.bank.unwrap().retrievability(t0);
        assert!((state.probability(Mode::Recognition, t0) - C_RECOGNITION * bank_r).abs() < 1e-9);

        state.promote(); // RecognitionProduction
        state.recognition = Some(Card::new().good(t0 - DAY));
        let rec_r = state.recognition.unwrap().retrievability(t0);
        assert!((state.probability(Mode::Production, t0) - C_PRODUCTION * rec_r).abs() < 1e-9);
    }

    // a word nobody has ever studied cannot be retrieved at all
    #[test]
    fn an_unstudied_word_has_no_chance() {
        let state = WordState::new(Stage::Scaffolding);
        assert_eq!(state.probability(Mode::Scaffolding, now()), 0.0);
    }

    // urgency is per (word, mode), and only for modes the stage still
    // allows that have actually been attempted
    #[test]
    fn due_reports_one_entry_per_allowed_card() {
        let t0 = now();
        let mut state = WordState::at(
            Stage::RecognitionProduction,
            Card::new().good(t0 - 30 * DAY),
        );
        state.recognition = Some(Card::new().good(t0));
        state.bank = Some(Card::new().good(t0)); // retired: not allowed any more
        let due: Vec<Mode> = state.due(t0).map(|(_, mode)| mode).collect();
        assert_eq!(due, [Mode::Recognition, Mode::Production]);
        // the long-decayed production card is the more urgent of the two
        let production = state.due(t0).find(|&(_, m)| m == Mode::Production).unwrap();
        let recognition = state
            .due(t0)
            .find(|&(_, m)| m == Mode::Recognition)
            .unwrap();
        assert!(production.0 < recognition.0);
    }

    // a mode the stage allows but that has never been attempted is due at
    // its derived first-attempt probability: this is what gets a freshly
    // unlocked mode served — and its card born — instead of starving it
    #[test]
    fn due_includes_an_unattempted_mode_at_its_derived_probability() {
        let t0 = now();
        // top of the ladder with only a recognition card: production is
        // legal but has never been attempted
        let mut state = WordState::new(Stage::RecognitionProduction);
        state.recognition = Some(Card::new().good(t0 - 3 * DAY));
        let r = state.recognition.unwrap().retrievability(t0);
        let due: Vec<(f64, Mode)> = state.due(t0).collect();
        // scaffolding is retired, so the entries are exactly the two typed
        // modes: recognition by its card, production by the derivation
        assert_eq!(due.len(), 2);
        assert_eq!(due[0], (r, Mode::Recognition));
        assert!((due[1].0 - C_PRODUCTION * r).abs() < 1e-9);
        assert_eq!(due[1].1, Mode::Production);
    }
}
