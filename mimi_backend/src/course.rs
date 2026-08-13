// The course, as the rest of the server sees it: the vocabulary it teaches,
// the skills that partition that vocabulary, the shape of the tree they sit
// in, and the pool of sentences the lesson builder chooses from.
//
// One `Course` is immutable after assembly. The server may atomically replace
// its `Arc<Course>` after a wiki edit, but every request sees one finished
// generation. Everything expensive about a sentence (expanding brackets,
// locating each word in each wording, marking the spans) has already happened
// by the time this exists (see loader.rs).
//
// Exercises are not stored, they are made. A sentence can be asked four ways
// (see `Ask`), and which of them a learner may meet depends on where each of
// its words sits on the ladder, so a question with an answer per learner cannot
// be a thing on a shelf. The pool holds sentences, a lesson's tasks point at a
// sentence and name a way of asking it, and `Course::exercise` builds the
// question the moment somebody is going to see it.
//
// Row is the course's only ordinal. Skills in a row are unlocked together and
// may be done in any order, so there is no total order over skills; rows are
// strictly ordered, and that is enough. The pool is sorted by row, so "every
// sentence at or before row n" is a prefix, which stops a learner re-taking an
// early skill from being shown later material.

use std::collections::HashMap;

use crate::exercise::{Ask, Exercise, FlashcardDirection, Side};
use crate::position::Position;
use crate::sentence::Sentence;
use crate::skill::SKILL_LESSONS;
use crate::skill::{Castle, Skill};
use crate::vocab::Vocab;

// how many wrong tiles a word bank offers alongside the right ones
const BANK_DISTRACTORS: usize = 3;

pub struct Course {
    pub id: String,
    pub source_lang: String,
    pub target_lang: String,
    // every word the course teaches, in frequency order
    pub vocab: Vocab,
    // the review pool: every sentence the lesson builder may draw a question
    // from, sorted by row, so `sentences_up_to` is a prefix slice
    pub sentences: Vec<Sentence>,
    // word id -> the index of every sentence that grades that word. A
    // sentence grading several words appears in several lists.
    by_word: HashMap<String, Vec<usize>>,
    // the word's first contact: the gentlest sentence that uses it, asked
    // with tiles. This is the only way a word enters a learner's memory,
    // because material teaches nothing.
    introductions: HashMap<String, usize>,
    // the skills, in layout order (castle, then row, then across the row)
    skills: Vec<Skill>,
    by_skill: HashMap<String, usize>,
    // global row index -> the skills in that row, as indices into `skills`
    rows: Vec<Vec<usize>>,
    // the castles, in order; castle 0 seals the first stretch of rows
    castles: Vec<Castle>,
    // the wrong tiles a word bank may offer, per language
    source_tiles: Vec<(usize, String)>,
    target_tiles: Vec<(usize, String)>,
}

impl Course {
    pub fn new(
        id: String,
        source_lang: String,
        target_lang: String,
        vocab: Vocab,
        mut sentences: Vec<Sentence>,
        skills: Vec<Skill>,
        rows: Vec<Vec<usize>>,
        castles: Vec<Castle>,
    ) -> Course {
        // keep the pool in course order, so everything a learner has reached
        // is always a prefix of it (see `sentences_up_to`). A stable sort, so
        // the order within a row stays the order the skills were read in.
        sentences.sort_by_key(|s| s.row);
        let mut by_word: HashMap<String, Vec<usize>> = HashMap::new();
        // A word's first contact is the gentlest sentence that uses it:
        // fewest words to meet at once, and among equals the earliest in
        // course order. A sentence that uses nothing else is therefore always
        // preferred, but it is not required: an author who only ever wrote
        // `pan` alongside `comer` has written a course where the two are met
        // together, and refusing to teach `pan` at all would be a worse answer
        // than teaching both in one question (see `Lesson::build`).
        let mut introductions: HashMap<String, usize> = HashMap::new();
        for (i, sentence) in sentences.iter().enumerate() {
            for word in &sentence.words {
                by_word.entry(word.clone()).or_default().push(i);
                // Review can safely fall back to whole-answer grading when a
                // form cannot be located. A first contact cannot: its UI
                // promises to point at every word it calls new, so only a
                // sentence with all of those dedicated prompt spans may be an
                // introduction candidate.
                if !sentence.can_introduce() {
                    continue;
                }
                match introductions.get(word) {
                    // an equal count keeps the earlier sentence, so the choice
                    // is course order and not iteration luck
                    Some(&best) if sentences[best].words.len() <= sentence.words.len() => {}
                    _ => {
                        introductions.insert(word.clone(), i);
                    }
                }
            }
        }
        let by_skill = skills
            .iter()
            .enumerate()
            .map(|(i, skill)| (skill.id.clone(), i))
            .collect();
        let source_tiles = tile_pool(&sentences, Side::Source);
        let target_tiles = tile_pool(&sentences, Side::Target);
        Course {
            id,
            source_lang,
            target_lang,
            vocab,
            sentences,
            by_word,
            introductions,
            skills,
            by_skill,
            rows,
            castles,
            source_tiles,
            target_tiles,
        }
    }

    // --- the pool ---

    // Every sentence at or before `row`, which is a prefix of the pool
    // because the pool is sorted by row.
    //
    // This is half of what a lesson may draw on. The other half, that the
    // learner has met every word a sentence grades, is `User`'s to answer and
    // covers what this can't see: a row-mate skill they haven't
    // touched, or a word from later in the very skill they are re-taking.
    pub fn sentences_up_to(&self, row: usize) -> &[Sentence] {
        let end = self.sentences.partition_point(|s| s.row <= row);
        &self.sentences[..end]
    }

    // the indices of every sentence that grades the given word
    pub fn sentences_with(&self, word: &str) -> &[usize] {
        self.by_word.get(word).map_or(&[], Vec::as_slice)
    }

    // A natural target-language example for a vocabulary card. Prefer the
    // word's first authored sentence so cards and lessons teach the same
    // usage; missing content is handled by the view with no example.
    pub fn example_for(&self, word: &str) -> Option<String> {
        let sentence = self.sentences.get(*self.sentences_with(word).first()?)?;
        Some(sentence.target.preferred().text.clone())
    }

    // The sentence that introduces a word: the one using the fewest words of
    // its skill, which is a sentence using nothing else wherever the course
    // has one. None means the course data uses the word in no sentence at
    // all, which the loader rejects: a word with no sentence can never be
    // learnt.
    //
    // It may introduce more than one word, and then all of them are met at
    // once (`Lesson::build`). That is not a compromise so much as the honest
    // reading of the content: a skill teaches its words together, and a course
    // that never says `pan` without `comer` is a course where knowing one is
    // knowing the other.
    //
    // It is always asked as `INTRODUCTION`: shown the target language, tiles
    // in the source. Recognizing beats producing for a first contact, and
    // tiles beat typing, which is also what makes meeting two words in one
    // question survivable: a bank constrains the answer to the point of being
    // solvable by somebody who has met neither.
    pub fn introduction_for(&self, word: &str) -> Option<usize> {
        self.introductions.get(word).copied()
    }

    // --- making a question ---

    // The exercise a sentence becomes when asked this way, tiles and all.
    //
    // This is the only place a full `Exercise` comes from, and it is called
    // once per question a learner actually sees, while the lesson response is
    // materialized, never again for grading and never over the pool at large.
    // The builder compares candidates by their sentence's words and their
    // `Ask`'s mode, which is everything the arithmetic needs.
    pub fn exercise(&self, sentence: usize, ask: Ask) -> Option<Exercise> {
        let sentence = self.sentences.get(sentence)?;
        let mut exercise = sentence.ask(ask);
        if ask.tiles() {
            exercise.bank = self.distractors(&exercise);
        }
        Some(exercise)
    }

    // A word bank's wrong tiles.
    //
    // They are sampled from the tokens of other sentences in the same
    // language, from the same row or an earlier one, never a later one, which
    // would show a learner vocabulary they haven't reached. Sampling is
    // seeded by the exercise's id, so a bank looks the same every time it is
    // served: a re-taken lesson is genuinely the same lesson, and the tiles a
    // client grades against are the tiles it showed.
    fn distractors(&self, exercise: &Exercise) -> Vec<String> {
        let pool = self.tiles(exercise.ask.produces());
        // the answer's own tokens are already on the board as the right ones
        let own = &exercise.tiles;
        let reachable = pool.partition_point(|(row, _)| *row <= exercise.row);
        // sample without replacement by walking a random permutation of the
        // reachable tokens until we have enough
        let mut rng = Rng::seeded(&exercise.id);
        let mut order: Vec<usize> = (0..reachable).collect();
        for i in (1..order.len()).rev() {
            order.swap(i, rng.below(i + 1));
        }
        let mut chosen: Vec<String> = Vec::new();
        for i in order {
            if chosen.len() == BANK_DISTRACTORS {
                break;
            }
            let token = &pool[i].1;
            if !own.contains(token) && !chosen.contains(token) {
                chosen.push(token.clone());
            }
        }
        chosen
    }

    fn tiles(&self, side: Side) -> &[(usize, String)] {
        match side {
            Side::Source => &self.source_tiles,
            Side::Target => &self.target_tiles,
        }
    }

    // How a question is described to a client: "es->en" for one shown Spanish
    // and answered in English, and the reverse for the other. The client shows
    // it as "Translate to English", which is the only reason the codes exist.
    //
    // This is the one place in the server that names a language. Nothing else
    // has needed to since a sentence stopped being authored pointing one way:
    // inside, a question knows which side it shows, and what those sides are
    // called is a property of the course rather than of the question.
    pub fn direction_of(&self, ask: Ask) -> String {
        self.direction(ask.shows(), ask.produces())
    }

    pub fn flashcard_direction_of(&self, direction: FlashcardDirection) -> String {
        self.direction(direction.shows(), direction.produces())
    }

    fn direction(&self, shows: Side, produces: Side) -> String {
        let name = |side: Side| match side {
            Side::Source => &self.source_lang,
            Side::Target => &self.target_lang,
        };
        format!("{}->{}", name(shows), name(produces))
    }

    // --- the tree ---

    pub fn skills(&self) -> &[Skill] {
        &self.skills
    }

    pub fn skill(&self, id: &str) -> Option<&Skill> {
        self.by_skill.get(id).map(|&i| &self.skills[i])
    }

    pub fn skill_index(&self, id: &str) -> Option<usize> {
        self.by_skill.get(id).copied()
    }

    // the skills of each row, in order
    pub fn rows(&self) -> &[Vec<usize>] {
        &self.rows
    }

    pub fn castles(&self) -> &[Castle] {
        &self.castles
    }

    // whether the course has this lesson at all
    pub fn has_lesson(&self, position: &Position) -> bool {
        self.skill(&position.skill).is_some()
            && position.lesson >= 1
            && position.lesson <= SKILL_LESSONS
    }

    // every word a castle's test may draw on: the words of every skill in
    // every row of its stretch, in course order. Reaching the castle means
    // having completed all of those skills, so every one of these words has
    // been met.
    pub fn words_in_castle(&self, castle: usize) -> Vec<&str> {
        let Some(castle) = self.castles.get(castle) else {
            return Vec::new();
        };
        self.rows[castle.rows.clone()]
            .iter()
            .flatten()
            .flat_map(|&i| self.skills[i].words.iter().map(String::as_str))
            .collect()
    }
}

// Every token the course says in one language, tagged with the row it is first
// said in and sorted by it, so "everything up to row n" is a prefix.
//
// Only preferred wordings contribute. An alternative is something the course
// will accept rather than something it teaches, and a bank tile is shown, so
// putting a merely-tolerated phrasing on the board would teach it by the back
// door.
//
// Tokens go through `tile`, the same cleanup the correct tiles get (see
// exercise.rs): a distractor carrying its punctuation would give away where
// it doesn't go, which is the same hint by other means.
fn tile_pool(sentences: &[Sentence], side: Side) -> Vec<(usize, String)> {
    let mut pool: Vec<(usize, String)> = sentences
        .iter()
        .flat_map(|sentence| {
            sentence
                .side(side)
                .tiles()
                .iter()
                .map(|token| (sentence.row, token.clone()))
        })
        .collect();
    pool.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    pool.dedup();
    pool
}

// A tiny deterministic PRNG (splitmix64), seeded from a string. A word bank
// that shuffled between requests would make a re-taken lesson a different
// lesson, and would hand the client a set of tiles it had never shown.
struct Rng(u64);

impl Rng {
    fn seeded(text: &str) -> Rng {
        // FNV-1a, so the seed depends on the whole id
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        for byte in text.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x1000_0000_01b3);
        }
        Rng(hash)
    }

    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            0
        } else {
            (self.next() % n as u64) as usize
        }
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::sentence::{Mark, Phrasing, Span, Wording};
    use crate::skill::Skill;
    use crate::vocab::Word;

    // the text of each accepted answer, which is what most tests care about;
    // the spans have their own tests in sentence.rs
    pub fn texts(phrasings: &[Phrasing]) -> Vec<&str> {
        phrasings.iter().map(|p| p.text.as_str()).collect()
    }
    use std::collections::HashSet;

    // a skill in the given row. The final argument is retained so older test
    // call sites remain compact; lesson count is deliberately not a skill.
    pub fn skill(id: &str, row: usize, castle: usize, words: &[&str], _lessons: u8) -> Skill {
        Skill {
            id: id.to_string(),
            name: id.to_string(),
            focus: String::new(),
            words: words.iter().map(|w| w.to_string()).collect(),
            material: Vec::new(),
            row,
            castle,
        }
    }

    pub fn vocab_of(words: &[&str]) -> Vocab {
        Vocab::new(
            words
                .iter()
                .map(|w| Word {
                    id: w.to_string(),
                    word: w.to_string(),
                    forms: vec![w.to_string()],
                    glosses: vec![w.to_string()],
                })
                .collect(),
        )
    }

    // A sentence grading the given words, located on both sides so that the
    // fixtures grade word by word. Each side reads "es_a es_b", one token per
    // word, each token spanned by the word it names; a test that cares which
    // language it is looking at can set the wordings itself.
    pub fn sentence(id: &str, words: &[&str], row: usize) -> Sentence {
        let marked = |prefix: &str| {
            let mut text = String::new();
            let mut marks = Vec::new();
            for word in words {
                if !text.is_empty() {
                    text.push(' ');
                }
                let start = text.chars().count();
                text.push_str(prefix);
                text.push_str(word);
                marks.push(Mark {
                    word: (*word).to_string(),
                    start,
                    end: text.chars().count(),
                });
            }
            Wording::new(Phrasing { text, words: marks }, Vec::new())
        };
        Sentence {
            id: id.to_string(),
            words: words.iter().map(|w| w.to_string()).collect(),
            row,
            skill: format!("row{row}"),
            source: marked("en_"),
            target: marked("es_"),
            target_marks: words
                .iter()
                .scan(0, |offset, word| {
                    let start = *offset + 3; // skip this fixture's `es_`
                    let end = start + word.encode_utf16().count();
                    *offset = end + 1; // and the space before the next token
                    Some(Mark {
                        word: (*word).to_string(),
                        start,
                        end,
                    })
                })
                .collect(),
        }
    }

    // one solo sentence per word, which is what a skill needs for every one of
    // its words to have a first contact
    pub fn solo_sentences(words: &[&str], row: usize) -> Vec<Sentence> {
        words
            .iter()
            .map(|word| sentence(word, &[word], row))
            .collect()
    }

    // a course of the given skills, one per row
    pub fn course_of(skills: Vec<Skill>, sentences: Vec<Sentence>) -> Course {
        let rows = rows_of(&skills);
        let castles = vec![Castle {
            castle: 0,
            rows: 0..rows.len(),
        }];
        let words: Vec<&str> = skills
            .iter()
            .flat_map(|s| s.words.iter().map(String::as_str))
            .collect();
        Course::new(
            "test".to_string(),
            "en".to_string(),
            "es".to_string(),
            vocab_of(&words),
            sentences,
            skills,
            rows,
            castles,
        )
    }

    pub fn rows_of(skills: &[Skill]) -> Vec<Vec<usize>> {
        let mut rows: Vec<Vec<usize>> = Vec::new();
        for (i, skill) in skills.iter().enumerate() {
            while rows.len() <= skill.row {
                rows.push(Vec::new());
            }
            rows[skill.row].push(i);
        }
        rows
    }

    fn ids(sentences: &[Sentence]) -> Vec<&str> {
        sentences.iter().map(|s| s.id.as_str()).collect()
    }

    fn three_row_course() -> Course {
        course_of(
            vec![
                skill("one", 0, 0, &["a"], 2),
                skill("two", 1, 0, &["b"], 2),
                skill("three", 2, 0, &["c"], 2),
            ],
            // deliberately out of order, so this also covers the sort in `new`
            vec![
                sentence("c1", &["c"], 2),
                sentence("a1", &["a"], 0),
                sentence("b1", &["b"], 1),
            ],
        )
    }

    #[test]
    fn new_sorts_the_pool_by_row() {
        assert_eq!(ids(&three_row_course().sentences), ["a1", "b1", "c1"]);
    }

    #[test]
    fn up_to_includes_the_row_itself() {
        let course = three_row_course();
        assert_eq!(ids(course.sentences_up_to(1)), ["a1", "b1"]);
        assert_eq!(ids(course.sentences_up_to(0)), ["a1"]);
        assert_eq!(course.sentences_up_to(9).len(), 3);
    }

    // sorting must not invalidate the indices stored in `by_word`
    #[test]
    fn the_word_index_survives_sorting() {
        let course = three_row_course();
        for (word, id) in [("a", "a1"), ("b", "b1"), ("c", "c1")] {
            let found = course.sentences_with(word);
            assert_eq!(found.len(), 1, "{word}");
            assert_eq!(course.sentences[found[0]].id, id);
        }
    }

    // A word is introduced by the gentlest sentence using it, fewest words to
    // meet at once, so a solo sentence always wins where the course has one.
    #[test]
    fn the_fewest_words_at_once_introduces_a_word() {
        let course = course_of(
            vec![skill("one", 0, 0, &["a", "b", "c"], 1)],
            vec![
                sentence("crowd_a", &["a", "b", "c"], 0),
                sentence("solo_a", &["a"], 0),
            ],
        );
        let intro = course.introduction_for("a").unwrap();
        assert_eq!(course.sentences[intro].id, "solo_a");
    }

    // ...and where it has none, the word is still introduced: by a sentence
    // that brings its neighbours in with it. Refusing to teach `c` at all
    // because the course never says it alone would be the worse answer.
    #[test]
    fn a_word_never_used_alone_is_introduced_alongside_its_neighbours() {
        let course = course_of(
            vec![skill("one", 0, 0, &["a", "b", "c"], 1)],
            vec![
                sentence("all_three", &["a", "b", "c"], 0),
                sentence("pair", &["b", "c"], 0),
            ],
        );
        // the pair is gentler than the trio, so it introduces both its words
        for word in ["b", "c"] {
            let intro = course.introduction_for(word).unwrap();
            assert_eq!(course.sentences[intro].id, "pair", "{word}");
        }
        // `a` appears in one sentence only, and that is what introduces it
        let intro = course.introduction_for("a").unwrap();
        assert_eq!(course.sentences[intro].id, "all_three");
        // a word no sentence uses has no first contact, which the loader rejects
        assert!(course.introduction_for("d").is_none());
    }

    // --- making a question ---

    // The same sentence, four questions: each shows one side's preferred
    // wording and accepts the other's. This is the whole bidirectional
    // bargain, and it is settled here rather than by the author.
    #[test]
    fn one_sentence_becomes_four_questions() {
        let course = course_of(
            vec![skill("one", 0, 0, &["a"], 1)],
            vec![sentence("s", &["a"], 0)],
        );
        let asked = |ask| course.exercise(0, ask).unwrap();
        // shown the target language, answering in the source...
        assert_eq!(asked(Ask::BuildSource).prompt, "es_a");
        assert_eq!(texts(&asked(Ask::BuildSource).answers), ["en_a"]);
        assert_eq!(asked(Ask::WriteSource).prompt, "es_a");
        // ...and the other way round
        assert_eq!(asked(Ask::BuildTarget).prompt, "en_a");
        assert_eq!(texts(&asked(Ask::WriteTarget).answers), ["es_a"]);
        // the ids differ, because the client reports verdicts against them
        let ids: HashSet<String> = Ask::ALL.iter().map(|&a| asked(a).id).collect();
        assert_eq!(ids.len(), 4);
    }

    // a prompt shows the preferred wording alone; the alternatives are
    // accepted as answers and never shown
    #[test]
    fn alternatives_are_accepted_but_never_prompted() {
        let mut written = sentence("s", &["a"], 0);
        written.source = Wording::new(
            Phrasing::of("en_a", &[("a", Span { start: 0, end: 4 })]),
            vec![Phrasing::of("also_a", &[])],
        );
        let course = course_of(vec![skill("one", 0, 0, &["a"], 1)], vec![written]);
        let recognition = course.exercise(0, Ask::WriteSource).unwrap();
        assert_eq!(texts(&recognition.answers), ["en_a", "also_a"]);
        // and asked the other way, the alternative is not what is shown
        let production = course.exercise(0, Ask::WriteTarget).unwrap();
        assert_eq!(production.prompt, "en_a");
    }

    // Only typed questions come without tiles. A bank's wrong tiles are drawn
    // from the language it is answered in and from no later row than its own,
    // which is what stops a bank leaking vocabulary the learner hasn't reached.
    #[test]
    fn only_word_banks_get_tiles_and_only_from_behind_them() {
        let course = course_of(
            vec![
                skill("early", 0, 0, &["a"], 1),
                skill("late", 1, 0, &["z"], 1),
            ],
            vec![sentence("s_a", &["a"], 0), sentence("s_z", &["z"], 1)],
        );
        assert!(
            course
                .exercise(0, Ask::WriteTarget)
                .unwrap()
                .bank
                .is_empty()
        );
        // the early sentence is alone in its era, so there is nothing to
        // distract it with...
        assert!(
            course
                .exercise(0, Ask::BuildTarget)
                .unwrap()
                .bank
                .is_empty()
        );
        // ...while the later one may borrow from the row behind it, in the
        // language it is answered in
        let late = course.exercise(1, Ask::BuildTarget).unwrap();
        assert_eq!(late.bank, ["es_a"]);
        let late_other_way = course.exercise(1, Ask::BuildSource).unwrap();
        assert_eq!(late_other_way.bank, ["en_a"]);
    }

    // Distractors go through the same cleanup as the correct tiles: one
    // carrying its punctuation ("todo!") would tell the learner where it
    // doesn't go, which is the same hint by other means. And the answer's
    // own tokens are compared stripped on both sides, or "es_z" would slip
    // onto the board next to the correct "es_z" because the pool's copy came
    // from "es_z.".
    #[test]
    fn distractors_carry_no_punctuation() {
        let mut early = sentence("s_a", &["a"], 0);
        // "¡" is two bytes, so the span is not where a naive count would put it
        early.target = Wording::new(
            Phrasing::of("¡es_a, todo!", &[("a", Span { start: 2, end: 6 })]),
            Vec::new(),
        );
        let mut late = sentence("s_z", &["z"], 1);
        late.target = Wording::new(
            Phrasing::of("es_z.", &[("z", Span { start: 0, end: 4 })]),
            Vec::new(),
        );
        let course = course_of(
            vec![
                skill("early", 0, 0, &["a"], 1),
                skill("late", 1, 0, &["z"], 1),
            ],
            vec![early, late],
        );
        let mut bank = course.exercise(1, Ask::BuildTarget).unwrap().bank;
        bank.sort();
        assert_eq!(bank, ["es_a", "todo"]);
    }

    // a bank that reshuffled between requests would hand the client a set of
    // tiles it had never shown, and make a re-taken lesson a different lesson
    #[test]
    fn a_banks_tiles_are_the_same_every_time() {
        let words: Vec<String> = (0..20).map(|i| format!("w{i}")).collect();
        let refs: Vec<&str> = words.iter().map(String::as_str).collect();
        let course = course_of(vec![skill("one", 0, 0, &refs, 1)], solo_sentences(&refs, 0));
        let once = course.exercise(3, Ask::BuildTarget).unwrap().bank;
        assert_eq!(once.len(), 3);
        assert_eq!(course.exercise(3, Ask::BuildTarget).unwrap().bank, once);
    }

    // the wire's language codes come from the course index, and are the only
    // place either language is named
    #[test]
    fn a_direction_names_the_two_sides_languages() {
        let course = three_row_course();
        assert_eq!(course.direction_of(Ask::WriteSource), "es->en");
        assert_eq!(course.direction_of(Ask::WriteTarget), "en->es");
        assert_eq!(course.direction_of(Ask::BuildSource), "es->en");
        assert_eq!(course.direction_of(Ask::BuildTarget), "en->es");
    }

    #[test]
    fn a_lesson_exists_only_within_its_skills_count() {
        let course = three_row_course();
        assert!(course.has_lesson(&Position::new("one", 1)));
        assert!(course.has_lesson(&Position::new("one", 2)));
        assert!(course.has_lesson(&Position::new("one", SKILL_LESSONS)));
        assert!(!course.has_lesson(&Position::new("one", SKILL_LESSONS + 1)));
        assert!(!course.has_lesson(&Position::new("one", 0)));
        assert!(!course.has_lesson(&Position::new("nope", 1)));
    }

    // a castle's test draws on every word of every skill in its stretch, and
    // on nothing outside it
    #[test]
    fn a_castle_covers_the_words_of_its_own_rows() {
        let skills = vec![
            skill("one", 0, 0, &["a"], 1),
            skill("two", 1, 0, &["b", "c"], 1),
            skill("three", 2, 1, &["d"], 1),
        ];
        let rows = rows_of(&skills);
        let course = Course::new(
            "test".to_string(),
            "en".to_string(),
            "es".to_string(),
            vocab_of(&["a", "b", "c", "d"]),
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
        );
        assert_eq!(course.words_in_castle(0), ["a", "b", "c"]);
        assert_eq!(course.words_in_castle(1), ["d"]);
        assert!(course.words_in_castle(2).is_empty());
    }
}
