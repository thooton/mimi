use rs_fsrs::{Parameters, Rating};
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;

static FSRS_P: LazyLock<Parameters> = LazyLock::new(Parameters::default);

// A card is plain data (three numbers), so it serializes straight to the
// database and back. `retrievability` is derived from `stability` and the
// time elapsed since `last_reviewed`, so it is never stored.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Card {
    pub stability: f64,
    pub difficulty: f64,
    pub last_reviewed: u64,
}

impl Card {
    pub const fn new() -> Self {
        Self {
            stability: 0.0,
            difficulty: 0.0,
            last_reviewed: 0,
        }
    }
    pub fn retrievability(&self, timestamp: u64) -> f64 {
        if self.last_reviewed == 0 {
            // a card that has never been studied cannot be retrieved
            return 0.0;
        }
        let elapsed_days = (timestamp.saturating_sub(self.last_reviewed) as f64) / 86400.0;
        Parameters::forgetting_curve(elapsed_days, self.stability)
    }
    // calculates the new memory based on the data
    fn next(self, timestamp: u64, rating: Rating) -> Self {
        if self.last_reviewed == 0 {
            // new card
            return Self {
                stability: FSRS_P.init_stability(rating),
                difficulty: FSRS_P.init_difficulty(rating),
                last_reviewed: timestamp,
            };
        }
        let retrievability = self.retrievability(timestamp);
        let new_difficulty = FSRS_P.next_difficulty(self.difficulty, rating);
        let new_stability = match rating {
            Rating::Again => {
                FSRS_P.next_forget_stability(self.difficulty, self.stability, retrievability)
            }
            _ => FSRS_P.next_recall_stability(
                self.difficulty,
                self.stability,
                retrievability,
                rating,
            ),
        };
        Self {
            difficulty: new_difficulty,
            stability: new_stability,
            last_reviewed: timestamp,
        }
    }
    pub fn again(self, timestamp: u64) -> Self {
        self.next(timestamp, Rating::Again)
    }
    pub fn good(self, timestamp: u64) -> Self {
        self.next(timestamp, Rating::Good)
    }
}
