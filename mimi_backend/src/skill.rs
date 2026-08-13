// A skill: a themed batch of words that also carries a grammar focus.
//
// This is the unit of authoring, and the idea it exists to express is that
// vocabulary and grammar are carried by the same object. A skill is not
// only "here are seven food words"; it is "here are seven food words, and the
// sentences you will see them in are simple present-tense statements with
// definite articles". The words say what the sentences are about, the focus
// says what shape they take.
//
// Every word in the course belongs to exactly one skill, so the skills are a
// partition of the vocabulary. Skills sit in rows, a few side by side, unlocked
// together and doable in any order, and a run of rows is sealed by a castle, a
// test drawn from just those rows (see lesson.rs).
//
// The `focus` is an authoring instruction and a blurb for the learner. It is
// never tracked, scheduled or graded: making it a trackable thing of its own
// would put one shared item in every sentence of a skill, and the no-repeat
// rule would then allow exactly one of them per lesson.

use std::ops::Range;

// Levels are deliberately course-wide rather than authored per skill. This
// makes a level mean the same amount of work everywhere in the tree.
pub const LESSONS_PER_LEVEL: u8 = 6;
pub const MAX_LEVEL: u8 = 5;
pub const ROW_GATE_LEVEL: u8 = 2;
pub const SKILL_LESSONS: u8 = LESSONS_PER_LEVEL * (MAX_LEVEL + 1);
pub const INTRODUCTION_LESSONS: u8 = LESSONS_PER_LEVEL * ROW_GATE_LEVEL;

pub struct Skill {
    // how the rest of the system names it ("food_1"); also its file name
    pub id: String,
    // what the learner sees on the tree ("Food 1")
    pub name: String,
    // what shape this skill's sentences take, for the author (and the
    // generator) to work from, and for the learner to read
    pub focus: String,
    // the words it teaches, in the order it teaches them
    pub words: Vec<String>,
    // tips, each attached to the lesson it belongs to. Material teaches
    // nothing in the technical sense: it introduces no words and creates no
    // cards. A word enters a learner's memory only by being answered.
    pub material: Vec<MaterialBlock>,
    // where the skill sits, filled in from the layout: its row (0-based,
    // global across the whole course) and the castle whose stretch it is in.
    // `row` is the course's only ordinal, and the pool is sorted by it.
    pub row: usize,
    pub castle: usize,
}

// One tip: markdown, and which lesson of the skill shows it.
#[derive(Debug, Clone, serde::Serialize)]
pub struct MaterialBlock {
    pub lesson: u8,
    pub text: String,
}

impl Skill {
    // The words this lesson of the skill introduces.
    //
    // The word list is the introduction queue and the lesson number indexes
    // into it: seven words over four lessons go 2, 2, 2, 1, with the remainder
    // falling to the earlier lessons. The split is a pure function of the
    // skill, so re-taking lesson 2 introduces exactly what it did the first
    // time.
    pub fn words_for_lesson(&self, lesson: u8) -> &[String] {
        if lesson == 0 {
            return &[];
        }
        // New vocabulary belongs only to levels 0-1. Later levels are pure
        // practice. Splitting over all 12 early lessons also front-loads the
        // remainder, while Lesson::build puts these introductions before all
        // generated review questions.
        let lessons = INTRODUCTION_LESSONS as usize;
        let index = lesson.saturating_sub(1) as usize;
        if index >= lessons {
            return &[];
        }
        let each = self.words.len() / lessons;
        let extra = self.words.len() % lessons;
        let start = index * each + index.min(extra);
        let count = each + usize::from(index < extra);
        &self.words[start..start + count]
    }

    // the tips this lesson shows, in the order they were written
    pub fn material_for_lesson(&self, lesson: u8) -> impl Iterator<Item = (usize, &MaterialBlock)> {
        self.material
            .iter()
            .enumerate()
            .filter(move |(_, block)| block.lesson == lesson)
    }
}

// A stretch of the tree sealed by a test.
//
// Castle 0 is the content before the first test, castle 1 the content between
// the first and the second, and so on. Rows are nested inside castles in the
// course data rather than numbered flat, so a castle boundary is structurally
// incapable of falling in the middle of a row.
pub struct Castle {
    // which castle this is, 0-based, matching the count a user has passed
    pub castle: usize,
    // the rows it covers, as a range of global row indices
    pub rows: Range<usize>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn skill(words: usize) -> Skill {
        Skill {
            id: "s".to_string(),
            name: "S".to_string(),
            focus: String::new(),
            words: (0..words).map(|i| format!("w{i}")).collect(),
            material: Vec::new(),
            row: 0,
            castle: 0,
        }
    }

    fn split(words: usize) -> Vec<usize> {
        let skill = skill(words);
        (1..=INTRODUCTION_LESSONS)
            .map(|l| skill.words_for_lesson(l).len())
            .collect()
    }

    // the remainder goes to the earlier lessons, so the skill front-loads
    // slightly rather than ending on a lump
    #[test]
    fn words_are_split_as_evenly_as_the_count_allows() {
        assert_eq!(split(7), [1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0]);
        assert_eq!(split(12), [1; 12]);
        assert_eq!(split(14), [2, 2, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1]);
    }

    // every word is introduced exactly once, whatever the split
    #[test]
    fn the_split_covers_every_word_once() {
        let skill = skill(7);
        let introduced: Vec<&String> = (1..=INTRODUCTION_LESSONS)
            .flat_map(|l| skill.words_for_lesson(l))
            .collect();
        assert_eq!(introduced.len(), 7);
        assert_eq!(introduced[0], "w0");
        assert_eq!(introduced[6], "w6");
    }

    #[test]
    fn skills_introduce_words_only_before_level_two() {
        let skill = skill(20);
        let counts: Vec<usize> = (1..=SKILL_LESSONS)
            .map(|lesson| skill.words_for_lesson(lesson).len())
            .collect();
        assert_eq!(&counts[..12], &[2, 2, 2, 2, 2, 2, 2, 2, 1, 1, 1, 1]);
        assert!(counts[12..].iter().all(|&count| count == 0));
    }

    // a lesson number past the end introduces nothing rather than panicking,
    // since it can only come from a request the caller should have rejected
    #[test]
    fn a_lesson_out_of_range_introduces_nothing() {
        let skill = skill(7);
        assert!(skill.words_for_lesson(0).is_empty());
        assert!(skill.words_for_lesson(SKILL_LESSONS + 1).is_empty());
    }

    #[test]
    fn material_is_selected_by_lesson() {
        let mut skill = skill(4);
        skill.material = vec![
            MaterialBlock {
                lesson: 1,
                text: "first".to_string(),
            },
            MaterialBlock {
                lesson: 2,
                text: "second".to_string(),
            },
            MaterialBlock {
                lesson: 1,
                text: "also first".to_string(),
            },
        ];
        let first: Vec<usize> = skill.material_for_lesson(1).map(|(i, _)| i).collect();
        assert_eq!(first, [0, 2]);
        let second: Vec<&str> = skill
            .material_for_lesson(2)
            .map(|(_, b)| b.text.as_str())
            .collect();
        assert_eq!(second, ["second"]);
    }
}
