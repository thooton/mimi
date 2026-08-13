use serde::{Deserialize, Serialize};

#[cfg(test)]
use crate::sentence::Mark;
use crate::sentence::Phrasing;

// Which retrieval task an exercise is — the unit the three-mode model
// tracks separately (one FSRS card per word per mode; see word.rs).
// Recognition and recall are different tasks with different difficulties,
// and one stability value can't summarize both.
//
// **The variants are declared in order of difficulty, and that order is the
// `Ord` derive**, so "at least as hard as the rung being climbed" is the plain
// comparison `mode >= top`. Do not reorder them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Mode {
    // picking the right tokens with heavy support: word banks, either
    // direction. A bank constrains the answer space so hard that a success
    // is weak evidence, so it stays in its own box.
    Scaffolding,
    // understanding the target language, unaided: typed, shown the target
    // language and asked for the source
    Recognition,
    // producing the target language, unaided: typed, shown the source
    // language and asked for the target
    Production,
}

// Which of a sentence's two halves we mean. A sentence is a pair — the same
// thing said in the language the learner already has and in the one they are
// learning — and every question is made by choosing one side to show and the
// other to ask for.
//
// This is the course index's `source_lang`/`target_lang` distinction, and the
// only place either language is named: nothing below this line knows what
// "en" or "es" is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    Source,
    Target,
}

// Which side of a flashcard is visible before it is flipped. Unlike `Ask`,
// this says nothing about an input control: recalling the hidden side is the
// exercise. The direction still determines which memory the verdict belongs
// to — seeing the language being learnt tests recognition, while seeing the
// language already known tests production.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlashcardDirection {
    TargetToSource,
    SourceToTarget,
}

impl FlashcardDirection {
    pub fn shows(self) -> Side {
        match self {
            Self::TargetToSource => Side::Target,
            Self::SourceToTarget => Side::Source,
        }
    }

    pub fn produces(self) -> Side {
        match self.shows() {
            Side::Source => Side::Target,
            Side::Target => Side::Source,
        }
    }

    pub fn mode(self) -> Mode {
        match self {
            Self::TargetToSource => Mode::Recognition,
            Self::SourceToTarget => Mode::Production,
        }
    }

    pub fn for_mode(mode: Mode) -> Option<Self> {
        match mode {
            Mode::Scaffolding => None,
            Mode::Recognition => Some(Self::TargetToSource),
            Mode::Production => Some(Self::SourceToTarget),
        }
    }
}

// How a sentence is being asked. One sentence can become any of these four
// questions; which of them a learner actually meets is the ladder's decision
// (see word.rs), and the exercise itself is built only once the lesson has
// chosen it (see `Course::exercise`).
//
// Named for what the learner *does*: `Build` taps tiles from a word bank,
// `Write` types unaided, and the suffix is the side they produce. The side
// they are shown is the other one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Ask {
    // tiles, shown the target language: the gentlest question there is, and
    // the one every word is introduced with
    BuildSource,
    // tiles, shown the source language
    BuildTarget,
    // typed, shown the target language
    WriteSource,
    // typed, shown the source language
    WriteTarget,
}

impl Ask {
    // every way a sentence can be asked, in no significant order — the
    // candidate set the lesson builder filters through the ladder
    pub const ALL: [Ask; 4] = [
        Ask::BuildSource,
        Ask::BuildTarget,
        Ask::WriteSource,
        Ask::WriteTarget,
    ];

    // How a word's first contact is asked, and the only way a word ever
    // enters a learner's memory. Recognizing beats producing for a first
    // meeting, and tiles beat typing, so this is the gentlest of the four —
    // which is what makes `Stage::introduced_by` land every word at the
    // bottom of the ladder.
    pub const INTRODUCTION: Ask = Ask::BuildSource;

    // which mode a verdict on this question is evidence about. Both banks
    // are scaffolding — a bank constrains the answer space so hard that
    // which way it points barely matters — and the typed pair is the
    // recognition/production split the whole ladder is built on.
    pub fn mode(self) -> Mode {
        match self {
            Ask::BuildSource | Ask::BuildTarget => Mode::Scaffolding,
            Ask::WriteSource => Mode::Recognition,
            Ask::WriteTarget => Mode::Production,
        }
    }

    // the side the learner answers in: the accepted answers are every
    // wording of this side, preferred first
    pub fn produces(self) -> Side {
        match self {
            Ask::BuildSource | Ask::WriteSource => Side::Source,
            Ask::BuildTarget | Ask::WriteTarget => Side::Target,
        }
    }

    // the side the learner is shown: the prompt is this side's *preferred*
    // wording alone, because a prompt has to be one sentence
    pub fn shows(self) -> Side {
        match self.produces() {
            Side::Source => Side::Target,
            Side::Target => Side::Source,
        }
    }

    // does the learner assemble the answer from tiles rather than type it?
    pub fn tiles(self) -> bool {
        self.mode() == Mode::Scaffolding
    }

    // what this way of asking is called inside an exercise id
    pub fn tag(self) -> &'static str {
        match self {
            Ask::BuildSource => "bs",
            Ask::BuildTarget => "bt",
            Ask::WriteSource => "ws",
            Ask::WriteTarget => "wt",
        }
    }
}

// One question, ready to be served or graded.
//
// **Nothing stores these.** An exercise is made from a sentence and an `Ask`
// the moment a lesson chooses that pairing (`Course::exercise`) and thrown
// away again; what the course holds, and what a short-lived lesson task points
// at while its response is materialized, is the sentence. Four ways of asking
// every sentence would otherwise be four copies of it in memory, and a lesson
// serves ten questions.
#[derive(Clone)]
pub struct Exercise {
    // generated from the sentence and the way it is being asked, never
    // authored: "food_1:12:bt" is the twelfth sentence of the food_1 skill,
    // asked by building the target language from tiles
    pub id: String,
    // which of the four questions this is
    pub ask: Ask,
    // what we show the user: the preferred wording of the side they are shown
    pub prompt: String,
    // the words this exercise grades, e.g. ["comer", "pan"] — no duplicates.
    // A sentence tags only words from its own skill; anything else it happens
    // to contain is scenery, and is not graded.
    pub words: Vec<String>,
    // which row of the tree the exercise's skill sits in. The course's only
    // ordinal: the pool is sorted by it, so "everything up to row n" is a
    // prefix, and re-taking an early skill can't surface later material.
    pub row: usize,
    // the skill the sentence was written for
    pub skill: String,
    // every answer we accept, each with the spans that say which stretch of it
    // proves which word. The first one is preferred.
    //
    // These are served to the client along with the exercise: grading happens
    // there (instant feedback can't wait for a round trip), and the spans are
    // what let it grade word by word. Hiding the answers would buy nothing —
    // a learner who wants to cheat already can.
    //
    // A word the sentence uses in a form its list doesn't cover is marked in
    // *no* variant (see `Exercise::graded_word_by_word`), so the client falls
    // back to the overall verdict for it — as it does for every word of a
    // sentence that tests one word, which carries no spans at all because
    // there is no credit to divide (see `loader::wording`).
    pub answers: Vec<Phrasing>,
    // a word bank's correct tiles: the preferred answer's own tokens, cut at
    // load time. Empty for typed questions, where nothing is tapped.
    pub tiles: Vec<String>,
    // a word bank's wrong tiles, sampled from vocabulary the learner has
    // reached — never from a later row, which would leak material. Empty for
    // typed questions.
    pub bank: Vec<String>,
}

impl Exercise {
    // which mode a verdict on this exercise is evidence about
    pub fn mode(&self) -> Mode {
        self.ask.mode()
    }

    // Does the client get to grade this word on its own, or does it fall back
    // to how the exercise went overall?
    //
    // The loader marks a word in every accepted answer or in none of them, so
    // that grading can't depend on which variant the learner happened to hit.
    // A word that appears in no span is one the sentence uses in an unlisted
    // form — the all-or-nothing case, which costs precision and never
    // correctness.
    pub fn graded_word_by_word(&self, word: &str) -> bool {
        self.answers.first().is_some_and(|a| a.grades(word))
    }
}

// Bare exercises of each mode, for the tests that care which rung of the
// ladder an exercise sits on rather than what it says. The prompt and the
// answer are just the id, which is enough to tell them apart in a failure.
#[cfg(test)]
impl Exercise {
    // a word bank
    pub fn scaffolding(id: &str, words: &[&str], row: usize) -> Exercise {
        Exercise::of(id, words, row, Ask::BuildTarget)
    }

    // typed, shown the target language
    pub fn recognition(id: &str, words: &[&str], row: usize) -> Exercise {
        Exercise::of(id, words, row, Ask::WriteSource)
    }

    // typed, shown the source language
    pub fn production(id: &str, words: &[&str], row: usize) -> Exercise {
        Exercise::of(id, words, row, Ask::WriteTarget)
    }

    pub fn of(id: &str, words: &[&str], row: usize, ask: Ask) -> Exercise {
        Exercise {
            id: id.to_string(),
            ask,
            prompt: id.to_string(),
            words: words.iter().map(|w| w.to_string()).collect(),
            row,
            skill: format!("row{row}"),
            // every word marked, so the fixtures grade word by word
            answers: vec![Phrasing {
                text: words.join(" "),
                words: {
                    let mut at = 0;
                    words
                        .iter()
                        .map(|w| {
                            let start = at;
                            at += w.chars().count() + 1;
                            Mark {
                                word: (*w).to_string(),
                                start,
                                end: start + w.chars().count(),
                            }
                        })
                        .collect()
                },
            }],
            tiles: Vec::new(),
            bank: Vec::new(),
        }
    }
}

// What a surface token becomes on a word bank's board: the word, and nothing
// but the word. Leading and trailing punctuation is stripped — "¡Hola," is
// shown as "Hola", because a tile carrying its "¡" or "," tells the learner
// where in the answer it sits, and placing the words is half of what a bank
// asks. Interior punctuation stays ("l'homme" is one token); the client
// grades without punctuation, so none of it is needed to check the answer.
//
// None for a token that is nothing but punctuation — a stray "—" has no
// business on the board.
pub fn tile(token: &str) -> Option<String> {
    let stripped: &str = token.trim_matches(|c: char| !c.is_alphanumeric());
    if stripped.is_empty() {
        None
    } else {
        Some(stripped.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // A tile is the word alone: punctuation stuck to it — "¡Hola,", "adiós!" —
    // tells the learner where in the sentence it goes, and placing the words
    // is half the question. Accents are letters, not punctuation, and survive.
    #[test]
    fn a_tile_carries_no_punctuation() {
        assert_eq!(tile("¡Hola,").as_deref(), Some("Hola"));
        assert_eq!(tile("adiós!").as_deref(), Some("adiós"));
        assert_eq!(tile("\"sí.\"").as_deref(), Some("sí"));
        // interior punctuation is part of the token, and stays
        assert_eq!(tile("l'homme").as_deref(), Some("l'homme"));
        // a token that is nothing but punctuation drops off the board entirely
        assert_eq!(tile("—"), None);
    }

    // a word with no span anywhere is the all-or-nothing case: the client has
    // nothing to grade it by on its own
    #[test]
    fn a_word_without_a_span_is_not_graded_on_its_own() {
        let mut exercise = Exercise::production("ex", &["comer", "pan"], 0);
        exercise.answers = vec![Phrasing {
            text: "Como pan.".to_string(),
            words: vec![Mark {
                word: "comer".to_string(),
                start: 0,
                end: 4,
            }],
        }];
        assert!(exercise.graded_word_by_word("comer"));
        assert!(!exercise.graded_word_by_word("pan"));
    }

    // a word bank scaffolds whichever way it points; typed translation is
    // recognition one way and production the other
    #[test]
    fn a_bank_scaffolds_both_ways_and_typing_splits_the_two_modes() {
        assert_eq!(Ask::BuildSource.mode(), Mode::Scaffolding);
        assert_eq!(Ask::BuildTarget.mode(), Mode::Scaffolding);
        assert_eq!(Ask::WriteSource.mode(), Mode::Recognition);
        assert_eq!(Ask::WriteTarget.mode(), Mode::Production);
    }

    // the two sides of a question are always opposite: you are shown the one
    // you are not being asked for
    #[test]
    fn a_question_shows_the_side_it_does_not_ask_for() {
        for ask in Ask::ALL {
            assert_ne!(ask.produces(), ask.shows(), "{ask:?}");
        }
        assert_eq!(Ask::WriteSource.shows(), Side::Target);
        assert_eq!(Ask::WriteTarget.shows(), Side::Source);
    }
}
