use std::fmt;

/// Which lesson of which skill.
///
/// This is an *address*, not a learner's identity. In a branching tree,
/// progress is a set — which skills are finished, and how far into each — so
/// no single coordinate says where somebody is (see `User::progress`). A
/// position only ever names a lesson to build.
///
/// Lessons are 1-based, and a skill has a fixed number of them.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Position {
    // the skill's id, e.g. "food_1"
    pub skill: String,
    // which of its lessons, 1-based
    pub lesson: u8,
}

impl Position {
    pub fn new(skill: impl Into<String>, lesson: u8) -> Position {
        Position {
            skill: skill.into(),
            lesson,
        }
    }
}

impl fmt::Display for Position {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.skill, self.lesson)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_position_reads_as_skill_dot_lesson() {
        assert_eq!(Position::new("food_1", 2).to_string(), "food_1.2");
    }
}
