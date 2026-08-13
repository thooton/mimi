// Daily quests, derived from the activity row that already records today's
// study. There is no quest table and no reset job: a different UTC day picks
// a different three definitions, and asking for progress measures that day's
// row. This is the same shape as the weekly leaderboard on a smaller clock.
//
// Only facts every learner can keep producing belong in the pool. In
// particular, "learn new words" eventually becomes impossible after the end
// of a course, and time practised is not a fact Mimi records. XP, lessons,
// correct answers and perfect lessons all come directly from `Activity`.

use crate::profile::Activity;

const QUESTS_PER_DAY: usize = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Quest {
    pub id: &'static str,
    pub title: String,
    pub done: u32,
    pub total: u32,
}

pub struct DailyQuests {
    // whole UTC days since the epoch, matching the activity table's key
    pub day: u32,
    pub quests: Vec<Quest>,
}

#[derive(Clone, Copy)]
enum Metric {
    Xp,
    Lessons,
    CorrectAnswers,
    PerfectLessons,
}

impl Metric {
    fn measure(self, activity: &Activity) -> u32 {
        match self {
            Self::Xp => activity.xp(),
            Self::Lessons => activity.lessons,
            Self::CorrectAnswers => activity.correct,
            Self::PerfectLessons => activity.perfect_lessons,
        }
    }

    // Build the number into the title from the same value used by progress,
    // so tuning a target cannot leave the prose promising something else.
    fn title(self, total: u32) -> String {
        match self {
            Self::Xp => format!("Earn {total} XP"),
            Self::Lessons => format!(
                "Complete {total} lesson{}",
                if total == 1 { "" } else { "s" }
            ),
            Self::CorrectAnswers => format!("Answer {total} exercises correctly"),
            Self::PerfectLessons => format!(
                "Complete {total} perfect lesson{}",
                if total == 1 { "" } else { "s" }
            ),
        }
    }
}

#[derive(Clone, Copy)]
struct Definition {
    id: &'static str,
    total: u32,
    metric: Metric,
}

const POOL: [Definition; 4] = [
    Definition {
        id: "earn_xp",
        total: 40,
        metric: Metric::Xp,
    },
    Definition {
        id: "complete_lessons",
        total: 2,
        metric: Metric::Lessons,
    },
    Definition {
        id: "correct_answers",
        total: 15,
        metric: Metric::CorrectAnswers,
    },
    Definition {
        id: "perfect_lesson",
        total: 1,
        metric: Metric::PerfectLessons,
    },
];

impl DailyQuests {
    pub fn of(activity: &Activity, day: u32) -> Self {
        // Advancing by three through a four-item pool visits every starting
        // point before repeating. Each day omits a different definition, so
        // two quests carry over while one turns over; all four get equal time.
        let start = (day as usize % POOL.len() * QUESTS_PER_DAY) % POOL.len();
        let quests = (0..QUESTS_PER_DAY)
            .map(|offset| POOL[(start + offset) % POOL.len()])
            .map(|definition| Quest {
                id: definition.id,
                title: definition.metric.title(definition.total),
                done: definition.metric.measure(activity),
                total: definition.total,
            })
            .collect();
        Self { day, quests }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn activity() -> Activity {
        Activity {
            lessons: 3,
            perfect_lessons: 1,
            exercises: 24,
            correct: 21,
            learned: Vec::new(),
            skills: Vec::new(),
        }
    }

    #[test]
    fn progress_is_measured_from_the_days_activity() {
        let quests = DailyQuests::of(&activity(), 0).quests;
        assert_eq!(
            quests,
            [
                Quest {
                    id: "earn_xp",
                    title: "Earn 40 XP".into(),
                    done: 80,
                    total: 40,
                },
                Quest {
                    id: "complete_lessons",
                    title: "Complete 2 lessons".into(),
                    done: 3,
                    total: 2,
                },
                Quest {
                    id: "correct_answers",
                    title: "Answer 15 exercises correctly".into(),
                    done: 21,
                    total: 15,
                },
            ]
        );
    }

    #[test]
    fn one_quest_rotates_each_day_and_the_pool_repeats_after_four() {
        let ids = |day| {
            DailyQuests::of(&Activity::default(), day)
                .quests
                .into_iter()
                .map(|quest| quest.id)
                .collect::<Vec<_>>()
        };
        assert_eq!(ids(0), ["earn_xp", "complete_lessons", "correct_answers"]);
        assert_eq!(ids(1), ["perfect_lesson", "earn_xp", "complete_lessons"]);
        assert_eq!(ids(2), ["correct_answers", "perfect_lesson", "earn_xp"]);
        assert_eq!(
            ids(3),
            ["complete_lessons", "correct_answers", "perfect_lesson"]
        );
        assert_eq!(ids(4), ids(0));
    }

    #[test]
    fn a_day_without_activity_starts_every_quest_at_zero() {
        assert!(
            DailyQuests::of(&Activity::default(), 1)
                .quests
                .iter()
                .all(|quest| quest.done == 0)
        );
    }
}
