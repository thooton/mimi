// One learner: what they know, and how far through the tree they are.
//
// A user is a map of word -> `WordState`, a cumulative count of completed
// completed lessons per skill, and the number of castles they have passed. The state of any single
// word — its rung on the ladder, its three cards — belongs to word.rs and is
// never reached into from here; this module's job is everything that spans
// *several* words at once: applying a whole lesson's verdicts, estimating how
// an exercise will go, and deciding what is most worth reviewing.
//
// **Progress is a set, not a point.** Skills in a row may be done in any
// order, so no single coordinate says where somebody is: two learners on the
// same row may have finished entirely different skills behind it. Everything
// about what is unlocked is derived from `progress` and `castles` against the
// course's shape, and nothing about it is stored.
//
// Building a lesson lives in lesson.rs, which reads a user through the three
// questions below (`allows`, `probability`, `due`) rather than by rummaging
// through their words.

use std::collections::{HashMap, HashSet};

use crate::course::Course;
use crate::exercise::{Ask, FlashcardDirection, Mode};
use crate::lesson::Target;
use crate::position::Position;
use crate::skill::{LESSONS_PER_LEVEL, MAX_LEVEL, ROW_GATE_LEVEL, SKILL_LESSONS};
use crate::word::WordState;

// the fraction of a castle's questions a learner must get right to pass it.
// Not meant to be a wall: a castle exists to stop new material piling onto a
// shaky foundation, and somebody who has kept up passes it without noticing.
pub const CASTLE_PASS: f64 = 0.8;

// How many cards one request for flashcards hands out. Standalone practice
// has no end — the client comes back for another batch for as long as the
// learner keeps going — so this is not a session length; it is how far ahead
// of the learner the server is willing to commit. Small on purpose: ratings
// move the underlying cards, and the batch after this one is chosen from a
// vocabulary these verdicts have already reordered, which is what keeps a
// long sitting from being one stale ordering read out to the end.
pub const FLASHCARD_BATCH_SIZE: usize = 20;

// One question returned by the client. Unlike a served `Exercise`, this is a
// self-describing memory delta: the retrieval task says which of a word's
// three cards to update, and the map names every word the question tested and
// its verdict. It contains no sentence index and survives a course reload.
pub struct QuestionReport {
    pub ask: Ask,
    pub correct: bool,
    pub words: HashMap<String, bool>,
}

// Everything needed to apply a lesson after the course that produced its
// prose may have been replaced. Skill, castle and word identities are stable
// strings/indices; prompts, answers and sentence pointers are deliberately
// absent because grading already happened on the client.
pub struct Submission {
    pub target: Target,
    pub questions: Vec<QuestionReport>,
}

// One self-graded vocabulary card. Direction is the retrieval task: target
// to source is recognition, while source to target is production.
pub struct FlashcardReport {
    pub word: String,
    pub direction: FlashcardDirection,
    pub correct: bool,
}

pub struct FlashcardOutcome {
    pub correct: usize,
    pub total: usize,
}

#[derive(Debug, PartialEq, Eq)]
pub enum FlashcardSubmissionError {
    Empty,
    UnknownWord(String),
    UnencounteredWord(String),
    RepeatedCard {
        word: String,
        direction: FlashcardDirection,
    },
}

impl std::fmt::Display for FlashcardSubmissionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => write!(f, "a flashcard session must contain at least one card"),
            Self::UnknownWord(word) => write!(f, "the course has no word named '{word}'"),
            Self::UnencounteredWord(word) => {
                write!(f, "word '{word}' has not been encountered by this learner")
            }
            Self::RepeatedCard { word, direction } => {
                write!(
                    f,
                    "flashcard '{word}' in direction {direction:?} was reported twice"
                )
            }
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum SubmissionError {
    Empty,
    EmptyQuestion,
    NoSuchLesson(Position),
    LessonLocked(Position),
    NoSuchCastle(usize),
    CastleLocked(usize),
    UnknownWord(String),
    UnexpectedNewWord { word: String, skill: String },
    WordOutsideCastle { word: String, castle: usize },
    RepeatedWord(String),
}

impl std::fmt::Display for SubmissionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => write!(f, "a lesson must contain at least one question"),
            Self::EmptyQuestion => write!(f, "every question must test at least one word"),
            Self::NoSuchLesson(position) => write!(f, "the course has no lesson at {position}"),
            Self::LessonLocked(position) => {
                write!(f, "the lesson at {position} is still locked")
            }
            Self::NoSuchCastle(castle) => write!(f, "the course has no castle {castle}"),
            Self::CastleLocked(castle) => {
                write!(f, "castle {castle} is not currently available")
            }
            Self::UnknownWord(word) => write!(f, "the course has no word named '{word}'"),
            Self::UnexpectedNewWord { word, skill } => write!(
                f,
                "word '{word}' is new to the learner but is not taught by skill '{skill}'"
            ),
            Self::WordOutsideCastle { word, castle } => {
                write!(f, "word '{word}' is not covered by castle {castle}")
            }
            Self::RepeatedWord(word) => {
                write!(f, "word '{word}' is reported by more than one question")
            }
        }
    }
}

// Where a skill stands for one user, as the course map shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillState {
    // every lesson done
    Completed,
    // its row is open: the learner may take it now
    Available,
    // an earlier row is unfinished, or a castle stands in the way
    Locked,
}

#[derive(Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct User {
    pub words: HashMap<String, WordState>,
    // skill id -> how many of its lessons are finished. A skill missing from
    // the map has had none.
    pub progress: HashMap<String, u8>,
    // how many castles the learner has passed, which is also the index of the
    // next one they face
    pub castles: usize,
}

impl User {
    pub fn new() -> User {
        User::default()
    }

    // --- where they are ---

    pub fn lessons_done(&self, skill: &str) -> u8 {
        self.progress.get(skill).copied().unwrap_or(0)
    }

    // Skill level (0-5) and progress through its six lessons. At the maximum the
    // progress is reported as six-of-six instead of wrapping back to zero.
    pub fn skill_level(&self, skill: &str) -> u8 {
        (self.lessons_done(skill) / LESSONS_PER_LEVEL).min(MAX_LEVEL)
    }

    pub fn lessons_done_in_level(&self, skill: &str) -> u8 {
        let done = self.lessons_done(skill);
        if done >= LESSONS_PER_LEVEL * (MAX_LEVEL + 1) {
            LESSONS_PER_LEVEL
        } else {
            done % LESSONS_PER_LEVEL
        }
    }

    pub fn has_row_level(&self, skill: &str) -> bool {
        self.lessons_done(skill) >= LESSONS_PER_LEVEL * ROW_GATE_LEVEL
    }

    // Has every skill in every row before `row` reached level 2, and has
    // the learner passed every castle
    // standing in front of it passed?
    //
    // The castle clause is the whole of what a castle *does*: a row belongs
    // to some castle's stretch, and being allowed into that stretch means
    // having passed every castle before it.
    pub fn row_is_open(&self, course: &Course, row: usize) -> bool {
        let castle = course
            .skills()
            .iter()
            .find(|skill| skill.row == row)
            .map_or(0, |skill| skill.castle);
        if self.castles < castle {
            return false;
        }
        course.rows()[..row.min(course.rows().len())]
            .iter()
            .flatten()
            .all(|&i| self.has_row_level(&course.skills()[i].id))
    }

    pub fn has_completed(&self, skill: &str, course: &Course) -> bool {
        course.skill(skill).is_some() && self.lessons_done(skill) >= SKILL_LESSONS
    }

    pub fn skill_state(&self, course: &Course, skill: &str) -> SkillState {
        let Some(definition) = course.skill(skill) else {
            return SkillState::Locked;
        };
        if self.has_completed(skill, course) {
            SkillState::Completed
        } else if self.row_is_open(course, definition.row) {
            SkillState::Available
        } else {
            SkillState::Locked
        }
    }

    // May this lesson be built for this learner? Its skill must be open, and
    // the lesson must be one they have reached — the next one, or any they
    // have already done and want to review.
    pub fn may_take(&self, course: &Course, position: &Position) -> bool {
        course.has_lesson(position)
            && self.skill_state(course, &position.skill) != SkillState::Locked
            && position.lesson <= self.lessons_done(&position.skill) + 1
    }

    // Where "continue" goes: first the required level work (through level 2),
    // then optional level 4-5 work. None at the end or when a castle waits.
    pub fn next_lesson(&self, course: &Course) -> Option<Position> {
        if self.castle_due(course).is_some() {
            return None;
        }
        let available: Vec<_> = course
            .skills()
            .iter()
            .filter(|skill| self.skill_state(course, &skill.id) == SkillState::Available)
            .collect();
        // "Continue" advances the required levels before offering optional
        // higher-level practice in rows already cleared at level 2.
        available
            .iter()
            .copied()
            .find(|skill| !self.has_row_level(&skill.id))
            .or_else(|| available.first().copied())
            .map(|skill| Position::new(&skill.id, self.lessons_done(&skill.id) + 1))
    }

    // The castle the learner is up against, if it is ready: the next one they
    // have not passed, once every skill in its stretch is finished.
    pub fn castle_due(&self, course: &Course) -> Option<usize> {
        let castle = course.castles().get(self.castles)?;
        let ready = course.rows()[castle.rows.clone()]
            .iter()
            .flatten()
            .all(|&i| self.has_row_level(&course.skills()[i].id));
        ready.then_some(self.castles)
    }

    // --- what the lesson builder asks ---

    // May a question grading these words in this mode fill a generated hole
    // for this user?
    //
    // Every word must be one they have **met**, and must allow the mode at its
    // current stage. Both halves are all-or-nothing, so "never production
    // first" can't leak through a sentence drill, and neither can a word from
    // a row-mate skill they haven't started.
    //
    // The met half is load-bearing rather than decorative. The arithmetic
    // would *mostly* handle it — an unmet word takes `probability` to zero and
    // loses nearly every comparison — but "mostly" isn't a rule, and the whole
    // point of a branching tree is that plenty of reachable material is
    // material this particular learner has never seen.
    //
    // Words and a mode rather than an `Exercise`, because the builder weighs
    // thousands of candidates and builds none of them: a sentence and a way of
    // asking it are all this needs, and are all it is given (see course.rs).
    pub fn allows(&self, words: &[String], mode: Mode) -> bool {
        words
            .iter()
            .all(|word| self.words.get(word).is_some_and(|state| state.allows(mode)))
    }

    // The estimated probability that the user answers such a question
    // correctly: the product of the retrievability of every word it grades, on
    // the card matching its mode. (This assumes independence, which is not
    // exactly true, but because a lesson never grades the same word twice, it
    // is a good approximation.)
    //
    // A word the user has never studied contributes 0 — it drags the whole
    // question to 0, which is what "you cannot answer this yet" should mean.
    pub fn probability(&self, words: &[String], mode: Mode, timestamp: u64) -> f64 {
        words
            .iter()
            .map(|word| {
                self.words
                    .get(word)
                    .map_or(0.0, |state| state.probability(mode, timestamp))
            })
            .product()
    }

    // Everything the user could usefully be served next, most urgent (lowest
    // success probability) first, as (probability, word, mode).
    //
    // One entry per (word, mode) the ladder allows rather than per word:
    // which mode is due is as much of the answer as which word, since an
    // exercise is only evidence about its own mode. An attempted mode sorts
    // by its card's retrievability, a freshly unlocked one by its derived
    // first-attempt probability — see `WordState::due`.
    pub fn due(&self, timestamp: u64) -> Vec<(f64, &str, Mode)> {
        let mut due: Vec<(f64, &str, Mode)> = self
            .words
            .iter()
            .flat_map(|(word, state)| {
                state
                    .due(timestamp)
                    .map(|(r, mode)| (r, word.as_str(), mode))
            })
            .collect();
        due.sort_by(|a, b| a.0.total_cmp(&b.0));
        due
    }

    // One card for each encountered word, most urgent first. A word that is
    // still at Scaffolding gets the gentler recognition direction; once the
    // ladder permits typed work, its least-retrievable permitted typed mode
    // determines the direction. This makes every encountered word eligible
    // without asking a learner to produce a word before they are ready.
    pub fn flashcards(&self, timestamp: u64) -> Vec<(&str, FlashcardDirection)> {
        let mut cards: Vec<(f64, &str, FlashcardDirection)> = self
            .words
            .iter()
            .map(|(word, state)| {
                let mode = [Mode::Recognition, Mode::Production]
                    .into_iter()
                    .filter(|&mode| state.allows(mode))
                    .min_by(|&a, &b| {
                        state
                            .probability(a, timestamp)
                            .total_cmp(&state.probability(b, timestamp))
                    })
                    .unwrap_or(Mode::Recognition);
                let direction = FlashcardDirection::for_mode(mode)
                    .expect("recognition and production are flashcard modes");
                (state.probability(mode, timestamp), word.as_str(), direction)
            })
            .collect();
        cards.sort_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.1.cmp(b.1)));
        cards
            .into_iter()
            .map(|(_, word, direction)| (word, direction))
            .collect()
    }

    // Apply a finished standalone practice run. Flashcards never introduce a
    // word or advance the course tree: the whole submission is validated
    // against `words` before any card moves. Every verdict is real evidence
    // about the direction it tested, including recognition practice offered
    // to a word still at Scaffolding, so it goes through `record`. That method
    // records the mode's card while keeping a forbidden failure from moving
    // the ladder's counters.
    pub fn submit_flashcards(
        &mut self,
        course: &Course,
        reports: &[FlashcardReport],
        timestamp: u64,
    ) -> Result<FlashcardOutcome, FlashcardSubmissionError> {
        if reports.is_empty() {
            return Err(FlashcardSubmissionError::Empty);
        }
        let mut reported = HashSet::new();
        for report in reports {
            if !course.vocab.contains(&report.word) {
                return Err(FlashcardSubmissionError::UnknownWord(report.word.clone()));
            }
            if !self.words.contains_key(&report.word) {
                return Err(FlashcardSubmissionError::UnencounteredWord(
                    report.word.clone(),
                ));
            }
            if !reported.insert((report.word.as_str(), report.direction)) {
                return Err(FlashcardSubmissionError::RepeatedCard {
                    word: report.word.clone(),
                    direction: report.direction,
                });
            }
        }

        let correct = reports.iter().filter(|report| report.correct).count();
        for report in reports {
            self.words
                .get_mut(&report.word)
                .expect("the whole flashcard submission was validated")
                .record(report.direction.mode(), report.correct, timestamp);
        }
        self.demote_decayed(timestamp);
        Ok(FlashcardOutcome {
            correct,
            total: reports.len(),
        })
    }

    // --- what a lesson does to them ---

    // Apply one already-graded, self-describing question. No course content
    // has to be rebuilt to discover either its mode or its words. Words are
    // updated individually, so nearly getting a sentence right does not punish
    // the words the learner did produce.
    fn record_question(&mut self, question: &QuestionReport, timestamp: u64, test: bool) {
        let mode = question.ask.mode();
        for (word, &correct) in &question.words {
            let state = self
                .words
                .entry(word.clone())
                .or_insert_with(|| WordState::new(crate::word::Stage::introduced_by(mode)));
            if test {
                state.record_test(mode, correct, timestamp);
            } else {
                state.record(mode, correct, timestamp);
            }
        }
    }

    // The deep-forgetting sweep, run once a lesson's verdicts are in: every
    // word whose top-mode card has quietly decayed slides down a rung. This is
    // what makes "back from vacation" work without anyone answering anything —
    // see `WordState::demote_if_decayed`.
    //
    // Note what this does *not* touch: a learner's `progress`. Skills do not
    // rot. What decays is the memory the builder acts on, not the tree.
    fn demote_decayed(&mut self, timestamp: u64) {
        for state in self.words.values_mut() {
            state.demote_if_decayed(timestamp);
        }
    }

    // Apply a finished lesson to the user's memory, and move them along if it
    // moved them.
    //
    // Every exercise takes its verdict — that is what reviewing *is* — and
    // there are no exceptions any more. The old carve-out for scripted
    // exercises in a re-taken lesson existed because such an exercise sat
    // directly after the material that taught it, so getting it right proved
    // nothing. Material teaches nothing now, so every question is a real one.
    //
    // The client returns every question directly. There is no server-side
    // pending lesson to compare it with: the submission is the memory delta,
    // not an answer sheet keyed into mutable course content.
    pub fn submit_lesson(
        &mut self,
        course: &Course,
        submission: &Submission,
        timestamp: u64,
    ) -> Result<Outcome, SubmissionError> {
        self.validate_submission(course, submission)?;

        // what the user knows *before* the lesson, so that "new" means new to
        // them and not merely new to the course — a re-taken lesson introduces
        // nothing, because they were here the first time
        let known: HashSet<&str> = self.words.keys().map(String::as_str).collect();
        let mut learned: Vec<String> = submission
            .questions
            .iter()
            .flat_map(|question| question.words.keys())
            .filter(|word| !known.contains(word.as_str()))
            .cloned()
            .collect();
        // A JSON object has no meaningful key order. Make the profile record
        // deterministic rather than inheriting HashMap iteration randomness.
        learned.sort();

        let test = matches!(submission.target, Target::Castle(_));
        let mut correct = 0;
        for question in &submission.questions {
            if question.correct {
                correct += 1;
            }
            self.record_question(question, timestamp, test);
        }
        // with the verdicts in, words whose cards have quietly decayed past
        // the threshold slide down a rung — the deep-forgetting path
        self.demote_decayed(timestamp);

        let mut outcome = Outcome {
            correct,
            total: submission.questions.len(),
            learned,
            cleared_skill: None,
            passed: None,
        };
        match &submission.target {
            Target::Skill(position) => self.finish_lesson(course, position, &mut outcome),
            Target::Castle(castle) => self.finish_castle(*castle, &mut outcome),
        }
        Ok(outcome)
    }

    // Validate the whole delta before touching a card. A malformed submission
    // must be all-or-nothing: returning a 400 after half the words moved would
    // be worse than accepting it.
    fn validate_submission(
        &self,
        course: &Course,
        submission: &Submission,
    ) -> Result<(), SubmissionError> {
        if submission.questions.is_empty() {
            return Err(SubmissionError::Empty);
        }

        enum Scope<'a> {
            Skill {
                id: &'a str,
                words: &'a [String],
            },
            Castle {
                index: usize,
                words: HashSet<&'a str>,
            },
        }
        let scope = match &submission.target {
            Target::Skill(position) => {
                if !course.has_lesson(position) {
                    return Err(SubmissionError::NoSuchLesson(position.clone()));
                }
                if !self.may_take(course, position) {
                    return Err(SubmissionError::LessonLocked(position.clone()));
                }
                let skill = course
                    .skill(&position.skill)
                    .expect("has_lesson guarantees the skill exists");
                Scope::Skill {
                    id: &skill.id,
                    words: &skill.words,
                }
            }
            Target::Castle(castle) => {
                if course.castles().get(*castle).is_none() {
                    return Err(SubmissionError::NoSuchCastle(*castle));
                }
                if self.castle_due(course) != Some(*castle) {
                    return Err(SubmissionError::CastleLocked(*castle));
                }
                Scope::Castle {
                    index: *castle,
                    words: course.words_in_castle(*castle).into_iter().collect(),
                }
            }
        };

        let mut reported = HashSet::new();
        for question in &submission.questions {
            if question.words.is_empty() {
                return Err(SubmissionError::EmptyQuestion);
            }
            for word in question.words.keys() {
                if !course.vocab.contains(word) {
                    return Err(SubmissionError::UnknownWord(word.clone()));
                }
                if !reported.insert(word.as_str()) {
                    return Err(SubmissionError::RepeatedWord(word.clone()));
                }
                match &scope {
                    // Review holes may test words from earlier skills. Only a
                    // word entering memory now must belong to this lesson's
                    // skill; known words are legitimate review deltas.
                    Scope::Skill { id, words }
                        if !self.words.contains_key(word) && !words.contains(word) =>
                    {
                        return Err(SubmissionError::UnexpectedNewWord {
                            word: word.clone(),
                            skill: (*id).to_string(),
                        });
                    }
                    Scope::Castle { index, words } if !words.contains(word.as_str()) => {
                        return Err(SubmissionError::WordOutsideCastle {
                            word: word.clone(),
                            castle: *index,
                        });
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }

    // Finishing the lesson a learner is *on* moves them one further into the
    // skill; finishing one behind them changes nothing. They may re-take any
    // lesson they have reached, and going through it again must not move them
    // — and certainly not backwards.
    fn finish_lesson(&mut self, course: &Course, position: &Position, outcome: &mut Outcome) {
        let done = self.lessons_done(&position.skill);
        if position.lesson != done + 1 {
            return; // a re-take: they already unlocked what it unlocks
        }
        self.progress
            .insert(position.skill.clone(), position.lesson);
        // the last lesson of a skill clears it — a milestone the profile
        // records, and the thing that opens the next row
        if let Some(skill) = course.skill(&position.skill)
            && position.lesson >= SKILL_LESSONS
        {
            outcome.cleared_skill = Some(skill.name.clone());
        }
    }

    // A castle is passed on the score alone, and passing the one in front of
    // them is what lets a learner into the next stretch of the tree. Failing
    // costs nothing but a retry with a fresh sample — the pressure to review
    // comes from not passing, not from a penalty.
    fn finish_castle(&mut self, castle: usize, outcome: &mut Outcome) {
        let passed =
            outcome.total > 0 && outcome.correct as f64 / outcome.total as f64 >= CASTLE_PASS;
        outcome.passed = Some(passed);
        if passed && castle == self.castles {
            self.castles += 1;
        }
    }
}

// What a submitted lesson came to. The first two are the score the client
// gets back; the rest is what the day's record is made of (see profile.rs) —
// worked out here because the answers depend on what the user knew and where
// they stood a moment ago, which nothing outside this function can still see.
pub struct Outcome {
    pub correct: usize,
    pub total: usize,
    // the words the user met for the first time in this lesson, in the order
    // they met them; empty for a re-taken one
    pub learned: Vec<String>,
    // the name of the skill this lesson finished, if it finished one
    pub cleared_skill: Option<String>,
    // whether a castle was passed; None if this wasn't a castle
    pub passed: Option<bool>,
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::card::Card;
    use crate::course::tests::{course_of, sentence, skill, solo_sentences};
    use crate::exercise::Ask;
    use crate::lesson::{Lesson, Task};
    use crate::word::Stage;
    use std::time::{SystemTime, UNIX_EPOCH};

    pub const DAY: u64 = 86400;

    pub fn now() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }

    fn question(ask: Ask, words: &[(&str, bool)]) -> QuestionReport {
        QuestionReport {
            ask,
            correct: words.iter().all(|(_, correct)| *correct),
            words: words
                .iter()
                .map(|(word, correct)| ((*word).to_string(), *correct))
                .collect(),
        }
    }

    // A user who reviewed each given word `days_ago` days before now, so the
    // first word is the most urgently due. Each word is all the way up the
    // ladder (RecognitionProduction), so every typed question about it is
    // legal.
    //
    // **All three cards, not just the top one.** A word cannot reach the top
    // of the ladder without having been recognized and tapped along the way,
    // and a word with only a production card is a state the system never
    // produces: its recognition mode would look unattempted and so maximally
    // urgent, which would quietly aim every builder test at the wrong
    // question.
    pub fn user_with_reviews(words_days: &[(&str, u64)]) -> User {
        let mut user = User::new();
        let timestamp = now();
        for &(word, days_ago) in words_days {
            let card = Card::new().good(timestamp - days_ago * DAY);
            let mut state = WordState::at(Stage::RecognitionProduction, card);
            state.bank = Some(card);
            state.recognition = Some(card);
            user.words.insert(word.to_string(), state);
        }
        user
    }

    // --- recording a submitted question ---

    // there is no other way into a user's memory: material teaches nothing,
    // so a word exists because it was answered
    #[test]
    fn recording_creates_a_card_for_a_new_word() {
        let mut user = User::new();
        let timestamp = now();
        user.record_question(
            &question(Ask::WriteTarget, &[("hola", true)]),
            timestamp,
            false,
        );
        let card = user.words["hola"].production.unwrap();
        assert_eq!(card.last_reviewed, timestamp);
        // just answered correctly, so recall is (near-)certain right now
        assert!(card.retrievability(timestamp) > 0.99);
    }

    // the sort of exercise that introduces a word decides where it starts on
    // the ladder; in the shipped course that is always a word bank
    #[test]
    fn the_introducing_exercises_mode_sets_the_initial_stage() {
        let mut user = User::new();
        let timestamp = now();
        user.record_question(
            &question(Ask::BuildSource, &[("a", true)]),
            timestamp,
            false,
        );
        user.record_question(
            &question(Ask::WriteSource, &[("b", true)]),
            timestamp,
            false,
        );
        assert_eq!(user.words["a"].stage, Stage::Scaffolding);
        assert!(user.words["a"].bank.is_some());
        assert_eq!(user.words["b"].stage, Stage::Recognition);
        assert!(user.words["b"].bank.is_none());
    }

    // half a sentence right means half its words are right: the words the
    // user did produce shouldn't be punished for the ones they didn't
    #[test]
    fn recording_follows_the_per_word_verdicts() {
        let mut user = User::new();
        user.record_question(
            &question(Ask::WriteTarget, &[("a", true), ("b", false)]),
            now(),
            false,
        );
        assert!(
            user.words["a"].production.unwrap().stability
                > user.words["b"].production.unwrap().stability
        );
    }

    // The sweep runs over every word the user has, not just the ones a lesson
    // happened to touch. A word untouched for a year has lost both of its
    // typed cards, so it slides all the way to the bottom — a rung per decayed
    // card, as `WordState::demote_if_decayed` has it.
    #[test]
    fn the_decay_sweep_covers_every_word() {
        let mut user = user_with_reviews(&[("old", 365), ("fresh", 0)]);
        user.demote_decayed(now());
        assert_eq!(user.words["old"].stage, Stage::Scaffolding);
        assert_eq!(user.words["fresh"].stage, Stage::RecognitionProduction);
    }

    // --- what the builder asks ---

    // a sentence drill is only as legal as its weakest word: "never
    // production first" can't leak through a multi-word exercise
    #[test]
    fn the_legality_gate_is_all_or_nothing() {
        let mut user = user_with_reviews(&[("a", 3)]);
        user.words
            .insert("b".to_string(), WordState::scaffolded(now()));
        let words = |list: &[&str]| -> Vec<String> { list.iter().map(|w| w.to_string()).collect() };
        assert!(user.allows(&words(&["a"]), Mode::Production));
        assert!(!user.allows(&words(&["a", "b"]), Mode::Production));
        assert!(user.allows(&words(&["b"]), Mode::Scaffolding));
    }

    // ...and a word the user has never met allows *nothing*, not even a word
    // bank. In a branching tree plenty of reachable material is material this
    // learner has never seen — a row-mate skill they skipped, or a word from
    // later in the very skill they are re-taking.
    #[test]
    fn an_unmet_word_allows_nothing() {
        let user = user_with_reviews(&[("a", 3)]);
        let unknown = vec!["unknown".to_string()];
        assert!(!user.allows(&unknown, Mode::Scaffolding));
        assert!(!user.allows(&["a".to_string(), "unknown".to_string()], Mode::Production));
    }

    // an exercise is as likely as the product of its words, and a word the
    // user has never studied takes the whole thing to zero
    #[test]
    fn an_exercises_probability_is_the_product_of_its_words() {
        let user = user_with_reviews(&[("a", 1), ("b", 1)]);
        let timestamp = now();
        let of = |list: &[&str]| {
            let words: Vec<String> = list.iter().map(|w| w.to_string()).collect();
            user.probability(&words, Mode::Production, timestamp)
        };
        assert!((of(&["a", "b"]) - of(&["a"]) * of(&["b"])).abs() < 1e-9);
        assert_eq!(of(&["a", "never"]), 0.0);
    }

    // the urgency list is sorted, and covers one entry per mode the ladder
    // allows — cards by retrievability, unattempted modes by derivation
    #[test]
    fn due_lists_the_most_urgent_card_first() {
        let user = user_with_reviews(&[("fresh", 1), ("stale", 60)]);
        let due = user.due(now());
        // the fixture's words sit at the top of the ladder with only a
        // production card, so each also yields a derived recognition entry
        // (derived from a bank card it doesn't have: 0.0, front of list) —
        // filter down to the real cards to compare them
        let production: Vec<&(f64, &str, Mode)> = due
            .iter()
            .filter(|(_, _, mode)| *mode == Mode::Production)
            .collect();
        assert_eq!(production.len(), 2);
        assert_eq!(production[0].1, "stale");
        assert!(production[0].0 < production[1].0);
    }

    // --- the shape of the tree ---

    // two skills side by side in row 0, one behind them in row 1
    fn branching_course() -> Course {
        course_of(
            vec![
                skill("left", 0, 0, &["a", "b"], 2),
                skill("right", 0, 0, &["c"], 1),
                skill("later", 1, 0, &["d"], 1),
            ],
            Vec::new(),
        )
    }

    #[test]
    fn the_first_row_is_open_and_the_rest_are_not() {
        let course = branching_course();
        let user = User::new();
        assert_eq!(user.skill_state(&course, "left"), SkillState::Available);
        assert_eq!(user.skill_state(&course, "right"), SkillState::Available);
        assert_eq!(user.skill_state(&course, "later"), SkillState::Locked);
    }

    #[test]
    fn level_progress_is_six_lessons_per_level() {
        let mut user = User::new();
        user.progress.insert("skill".to_string(), 11);
        assert_eq!(user.skill_level("skill"), 1);
        assert_eq!(user.lessons_done_in_level("skill"), 5);
        assert!(!user.has_row_level("skill"));

        user.progress.insert("skill".to_string(), 12);
        assert_eq!(user.skill_level("skill"), 2);
        assert_eq!(user.lessons_done_in_level("skill"), 0);
        assert!(user.has_row_level("skill"));

        user.progress.insert("skill".to_string(), 36);
        assert_eq!(user.skill_level("skill"), 5);
        assert_eq!(user.lessons_done_in_level("skill"), 6);
    }

    // a row opens when *every* skill in the row before it reaches level 2 — one
    // of two side-by-side skills is not enough
    #[test]
    fn a_row_opens_only_once_all_of_the_previous_one_is_done() {
        let course = branching_course();
        let mut user = User::new();
        user.progress.insert("left".to_string(), 12);
        assert_eq!(user.skill_state(&course, "left"), SkillState::Available);
        assert_eq!(user.skill_state(&course, "later"), SkillState::Locked);
        user.progress.insert("right".to_string(), 12);
        assert_eq!(user.skill_state(&course, "later"), SkillState::Available);
    }

    // a learner may re-take anything they have reached, take the next lesson
    // of an open skill, and nothing beyond that
    #[test]
    fn a_learner_may_take_what_they_have_reached_and_one_more() {
        let course = branching_course();
        let mut user = User::new();
        user.progress.insert("left".to_string(), 1);
        assert!(user.may_take(&course, &Position::new("left", 1))); // re-take
        assert!(user.may_take(&course, &Position::new("left", 2))); // next
        assert!(!user.may_take(&course, &Position::new("left", 3))); // no such lesson
        assert!(!user.may_take(&course, &Position::new("later", 1))); // locked row
    }

    #[test]
    fn continue_goes_to_the_first_unfinished_open_lesson() {
        let course = branching_course();
        let mut user = User::new();
        assert_eq!(user.next_lesson(&course), Some(Position::new("left", 1)));
        user.progress.insert("left".to_string(), 2);
        assert_eq!(user.next_lesson(&course), Some(Position::new("left", 3)));
        user.progress.insert("left".to_string(), 12);
        assert_eq!(user.next_lesson(&course), Some(Position::new("right", 1)));
    }

    // --- castles ---

    // a course of two castles: rows 0-1 behind the first, row 2 behind the
    // second
    fn castled_course() -> Course {
        use crate::course::tests::{rows_of, vocab_of};
        use crate::skill::Castle;
        let skills = vec![
            skill("one", 0, 0, &["a"], 1),
            skill("two", 1, 0, &["b"], 1),
            skill("three", 2, 1, &["c"], 1),
        ];
        let rows = rows_of(&skills);
        Course::new(
            "test".to_string(),
            "en".to_string(),
            "es".to_string(),
            vocab_of(&["a", "b", "c"]),
            Vec::new(),
            skills,
            rows,
            vec![
                Castle {
                    castle: 0,
                    rows: 0..2,
                },
                Castle {
                    castle: 1,
                    rows: 2..3,
                },
            ],
        )
    }

    // the castle is due once its whole stretch is finished, and not before
    #[test]
    fn a_castle_comes_due_when_its_rows_are_done() {
        let course = castled_course();
        let mut user = User::new();
        assert_eq!(user.castle_due(&course), None);
        user.progress.insert("one".to_string(), 12);
        assert_eq!(user.castle_due(&course), None);
        user.progress.insert("two".to_string(), 12);
        assert_eq!(user.castle_due(&course), Some(0));
    }

    // and it gates the row behind it: finishing the stretch is not enough,
    // the test has to be passed
    #[test]
    fn a_castle_gates_the_next_stretch() {
        let course = castled_course();
        let mut user = User::new();
        user.progress.insert("one".to_string(), 12);
        user.progress.insert("two".to_string(), 12);
        assert_eq!(user.skill_state(&course, "three"), SkillState::Locked);
        assert_eq!(user.next_lesson(&course), None); // nothing but the castle
        user.castles = 1;
        assert_eq!(user.skill_state(&course, "three"), SkillState::Available);
    }

    // --- submitting a lesson ---

    fn lesson(target: Target, tasks: Vec<Task>) -> Lesson {
        Lesson {
            username: "sam".to_string(),
            target,
            tasks,
        }
    }

    fn one_sentence_course() -> Course {
        course_of(
            vec![skill("one", 0, 0, &["a", "b"], 2)],
            vec![sentence("s_a", &["a"], 0)],
        )
    }

    fn intro_task() -> Task {
        Task::Exercise {
            sentence: 0,
            ask: Ask::INTRODUCTION,
            introduces: vec!["a".to_string()],
        }
    }

    // What the client sends back, with the first `right` questions correct.
    // Resolving the transient lesson here mirrors the client receiving it;
    // the resulting submission contains no sentence pointers.
    fn answered(course: &Course, lesson: &Lesson, right: usize) -> Submission {
        let questions = lesson
            .tasks
            .iter()
            .filter_map(|task| task.exercise(course))
            .enumerate()
            .map(|(index, exercise)| {
                let correct = index < right;
                QuestionReport {
                    ask: exercise.ask,
                    correct,
                    words: exercise
                        .words
                        .into_iter()
                        .map(|word| (word, correct))
                        .collect(),
                }
            })
            .collect();
        Submission {
            target: lesson.target.clone(),
            questions,
        }
    }

    #[test]
    fn submitting_the_next_lesson_advances_the_skill() {
        let course = one_sentence_course();
        let mut user = User::new();
        let lesson = lesson(Target::Skill(Position::new("one", 1)), vec![intro_task()]);
        let submission = answered(&course, &lesson, 1);
        let outcome = user.submit_lesson(&course, &submission, now()).unwrap();
        assert_eq!(user.lessons_done("one"), 1);
        assert_eq!(outcome.learned, ["a"]);
        assert!(user.words.contains_key("a")); // answering is what teaches it
        assert!(outcome.cleared_skill.is_none()); // one of two lessons
    }

    // re-taking a lesson reviews it — every question is a real question now —
    // but must not move the learner, and introduces nothing they already know
    #[test]
    fn retaking_a_lesson_reviews_without_moving_the_learner() {
        let course = one_sentence_course();
        let mut user = User::new();
        user.progress.insert("one".to_string(), 2);
        user.words
            .insert("a".to_string(), WordState::scaffolded(now() - 3 * DAY));
        let before = user.words["a"].bank.unwrap();
        let lesson = lesson(Target::Skill(Position::new("one", 1)), vec![intro_task()]);
        let submission = answered(&course, &lesson, 1);
        let timestamp = now();
        let outcome = user.submit_lesson(&course, &submission, timestamp).unwrap();
        assert_eq!(user.lessons_done("one"), 2); // not moved, not reset
        assert!(outcome.learned.is_empty());
        assert!(outcome.cleared_skill.is_none());
        // the review counted: it is an honest question, not filler after a tip
        assert!(user.words["a"].bank.unwrap().last_reviewed > before.last_reviewed);
        assert_eq!(outcome.total, 1);
    }

    #[test]
    fn the_last_lesson_of_a_skill_clears_it() {
        let course = one_sentence_course();
        let mut user = User::new();
        user.progress.insert("one".to_string(), SKILL_LESSONS - 1);
        let lesson = lesson(
            Target::Skill(Position::new("one", SKILL_LESSONS)),
            vec![intro_task()],
        );
        let submission = answered(&course, &lesson, 0);
        let outcome = user.submit_lesson(&course, &submission, now()).unwrap();
        assert_eq!(outcome.cleared_skill.as_deref(), Some("one"));
        assert!(user.has_completed("one", &course));
    }

    // With no pending lesson on the server, an empty object could otherwise
    // advance a skill without changing memory. Refuse it before progress moves.
    #[test]
    fn an_empty_submission_does_not_advance() {
        let course = one_sentence_course();
        let mut user = User::new();
        let submission = Submission {
            target: Target::Skill(Position::new("one", 1)),
            questions: Vec::new(),
        };
        assert!(matches!(
            user.submit_lesson(&course, &submission, now()),
            Err(SubmissionError::Empty)
        ));
        assert_eq!(user.lessons_done("one"), 0);
    }

    // Generated review holes draw on earlier/reached skills, not only on the
    // skill whose lesson advances. A self-describing submission must preserve
    // that real lesson shape while still preventing arbitrary introductions.
    #[test]
    fn a_known_word_from_another_skill_is_a_valid_review() {
        let course = branching_course();
        let mut user = User::new();
        user.words
            .insert("c".to_string(), WordState::scaffolded(now() - DAY));
        let before = user.words["c"].bank.unwrap().last_reviewed;
        let submission = Submission {
            target: Target::Skill(Position::new("left", 1)),
            questions: vec![question(Ask::BuildSource, &[("c", true)])],
        };
        user.submit_lesson(&course, &submission, now()).unwrap();
        assert!(user.words["c"].bank.unwrap().last_reviewed > before);
        assert_eq!(user.lessons_done("left"), 1);
    }

    #[test]
    fn another_skills_word_cannot_be_introduced_by_the_submission() {
        let course = branching_course();
        let mut user = User::new();
        let submission = Submission {
            target: Target::Skill(Position::new("left", 1)),
            questions: vec![question(Ask::BuildSource, &[("c", true)])],
        };
        assert!(matches!(
            user.submit_lesson(&course, &submission, now()),
            Err(SubmissionError::UnexpectedNewWord { word, skill })
                if word == "c" && skill == "left"
        ));
        assert!(user.words.is_empty());
        assert_eq!(user.lessons_done("left"), 0);
    }

    // The builder's no-repeated-words rule is what makes its probability
    // arithmetic honest. The client-returned delta must not bypass it by
    // updating one card twice in a single lesson.
    #[test]
    fn a_repeated_word_rejects_the_whole_submission() {
        let course = one_sentence_course();
        let mut user = User::new();
        let submission = Submission {
            target: Target::Skill(Position::new("one", 1)),
            questions: vec![
                question(Ask::BuildSource, &[("a", true)]),
                question(Ask::BuildTarget, &[("a", false)]),
            ],
        };
        assert!(matches!(
            user.submit_lesson(&course, &submission, now()),
            Err(SubmissionError::RepeatedWord(word)) if word == "a"
        ));
        assert!(user.words.is_empty());
    }

    // --- standalone flashcards ---

    #[test]
    fn flashcard_direction_updates_only_its_own_memory() {
        let course = course_of(
            vec![skill("one", 0, 0, &["a", "b"], 1)],
            solo_sentences(&["a", "b"], 0),
        );
        let mut user = User::new();
        user.words
            .insert("a".to_string(), WordState::scaffolded(now() - DAY));
        user.words
            .insert("b".to_string(), WordState::scaffolded(now() - DAY));
        let reports = [
            FlashcardReport {
                word: "a".to_string(),
                direction: FlashcardDirection::TargetToSource,
                correct: false,
            },
            FlashcardReport {
                word: "b".to_string(),
                direction: FlashcardDirection::SourceToTarget,
                correct: true,
            },
        ];

        let outcome = user.submit_flashcards(&course, &reports, now()).unwrap();
        assert_eq!(outcome.correct, 1);
        assert!(user.words["a"].recognition.is_some());
        assert!(user.words["a"].production.is_none());
        assert_eq!(user.words["a"].stage, Stage::Scaffolding);
        assert!(user.words["b"].recognition.is_none());
        assert!(user.words["b"].production.is_some());
    }

    #[test]
    fn an_unencountered_flashcard_rejects_the_whole_submission() {
        let course = course_of(
            vec![skill("one", 0, 0, &["a", "b"], 1)],
            solo_sentences(&["a", "b"], 0),
        );
        let mut user = User::new();
        user.words
            .insert("a".to_string(), WordState::scaffolded(now() - DAY));
        let reports = [
            FlashcardReport {
                word: "a".to_string(),
                direction: FlashcardDirection::TargetToSource,
                correct: true,
            },
            FlashcardReport {
                word: "b".to_string(),
                direction: FlashcardDirection::TargetToSource,
                correct: true,
            },
        ];

        assert!(matches!(
            user.submit_flashcards(&course, &reports, now()),
            Err(FlashcardSubmissionError::UnencounteredWord(word)) if word == "b"
        ));
        assert!(user.words["a"].recognition.is_none());
        assert!(!user.words.contains_key("b"));
    }

    // --- submitting a castle ---

    // a castle asks for production, which is the whole point of it: the
    // ladder is bypassed on the way in
    fn castle_lesson(sentences: &[usize]) -> Lesson {
        lesson(
            Target::Castle(0),
            sentences
                .iter()
                .map(|&sentence| Task::Exercise {
                    sentence,
                    ask: Ask::WriteTarget,
                    introduces: Vec::new(),
                })
                .collect(),
        )
    }

    fn five_question_course() -> Course {
        course_of(
            vec![skill("one", 0, 0, &["a", "b", "c", "d", "e"], 1)],
            solo_sentences(&["a", "b", "c", "d", "e"], 0),
        )
    }

    #[test]
    fn passing_a_castle_unlocks_the_next_stretch() {
        let course = five_question_course();
        let mut user = user_with_reviews(&[("a", 1), ("b", 1), ("c", 1), ("d", 1), ("e", 1)]);
        user.progress.insert("one".to_string(), 12);
        let lesson = castle_lesson(&[0, 1, 2, 3, 4]);
        // 4 of 5 is exactly the 80% threshold
        let submission = answered(&course, &lesson, 4);
        let outcome = user.submit_lesson(&course, &submission, now()).unwrap();
        assert_eq!(outcome.passed, Some(true));
        assert_eq!(user.castles, 1);
    }

    // failing costs nothing but a retry: the pressure to review comes from
    // not passing, not from a penalty
    #[test]
    fn failing_a_castle_leaves_the_learner_where_they_were() {
        let course = five_question_course();
        let mut user = user_with_reviews(&[("a", 1), ("b", 1), ("c", 1), ("d", 1), ("e", 1)]);
        user.progress.insert("one".to_string(), 12);
        let lesson = castle_lesson(&[0, 1, 2, 3, 4]);
        let submission = answered(&course, &lesson, 2);
        let outcome = user.submit_lesson(&course, &submission, now()).unwrap();
        assert_eq!(outcome.passed, Some(false));
        assert_eq!(user.castles, 0);
    }

    // a castle asks for production whatever stage a word is at, and a wrong
    // answer above the ladder must leave no trace at all
    #[test]
    fn a_castle_never_punishes_a_word_above_its_stage() {
        let course = five_question_course();
        let mut user = User::new();
        for word in ["a", "b", "c", "d", "e"] {
            user.words
                .insert(word.to_string(), WordState::scaffolded(now()));
        }
        user.progress.insert("one".to_string(), 12);
        let lesson = castle_lesson(&[0, 1, 2, 3, 4]);
        let submission = answered(&course, &lesson, 0);
        user.submit_lesson(&course, &submission, now()).unwrap();
        for word in ["a", "b", "c", "d", "e"] {
            let state = &user.words[word];
            assert!(state.production.is_none(), "{word} grew a production card");
            assert_eq!(state.stage, Stage::Scaffolding);
        }
    }
}
