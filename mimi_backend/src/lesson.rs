// A lesson: the tasks a user is asked to work through, and the algorithms that
// choose them.
//
// There are two, and they are deliberately different. An ordinary lesson is
// **introduction plus review**: the skill's word list is an introduction
// queue. Its first lesson front-loads every still-new word that fits; later
// lessons retain the authored introduction schedule, and everything else is
// filled by difficulty-targeting the user's own memory. A **castle** is a
// test, and a test targeted at 85% is by construction a test everyone passes
// at 85% — so it samples evenly across a whole stretch of the tree instead,
// and ignores the ladder on the way in.
//
// Nothing here is authored. There is no pattern language and no scripted
// slot: a lesson is `material -> introductions -> holes`, and the only thing
// the course data decides is which words the skill teaches and in what order.
//
// Once built, a lesson is immediately resolved and served to the client
// (answers included) via `POST /me/lessons`. It is not stored.
// The client later returns a self-describing per-word memory delta to
// `POST /me/lessons/submit`, applied by `User::submit_lesson`.

use std::collections::HashSet;
use std::time::{Duration, SystemTime};

use rand::prelude::IndexedRandom;
use rand::seq::SliceRandom;

use crate::course::Course;
use crate::exercise::{Ask, Exercise, Mode};
use crate::position::Position;
use crate::sentence::{Mark, Sentence};
use crate::skill::MaterialBlock;
use crate::user::User;

// how many exercises go in one lesson, introductions included
const LESSON_SIZE: usize = 10;
// the fraction of exercises we want the user to get right
const TARGET: f64 = 0.85;
// while filling the lesson, if the REMAINING exercises would need an average
// success probability above this, stop taking hard (urgent) exercises
const MAX_NEEDED: f64 = 0.925;
// always take at least this many urgent words, even if it makes the lesson hard
const MIN_URGENT: usize = 2;
// how many questions a castle asks. Fixed, and far smaller than the number of
// words behind it — which is what makes it a test rather than a recital.
const CASTLE_SIZE: usize = 20;

pub struct Lesson {
    // the user this lesson was built for
    pub username: String,
    pub target: Target,
    // the tasks of the lesson, in the order they are to be done
    pub tasks: Vec<Task>,
}

// What this lesson *is*. The two take different paths on the way in (they are
// built by different algorithms) and on the way out (a castle is passed or
// failed; a lesson moves you along a skill).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    // one lesson of one skill. Any lesson a learner has reached is fair game:
    // re-taking an old one is how you review it deliberately.
    Skill(Position),
    // the test at the end of a stretch of rows, by castle index
    Castle(usize),
}

// One task of a served lesson: a tip to read, or a question to answer.
//
// Only a short-lived pointer — into a skill's material, or at a sentence
// together with the way it is to be asked. The task is resolved against the
// same `Arc<Course>` immediately before the response is serialized, then
// discarded. It never crosses a request or enters SQLite, so a later course
// generation may safely arrange its sentence vector differently.
//
// A sentence alone would not be enough: the same sentence is four different
// questions, and which one this task means is the builder's decision, not
// something to re-derive while materializing the response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Task {
    Material {
        skill: String,
        index: usize,
    },
    Exercise {
        // an index into `Course::sentences`
        sentence: usize,
        // which of the four questions that sentence is being asked as
        ask: Ask,
        // the words the learner meets for the first time here. Answering is
        // what creates their cards — there is no other way in, because
        // material teaches nothing — so this is only so the client can mark
        // them as new and show what they mean.
        introduces: Vec<String>,
    },
}

impl Task {
    // the tip this task shows, if it's a material task
    pub fn material<'a>(&self, course: &'a Course) -> Option<&'a MaterialBlock> {
        match self {
            Task::Material { skill, index } => course.skill(skill)?.material.get(*index),
            Task::Exercise { .. } => None,
        }
    }

    // The question this task asks, if it's an exercise task — built here and
    // now, because nothing stores one (see course.rs).
    pub fn exercise(&self, course: &Course) -> Option<Exercise> {
        match self {
            Task::Exercise { sentence, ask, .. } => course.exercise(*sentence, *ask),
            Task::Material { .. } => None,
        }
    }

    pub fn introduces(&self) -> &[String] {
        match self {
            Task::Exercise { introduces, .. } => introduces,
            Task::Material { .. } => &[],
        }
    }

    // Dedicated presentation spans for the words this first-contact task
    // announces. They come from the sentence's preferred target wording,
    // because every introduction is `BuildSource` and therefore shows that
    // side. Review tasks have no new words and no spans.
    pub fn new_words(&self, course: &Course) -> Option<Vec<Mark>> {
        match self {
            Task::Exercise {
                sentence,
                ask,
                introduces,
            } => {
                if introduces.is_empty() {
                    return Some(Vec::new());
                }
                if *ask != Ask::INTRODUCTION {
                    return None;
                }
                course.sentences.get(*sentence)?.new_word_marks(introduces)
            }
            Task::Material { .. } => Some(Vec::new()),
        }
    }
}

impl Lesson {
    // Build the lesson at `position` for this user: the skill's tips for that
    // lesson, a word bank for each word it introduces, and review chosen for
    // whatever is left.
    //
    // `position` is not necessarily the lesson the user is next due: they may
    // re-take any lesson they have reached. Everything is relative to the
    // lesson's *skill*, not to the user — the holes draw only on rows at or
    // before it, so re-doing an early skill has no business confronting the
    // user with a later one's vocabulary, however well they now know it.
    // Anything they haven't reached is locked: None.
    pub fn build(
        course: &Course,
        user: &User,
        username: &str,
        position: &Position,
    ) -> Option<Lesson> {
        if !user.may_take(course, position) {
            return None;
        }
        let skill = course.skill(&position.skill)?;
        let mut tasks: Vec<Task> = skill
            .material_for_lesson(position.lesson)
            .map(|(index, _)| Task::Material {
                skill: skill.id.clone(),
                index,
            })
            .collect();

        // The introductions. A word's first contact is the gentlest sentence
        // that uses it, asked with tiles (`Ask::INTRODUCTION`), and it is what
        // brings the word into memory: it is asked, not told, so the first
        // card is rated by an answer the learner actually gave. On the first
        // lesson, walk the whole skill rather than its normal per-lesson
        // slice: new vocabulary owns every available exercise slot before
        // urgency or difficulty targeting gets a say.
        //
        // **An introduction may bring in more than one word.** Where the course
        // never uses a word by itself, its gentlest sentence grades its
        // neighbours too and one answer meets all of them — so `introduces`
        // names every one it is a first contact for, and the queue skips
        // whatever an earlier introduction has already covered. That skip is
        // load-bearing rather than tidy: without it the same word would be
        // introduced twice in one lesson, and a lesson grading a word twice is
        // what the whole 85% arithmetic assumes cannot happen (see `Builder`).
        let mut used: HashSet<&str> = HashSet::new();
        let introductions = if position.lesson == 1 {
            skill.words.as_slice()
        } else {
            skill.words_for_lesson(position.lesson)
        };
        let mut introduction_count = 0;
        for word in introductions {
            if introduction_count == LESSON_SIZE {
                break;
            }
            // A re-take reviews words it introduced the first time; it does
            // not present them as first contacts again or reserve them from
            // the personalized review section. `used` covers the same word
            // reached a second way: an introduction earlier in this lesson.
            if user.words.contains_key(word) || used.contains(word.as_str()) {
                continue;
            }
            let Some(sentence) = course.introduction_for(word) else {
                continue; // no sentence uses it; the loader rejects this
            };
            let words = &course.sentences[sentence].words;
            // Every word of it the learner has not met is being met now. The
            // ones they have are ordinary review riding along, and are not
            // announced as new.
            let introduces: Vec<String> = words
                .iter()
                .filter(|word| !user.words.contains_key(*word))
                .cloned()
                .collect();
            used.extend(words.iter().map(String::as_str));
            tasks.push(Task::Exercise {
                sentence,
                ask: Ask::INTRODUCTION,
                introduces,
            });
            introduction_count += 1;
        }

        // ...and the rest is review. The holes are for review and only
        // review: whatever the lesson is teaching right now is spoken for
        // before we start, so a hole can't fetch another exercise for it.
        let holes = LESSON_SIZE.saturating_sub(
            tasks
                .iter()
                .filter(|t| matches!(t, Task::Exercise { .. }))
                .count(),
        );
        let picks = Builder::new(course, user, skill.row, used, holes).fill();
        tasks.extend(picks.into_iter().map(|(sentence, ask)| Task::Exercise {
            sentence,
            ask,
            introduces: Vec::new(),
        }));

        Some(Lesson {
            username: username.to_string(),
            target: Target::Skill(position.clone()),
            tasks,
        })
    }

    // Build the castle test the user is up against.
    //
    // A different algorithm from the one above, on purpose:
    //
    //   - **Even sampling, not urgency.** A castle asks about the stretch as a
    //     whole, the way a teacher samples a term's material, rather than
    //     hunting for the learner's weak spots. A test aimed at what you are
    //     worst at is a better diagnostic and a worse test.
    //   - **No 85% targeting.** The whole point is to find out.
    //   - **Recognition and production only.** No word banks: a bank is
    //     guessable enough that passing one proves little.
    //   - **The ladder is bypassed.** A castle asks for production whatever
    //     stage a word has reached, because a test restricted to what the
    //     ladder already permits would not be testing the thing worth testing.
    //     `WordState::record_test` is what keeps that fair on the way out.
    //
    // Every word in the stretch has certainly been *met*: reaching the castle
    // means completing every skill behind it, and completing a skill
    // introduces all of its words.
    pub fn castle(course: &Course, user: &User, username: &str, castle: usize) -> Option<Lesson> {
        if user.castle_due(course) != Some(castle) {
            return None;
        }
        let mut words = course.words_in_castle(castle);
        words.shuffle(&mut rand::rng());
        let mut used: HashSet<&str> = HashSet::new();
        let mut tasks: Vec<Task> = Vec::new();
        for word in words {
            if tasks.len() == CASTLE_SIZE {
                break;
            }
            if used.contains(word) {
                continue; // a sentence we already took covers this one
            }
            // both typed questions are fair game, and which one a sentence is
            // asked as is part of the sample: a castle that always chose the
            // same way round would test half of what it means to know a word
            let candidates: Vec<(usize, Ask)> = course
                .sentences_with(word)
                .iter()
                .flat_map(|&i| Ask::ALL.map(|ask| (i, ask)))
                .filter(|&(i, ask)| {
                    let sentence = &course.sentences[i];
                    ask.mode() != Mode::Scaffolding
                        && sentence.words.iter().all(|w| user.words.contains_key(w))
                        && !sentence.words.iter().any(|w| used.contains(w.as_str()))
                })
                .collect();
            let Some(&(sentence, ask)) = candidates.choose(&mut rand::rng()) else {
                continue;
            };
            used.extend(course.sentences[sentence].words.iter().map(String::as_str));
            tasks.push(Task::Exercise {
                sentence,
                ask,
                introduces: Vec::new(),
            });
        }
        (!tasks.is_empty()).then(|| Lesson {
            username: username.to_string(),
            target: Target::Castle(castle),
            tasks,
        })
    }
}

// --- filling the holes ---
//
// This is the part that does not change. The three-mode ladder, the urgency
// walk and the 85% targeting are exactly what they were when a lesson was
// half-authored; all that changed is how a lesson says which exercises it may
// draw on.
//
// A **candidate** is now a sentence together with one of the four ways of
// asking it, and that pairing is where bidirectionality earns its keep: the
// same sentence is a word bank for a word at the bottom of the ladder, a
// recognition drill once it has climbed, and a production drill at the top,
// and nothing has to be authored four times for that to be true. No exercise
// is built here at all — a candidate's difficulty is a question about the
// sentence's *words* and the ask's *mode*, and both are known without
// assembling a single string.
//
// The state of one lesson's review section. Everything the algorithm needs to
// remember as it goes — how many holes there are, which words are spoken for,
// what it has picked and how hard those picks are — is a field here, so each
// phase below is a method that reads and updates the lesson in progress.
//
// Our strategy is pretty simple. We want the user to review the words that
// they most need to review. BUT we don't want to make it too hard for the
// user, because then they will get frustrated and quit. So our target is 85%
// correct (see "The Eighty-Five Percent Rule for Optimal Learning").
//
// Two filters decide what a hole may hold at all, before any arithmetic: the
// era rule (nothing past the lesson's own row) and the gate in `User::allows`
// (every word the exercise grades must have been met, and must allow the
// exercise's mode at its stage — the ladder decides *whether* a mode is
// served, FSRS decides *when*).
struct Builder<'a> {
    course: &'a Course,
    user: &'a User,
    row: usize,
    timestamp: u64,
    // how many exercises the review section has room for
    holes: usize,
    // words already spoken for, and so not to be asked about again
    used: HashSet<&'a str>,
    // what we have chosen: an index into `course.sentences` and the way that
    // sentence is to be asked
    picks: Vec<(usize, Ask)>,
    // the summed success probability of `picks`, for the running average
    total: f64,
}

impl<'a> Builder<'a> {
    fn new(
        course: &'a Course,
        user: &'a User,
        row: usize,
        used: HashSet<&'a str>,
        holes: usize,
    ) -> Builder<'a> {
        Builder {
            course,
            user,
            row,
            timestamp: SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or(Duration::ZERO)
                .as_secs(),
            holes,
            used,
            picks: Vec::new(),
            total: 0.0,
        }
    }

    // fill the holes, order the result, and hand back the picks
    fn fill(mut self) -> Vec<(usize, Ask)> {
        if self.holes == 0 {
            return Vec::new();
        }
        self.take_urgent();
        self.top_up();
        self.order();
        self.picks
    }

    // PHASE 1: take exercises for the most urgent modes, until the lesson
    // would become too hard.
    //
    // For each mode that needs serving, starting with the most urgent
    // (lowest success probability), we try to pick an exercise for it that
    // is as close to 85% success probability as possible — an exercise *of
    // that mode*, since a word with a fresh recognition card and a decayed
    // production card must be served a production exercise or its production
    // decay would never be addressed. Two sorts of urgency walk this list
    // together: a card that has decayed (sorted by retrievability) and a
    // freshly unlocked mode with no card yet (sorted by its derived
    // first-attempt probability) — serving the latter is the only way its
    // card is ever born.
    //
    // While we do this we keep track of a number: given the exercises chosen
    // so far, what average success probability will the REST of them need in
    // order to reach our 85% target? Once we have taken `MIN_URGENT` words, if
    // that average is above 92.5% we stop taking urgent ones. Although we
    // could still theoretically meet the target by putting a bunch of
    // extremely hard exercises next to super easy ones, that is not beneficial
    // for learning.
    fn take_urgent(&mut self) {
        for (_, word, mode) in self.user.due(self.timestamp) {
            if self.picks.len() == self.holes {
                break;
            }
            if self.used.contains(word) {
                // already asked by an exercise we picked for an earlier card
                continue;
            }
            if self.picks.len() >= MIN_URGENT && self.needed_average() > MAX_NEEDED {
                // the lesson is as hard as we're willing to make it
                break;
            }
            // out of every way of asking a sentence that grades this word in
            // this card's own mode (that the lesson has reached, that every
            // word's stage allows, and that doesn't repeat a word), take the
            // one with success probability closest to our target.
            //
            // Ties are common and meant to be: a word bank points both ways,
            // and both point at the same card, so nothing in the arithmetic
            // can separate them. Shuffling first and sorting stably is what
            // stops that being settled by declaration order — otherwise every
            // bank a learner ever saw would face the same way.
            let mut candidates: Vec<(usize, Ask)> = self
                .course
                .sentences_with(word)
                .iter()
                .flat_map(|&i| Ask::ALL.map(|ask| (i, ask)))
                .filter(|&candidate| candidate.1.mode() == mode && self.fits(candidate))
                .collect();
            candidates.shuffle(&mut rand::rng());
            let best = candidates.into_iter().min_by(|&a, &b| {
                self.distance(a, TARGET)
                    .total_cmp(&self.distance(b, TARGET))
            });
            if let Some(candidate) = best {
                self.take(candidate);
            }
        }
    }

    // PHASE 2: fill the rest of the lesson with the exercises that bring the
    // average success probability as close to the target as possible.
    //
    // We take the average we need for the remaining exercises to make the
    // total closest to 85%, cap it at 92.5%, and take the exercise whose
    // success probability is closest to that number — repeatedly, until the
    // lesson is full or we run out of exercises that don't repeat a word.
    fn top_up(&mut self) {
        // (probability, candidate), so each probability is computed once,
        // sorted so that the candidates closest to a wanted probability are
        // the ones lying next to it. Candidates taken in phase 1 are in here
        // too; the overlap check below rejects them, because their words are
        // spoken for by then.
        //
        // Shuffled before the sort, and the sort is stable, so the many exact
        // ties — every sentence appears here up to four times, and the two
        // banks always score identically — break at random rather than by
        // declaration order.
        let mut candidates: Vec<(f64, (usize, Ask))> = self
            .course
            .sentences_up_to(self.row)
            .iter()
            .enumerate()
            .flat_map(|(i, sentence)| Ask::ALL.map(|ask| (sentence, (i, ask))))
            .filter(|(sentence, (_, ask))| self.user.allows(&sentence.words, ask.mode()))
            .map(|(sentence, candidate)| {
                (
                    self.user
                        .probability(&sentence.words, candidate.1.mode(), self.timestamp),
                    candidate,
                )
            })
            .collect();
        candidates.shuffle(&mut rand::rng());
        candidates.sort_by(|(a, _), (b, _)| a.total_cmp(b));
        while self.picks.len() < self.holes {
            let needed = self.needed_average().min(MAX_NEEDED);
            // `needed` splits the candidates into two queues: those below it,
            // nearest one last, and those above it, nearest one first. The
            // closest candidate of all is at the head of one of the two, so we
            // keep taking whichever head is nearer until one of them is an
            // exercise we may actually use.
            //
            // A candidate that repeats a word is out of the running for good,
            // not just for this round — `used` only ever grows — so everything
            // we walk past here can leave the vector with the pick.
            let mut lo = candidates.partition_point(|&(p, _)| p < needed);
            let mut hi = lo;
            let chosen = loop {
                // the nearer of the two heads, or whichever one still exists
                let left = match (lo > 0, hi < candidates.len()) {
                    (true, true) => needed - candidates[lo - 1].0 <= candidates[hi].0 - needed,
                    (true, false) => true,
                    (false, true) => false,
                    (false, false) => break None, // both queues exhausted
                };
                let head = if left { lo - 1 } else { hi };
                let candidate = candidates[head];
                if left {
                    lo -= 1
                } else {
                    hi += 1
                }
                if !self.overlaps(candidate.1) {
                    break Some(candidate);
                }
            };
            let Some((probability, candidate)) = chosen else {
                break; // no candidates left that don't repeat a word
            };
            candidates.drain(lo..hi);
            self.total += probability;
            self.claim(candidate);
            self.picks.push(candidate);
        }
    }

    // ORDERING: easiest exercise first (a warm-up), second easiest last (end
    // on a high note), and everything in between shuffled. Only the review
    // section moves; the lesson's tips and introductions keep their place at
    // the front, which is the pacing the skill's word order decided.
    fn order(&mut self) {
        // taken out of `self` so the sort can ask `self` how hard each pick is
        let mut picks = std::mem::take(&mut self.picks);
        // easiest (highest probability) first
        picks.sort_by(|&a, &b| self.probability(b).total_cmp(&self.probability(a)));
        if picks.len() > 3 {
            let opener = picks.remove(0);
            let closer = picks.remove(0);
            picks.shuffle(&mut rand::rng());
            picks.insert(0, opener);
            picks.push(closer);
        }
        self.picks = picks;
    }

    // --- the arithmetic ---

    // given what's in the lesson so far, what average success probability do
    // the remaining holes need to hit our target?
    fn needed_average(&self) -> f64 {
        let slots = self.holes as f64;
        (TARGET * slots - self.total) / (slots - self.picks.len() as f64)
    }

    fn sentence(&self, (i, _): (usize, Ask)) -> &'a Sentence {
        &self.course.sentences[i]
    }

    fn probability(&self, candidate: (usize, Ask)) -> f64 {
        self.user.probability(
            &self.sentence(candidate).words,
            candidate.1.mode(),
            self.timestamp,
        )
    }

    // how far this candidate's success probability sits from where we want it
    fn distance(&self, candidate: (usize, Ask), wanted: f64) -> f64 {
        (self.probability(candidate) - wanted).abs()
    }

    // does this candidate grade a word that is already spoken for?
    fn overlaps(&self, candidate: (usize, Ask)) -> bool {
        self.sentence(candidate)
            .words
            .iter()
            .any(|w| self.used.contains(w.as_str()))
    }

    // may this candidate go in a hole at all? The era rule, then the gate,
    // then the no-repeat rule.
    fn fits(&self, candidate: (usize, Ask)) -> bool {
        let sentence = self.sentence(candidate);
        sentence.row <= self.row
            && self.user.allows(&sentence.words, candidate.1.mode())
            && !self.overlaps(candidate)
    }

    // this candidate's words are spoken for from now on
    fn claim(&mut self, candidate: (usize, Ask)) {
        for word in &self.sentence(candidate).words {
            self.used.insert(word.as_str());
        }
    }

    fn take(&mut self, candidate: (usize, Ask)) {
        self.total += self.probability(candidate);
        self.claim(candidate);
        self.picks.push(candidate);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card::Card;
    use crate::course::tests::{course_of, rows_of, sentence, skill, solo_sentences, vocab_of};
    use crate::skill::{Castle, MaterialBlock};
    use crate::user::tests::{DAY, now, user_with_reviews};
    use crate::word::{Stage, WordState};

    // every word a lesson's questions grade, in order. One solo sentence per
    // word is all these fixtures need — the four ways of asking it are made
    // by the builder, not by the pool.
    fn asked_words(course: &Course, tasks: &[Task]) -> Vec<String> {
        tasks
            .iter()
            .filter_map(|t| t.exercise(course))
            .flat_map(|e| e.words)
            .collect()
    }

    fn exercise_count(tasks: &[Task]) -> usize {
        tasks
            .iter()
            .filter(|t| matches!(t, Task::Exercise { .. }))
            .count()
    }

    // a course of 12 words in one skill of one lesson, and a user who has
    // reviewed all 12 of them 1 to 12 days ago, so every exercise has a
    // different success probability
    fn varied_course_and_user() -> (Course, User) {
        let names: Vec<String> = (0..12).map(|i| format!("c{i}")).collect();
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        let course = course_of(
            vec![skill("only", 0, 0, &refs, 1)],
            solo_sentences(&refs, 0),
        );
        let days: Vec<(&str, u64)> = refs
            .iter()
            .enumerate()
            .map(|(i, &name)| (name, i as u64 + 1))
            .collect();
        let mut user = user_with_reviews(&days);
        // the skill's own words are introduced by lesson 1, so building
        // lesson 1 would claim them all; put the user past it
        user.progress.insert("only".to_string(), 1);
        (course, user)
    }

    // --- shape and size ---

    // a pure-review lesson (nothing left to introduce) fills to LESSON_SIZE
    #[test]
    fn a_lesson_fills_to_lesson_size() {
        let (course, user) = varied_course_and_user();
        let lesson = Lesson::build(&course, &user, "sam", &Position::new("only", 1)).unwrap();
        assert_eq!(exercise_count(&lesson.tasks), LESSON_SIZE);
    }

    // the no-repeat rule is what makes the independence assumption in
    // `probability` reasonable, and it spans the whole lesson
    #[test]
    fn a_lesson_never_asks_about_a_word_twice() {
        let (course, user) = varied_course_and_user();
        let lesson = Lesson::build(&course, &user, "sam", &Position::new("only", 1)).unwrap();
        let mut seen: HashSet<String> = HashSet::new();
        for word in asked_words(&course, &lesson.tasks) {
            assert!(seen.insert(word.clone()), "{word} asked twice");
        }
    }

    // --- the era rule ---

    // a two-row course: the user has finished both, so everything is met
    fn two_row_course_and_user() -> (Course, User) {
        let course = course_of(
            vec![
                skill("early", 0, 0, &["a", "b"], 1),
                skill("late", 1, 0, &["y", "z"], 1),
            ],
            solo_sentences(&["a", "b", "y", "z"], 0)
                .into_iter()
                .enumerate()
                .map(|(i, mut sentence)| {
                    sentence.row = usize::from(i >= 2);
                    sentence
                })
                .collect(),
        );
        let mut user = user_with_reviews(&[("a", 5), ("b", 5), ("y", 1), ("z", 1)]);
        user.progress.insert("early".to_string(), 12);
        user.progress.insert("late".to_string(), 1);
        (course, user)
    }

    // re-taking an early skill draws only on what *that row* had reached —
    // otherwise the learner's later knowledge would leak a later skill's
    // vocabulary into an early lesson
    #[test]
    fn a_retaken_lesson_never_reaches_past_its_own_row() {
        let (course, user) = two_row_course_and_user();
        let lesson = Lesson::build(&course, &user, "sam", &Position::new("early", 1)).unwrap();
        let asked = asked_words(&course, &lesson.tasks);
        assert!(
            !asked.iter().any(|w| w == "y" || w == "z"),
            "leaked {asked:?}"
        );
    }

    // ...and the later skill's own lesson may use everything behind it
    #[test]
    fn a_later_lesson_may_review_everything_behind_it() {
        let (course, user) = two_row_course_and_user();
        let lesson = Lesson::build(&course, &user, "sam", &Position::new("late", 1)).unwrap();
        let asked = asked_words(&course, &lesson.tasks);
        assert!(asked.iter().any(|w| w == "a" || w == "b"));
    }

    // a word from a row-mate skill the learner never started is not eligible,
    // even though its row is reached — this is the half of the era rule the
    // prefix slice cannot see
    #[test]
    fn a_sibling_skill_the_learner_skipped_stays_out() {
        let course = course_of(
            vec![
                skill("left", 0, 0, &["a", "b"], 1),
                skill("right", 0, 0, &["s", "t"], 1),
            ],
            solo_sentences(&["a", "b", "s", "t"], 0),
        );
        // the user did `left` and never touched `right`
        let mut user = user_with_reviews(&[("a", 5), ("b", 5)]);
        user.progress.insert("left".to_string(), 1);
        let lesson = Lesson::build(&course, &user, "sam", &Position::new("left", 1)).unwrap();
        let asked = asked_words(&course, &lesson.tasks);
        assert!(
            !asked.iter().any(|w| w == "s" || w == "t"),
            "leaked {asked:?}"
        );
    }

    // a lesson the user hasn't reached is locked
    #[test]
    fn a_lesson_past_the_learner_is_locked() {
        let (course, user) = two_row_course_and_user();
        let mut fresh = User::new();
        assert!(Lesson::build(&course, &fresh, "sam", &Position::new("early", 2)).is_none());
        assert!(Lesson::build(&course, &fresh, "sam", &Position::new("late", 1)).is_none());
        fresh.progress.insert("early".to_string(), 1);
        assert!(Lesson::build(&course, &user, "sam", &Position::new("early", 1)).is_some());
    }

    // --- introductions ---

    #[test]
    fn a_first_lesson_front_loads_all_of_the_skills_words_that_fit() {
        let course = course_of(
            vec![skill("s", 0, 0, &["a", "b", "c", "d"], 2)],
            solo_sentences(&["a", "b", "c", "d"], 0),
        );
        let user = User::new();
        let lesson = Lesson::build(&course, &user, "sam", &Position::new("s", 1)).unwrap();
        let introduced: Vec<&String> = lesson.tasks.iter().flat_map(Task::introduces).collect();
        assert_eq!(introduced, ["a", "b", "c", "d"]);
        // and where the course has a solo sentence for every word, as here,
        // each introduction is one — asked the gentlest way there is: shown the
        // target language, answered from tiles
        for task in &lesson.tasks {
            if task.introduces().is_empty() {
                continue;
            }
            let exercise = task.exercise(&course).unwrap();
            assert_eq!(exercise.ask, Ask::INTRODUCTION);
            assert_eq!(exercise.mode(), Mode::Scaffolding);
            assert_eq!(exercise.words.len(), 1);
        }
    }

    // A course that never uses `a` or `b` apart introduces them together, in
    // one question, and says so. The alternative — the loader refusing the
    // course, or the sentence claiming to grade one word while the learner is
    // graded on two — is worse in both directions.
    #[test]
    fn a_word_never_used_alone_is_introduced_with_its_neighbour() {
        let course = course_of(
            vec![skill("s", 0, 0, &["a", "b", "c"], 2)],
            vec![sentence("ab", &["a", "b"], 0), sentence("c", &["c"], 0)],
        );
        let lesson = Lesson::build(&course, &User::new(), "sam", &Position::new("s", 1)).unwrap();
        let introduced: Vec<&String> = lesson.tasks.iter().flat_map(Task::introduces).collect();
        assert_eq!(introduced, ["a", "b", "c"]);
        // two questions for three words: `b` was met by `a`'s, so the queue
        // skipped it rather than asking it again
        assert_eq!(exercise_count(&lesson.tasks), 2);
        // ...which is the rule that matters — a lesson never grades a word
        // twice, and the 85% arithmetic assumes exactly that
        let asked = asked_words(&course, &lesson.tasks);
        let distinct: HashSet<&String> = asked.iter().collect();
        assert_eq!(asked.len(), distinct.len(), "repeated a word: {asked:?}");
    }

    // A word the learner already knows riding along in someone else's first
    // contact is review, not news: the client's "New word" badge should name
    // the words that really are new and no others.
    #[test]
    fn an_introduction_announces_only_the_words_that_are_new() {
        let course = course_of(
            vec![skill("s", 0, 0, &["a", "b"], 2)],
            vec![sentence("ab", &["a", "b"], 0)],
        );
        // `b` is already in memory, so only `a` is being met here
        let user = user_with_reviews(&[("b", 5)]);
        let lesson = Lesson::build(&course, &user, "sam", &Position::new("s", 1)).unwrap();
        let introduced: Vec<&String> = lesson.tasks.iter().flat_map(Task::introduces).collect();
        assert_eq!(introduced, ["a"]);
    }

    // Ten new words consume the complete exercise budget. In particular, no
    // urgent or 85%-targeted review can displace one of them.
    #[test]
    fn ten_new_words_take_the_entire_first_lesson() {
        let words: Vec<String> = (0..10).map(|i| format!("w{i}")).collect();
        let refs: Vec<&str> = words.iter().map(String::as_str).collect();
        let course = course_of(vec![skill("s", 0, 0, &refs, 2)], solo_sentences(&refs, 0));
        let lesson = Lesson::build(&course, &User::new(), "sam", &Position::new("s", 1)).unwrap();
        assert_eq!(exercise_count(&lesson.tasks), LESSON_SIZE);
        assert!(
            lesson
                .tasks
                .iter()
                .all(|task| !task.introduces().is_empty())
        );
    }

    // A larger skill still cannot overflow the lesson. Its normal schedule
    // remains available to introduce the words that did not fit later.
    #[test]
    fn first_lesson_introductions_respect_the_lesson_size() {
        let words: Vec<String> = (0..12).map(|i| format!("w{i}")).collect();
        let refs: Vec<&str> = words.iter().map(String::as_str).collect();
        let course = course_of(vec![skill("s", 0, 0, &refs, 2)], solo_sentences(&refs, 0));
        let lesson = Lesson::build(&course, &User::new(), "sam", &Position::new("s", 1)).unwrap();
        assert_eq!(exercise_count(&lesson.tasks), LESSON_SIZE);
        assert_eq!(
            lesson.tasks.iter().flat_map(Task::introduces).count(),
            LESSON_SIZE
        );
    }

    // the holes are for review and only review: a word this very lesson is
    // teaching must not also turn up in one, even though a re-taken lesson's
    // own words ARE in memory by then and would top the urgent list
    #[test]
    fn a_lessons_own_introductions_stay_out_of_its_holes() {
        let course = course_of(
            vec![skill("s", 0, 0, &["new", "old"], 2)],
            solo_sentences(&["new", "old"], 0),
        );
        // "new" is by far the most urgent word the learner has
        let mut user = user_with_reviews(&[("new", 90), ("old", 1)]);
        user.progress.insert("s".to_string(), 2);
        let lesson = Lesson::build(&course, &user, "sam", &Position::new("s", 1)).unwrap();
        // exactly one task mentions "new": the introduction itself
        let mentions = lesson
            .tasks
            .iter()
            .filter_map(|t| t.exercise(&course))
            .filter(|e| e.words.iter().any(|w| w == "new"))
            .count();
        assert_eq!(mentions, 1);
    }

    // --- material ---

    #[test]
    fn material_comes_first_and_only_for_its_own_lesson() {
        let mut definition = skill("s", 0, 0, &["a", "b"], 2);
        definition.material = vec![
            MaterialBlock {
                lesson: 2,
                text: "second".to_string(),
            },
            MaterialBlock {
                lesson: 1,
                text: "first".to_string(),
            },
        ];
        let course = course_of(vec![definition], solo_sentences(&["a", "b"], 0));
        let lesson = Lesson::build(&course, &User::new(), "sam", &Position::new("s", 1)).unwrap();
        assert!(matches!(lesson.tasks[0], Task::Material { .. }));
        assert_eq!(lesson.tasks[0].material(&course).unwrap().text, "first");
        assert_eq!(
            lesson
                .tasks
                .iter()
                .filter(|t| t.material(&course).is_some())
                .count(),
            1
        );
    }

    // --- hitting the target ---

    // the algorithm exists to hit the 85% target, so a lesson's expected
    // accuracy should land near it
    #[test]
    fn a_lessons_expected_accuracy_is_near_target() {
        let (course, user) = varied_course_and_user();
        let lesson = Lesson::build(&course, &user, "sam", &Position::new("only", 1)).unwrap();
        let timestamp = now();
        let probabilities: Vec<f64> = lesson
            .tasks
            .iter()
            .filter_map(|t| t.exercise(&course))
            .map(|e| user.probability(&e.words, e.mode(), timestamp))
            .collect();
        let mean = probabilities.iter().sum::<f64>() / probabilities.len() as f64;
        assert!((mean - TARGET).abs() < 0.1, "expected accuracy {mean}");
    }

    // the review section opens with the easiest exercise (a warm-up) and
    // closes with the second easiest (end on a high note)
    #[test]
    fn the_review_section_opens_and_closes_with_the_easiest() {
        let (course, user) = varied_course_and_user();
        let lesson = Lesson::build(&course, &user, "sam", &Position::new("only", 1)).unwrap();
        let timestamp = now();
        let probabilities: Vec<f64> = lesson
            .tasks
            .iter()
            .filter_map(|t| t.exercise(&course))
            .map(|e| user.probability(&e.words, e.mode(), timestamp))
            .collect();
        let hardest = probabilities.iter().copied().fold(f64::INFINITY, f64::min);
        assert!(probabilities[0] > hardest);
        assert!(probabilities[probabilities.len() - 1] > hardest);
    }

    // --- castles ---

    fn castle_course() -> Course {
        let words: Vec<String> = (0..30).map(|i| format!("w{i}")).collect();
        let refs: Vec<&str> = words.iter().map(String::as_str).collect();
        let skills = vec![
            skill("one", 0, 0, &refs[..15], 1),
            skill("two", 1, 0, &refs[15..], 1),
        ];
        let rows = rows_of(&skills);
        // one sentence per word is enough: every one of them is askable as a
        // word bank, a recognition drill and a production drill
        let mut sentences = solo_sentences(&refs[..15], 0);
        sentences.extend(solo_sentences(&refs[15..], 1));
        Course::new(
            "test".to_string(),
            "en".to_string(),
            "es".to_string(),
            vocab_of(&refs),
            sentences,
            skills,
            rows,
            vec![Castle {
                castle: 0,
                rows: 0..2,
            }],
        )
    }

    fn ready_for_the_castle(course: &Course) -> User {
        let mut user = User::new();
        let timestamp = now();
        for skill in course.skills() {
            user.progress.insert(
                skill.id.clone(),
                crate::skill::LESSONS_PER_LEVEL * crate::skill::ROW_GATE_LEVEL,
            );
            for word in &skill.words {
                user.words.insert(
                    word.clone(),
                    WordState::at(Stage::Recognition, Card::new().good(timestamp - DAY)),
                );
            }
        }
        user
    }

    #[test]
    fn a_castle_asks_castle_size_questions() {
        let course = castle_course();
        let user = ready_for_the_castle(&course);
        let test = Lesson::castle(&course, &user, "sam", 0).unwrap();
        assert_eq!(test.tasks.len(), CASTLE_SIZE);
        assert_eq!(test.target, Target::Castle(0));
    }

    // no word banks: a bank is guessable enough that passing one proves
    // little, and a castle exists to prove something
    #[test]
    fn a_castle_asks_only_recognition_and_production() {
        let course = castle_course();
        let user = ready_for_the_castle(&course);
        let test = Lesson::castle(&course, &user, "sam", 0).unwrap();
        for task in &test.tasks {
            let exercise = task.exercise(&course).unwrap();
            assert_ne!(exercise.mode(), Mode::Scaffolding, "{}", exercise.id);
        }
    }

    // the ladder is bypassed on the way in: every word here sits at
    // Recognition, and a castle still asks some of them to produce
    #[test]
    fn a_castle_asks_above_the_ladder() {
        let course = castle_course();
        let user = ready_for_the_castle(&course);
        // the sample is random, so look across a few tests rather than one
        let produced = (0..10).any(|_| {
            let test = Lesson::castle(&course, &user, "sam", 0).unwrap();
            test.tasks
                .iter()
                .any(|task| task.exercise(&course).unwrap().mode() == Mode::Production)
        });
        assert!(produced, "a castle never asked above the ladder");
    }

    // it samples across the whole stretch, not just the row nearest the test
    #[test]
    fn a_castle_samples_across_all_of_its_rows() {
        let course = castle_course();
        let user = ready_for_the_castle(&course);
        let test = Lesson::castle(&course, &user, "sam", 0).unwrap();
        let rows: HashSet<usize> = test
            .tasks
            .iter()
            .map(|t| t.exercise(&course).unwrap().row)
            .collect();
        assert_eq!(rows.len(), 2);
    }

    #[test]
    fn a_castle_never_asks_about_a_word_twice() {
        let course = castle_course();
        let user = ready_for_the_castle(&course);
        let test = Lesson::castle(&course, &user, "sam", 0).unwrap();
        let mut seen: HashSet<String> = HashSet::new();
        for word in asked_words(&course, &test.tasks) {
            assert!(seen.insert(word.clone()), "{word} asked twice");
        }
    }

    // a castle that isn't due can't be taken
    #[test]
    fn a_castle_is_locked_until_its_stretch_is_finished() {
        let course = castle_course();
        let mut user = ready_for_the_castle(&course);
        user.progress.remove("two");
        assert!(Lesson::castle(&course, &user, "sam", 0).is_none());
    }
}
