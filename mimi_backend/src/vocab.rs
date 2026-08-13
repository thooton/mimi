// The course's vocabulary: the fixed list of words it teaches, in frequency
// order, each in dictionary form.
//
// A word is the atom of spaced repetition: one `WordState`, up to three cards
// (see word.rs). It is deliberately not one inflection: a learner is asked
// about `comer` through whichever of `como`, `comí` or `comen` a sentence
// happens to use, and every one of those verdicts lands on the same card. The
// question the system asks is "do you know this word", and asking it through a
// different form each time is the point rather than a compromise.
//
// The two form lists are what make grading work without hand-marked answers.
// `forms` is every target-language spelling of this word; `glosses` is the
// same list on the source-language side, and exists so that an es->en answer
// can be graded word by word too rather than all-or-nothing (see sentence.rs).
//
// Neither list is pruned for ambiguity here, deliberately. A sentence is only
// ever searched for the handful of words it tags, so a form is confusing only
// when two of those words offer it, and `sentence::locate` drops exactly those
// clashes, per search. A course-wide pass would instead strip a form from
// every word that shares it with anything, including the vast majority of
// pairs that never meet in one sentence.

use std::collections::HashMap;

pub struct Word {
    // how the rest of the system names this word ("comer")
    pub id: String,
    // its dictionary form, as shown to a person
    pub word: String,
    // every target-language form of it
    pub forms: Vec<String>,
    // every source-language form of its translation
    pub glosses: Vec<String>,
}

// The whole word list, in course order, with an index by id.
//
// Order is frequency order, and it is only ever used for authoring and
// display: what a learner meets when is decided by which skill a word belongs
// to and where that skill sits in the tree, never by rank.
pub struct Vocab {
    words: Vec<Word>,
    by_id: HashMap<String, usize>,
}

impl Vocab {
    pub fn new(words: Vec<Word>) -> Vocab {
        let by_id = words
            .iter()
            .enumerate()
            .map(|(i, word)| (word.id.clone(), i))
            .collect();
        Vocab { words, by_id }
    }

    pub fn get(&self, id: &str) -> Option<&Word> {
        self.by_id.get(id).map(|&i| &self.words[i])
    }

    pub fn contains(&self, id: &str) -> bool {
        self.by_id.contains_key(id)
    }

    pub fn words(&self) -> &[Word] {
        &self.words
    }

    pub fn len(&self) -> usize {
        self.words.len()
    }

    // What a word id says, for the benefit of a person reading it: a profile
    // says "you learnt comer", not "you learnt the word with id comer". They
    // are usually the same string; falling back to the id keeps a word the
    // list somehow doesn't cover visible rather than dropping it from
    // whatever is being shown.
    pub fn word_for(&self, id: &str) -> String {
        self.get(id)
            .map_or_else(|| id.to_string(), |w| w.word.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn word(id: &str) -> Word {
        Word {
            id: id.to_string(),
            word: id.to_string(),
            forms: vec![id.to_string()],
            glosses: Vec::new(),
        }
    }

    #[test]
    fn words_are_found_by_id() {
        let vocab = Vocab::new(vec![word("pan"), word("agua")]);
        assert_eq!(vocab.get("pan").unwrap().word, "pan");
        assert!(vocab.contains("agua"));
        assert!(!vocab.contains("queso"));
        assert_eq!(vocab.len(), 2);
    }

    // a word the list doesn't cover is a course-data gap; showing the id makes
    // it visible instead of quietly dropping the word
    #[test]
    fn an_unknown_id_glosses_as_itself() {
        let vocab = Vocab::new(vec![word("pan")]);
        assert_eq!(vocab.word_for("pan"), "pan");
        assert_eq!(vocab.word_for("queso"), "queso");
    }
}
