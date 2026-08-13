// The weekly board: one global ranking of the XP earned since Monday.
//
// There is no leagues machinery and no stored board. A standing is a sum over
// the activity rows of one week, computed when the request is served: the
// same principle as the profile's running totals and a card's retrievability,
// and for the same reason: a stored board would be a second copy of the
// activity table that could disagree with it. Rewriting the XP schedule
// (`profile::XP_PER_LESSON`) re-scores this week's board along with every
// profile, because both are reading the same rows through `Activity::xp`.
//
// Nothing is written when the week turns over, either. "Resets Monday" is not
// an event: it is the arithmetic in `week_start` choosing a different range of
// days, so a board that nobody asked for costs nothing and a server that was
// switched off over the weekend has nothing to catch up on.
//
// Guests are not ranked. A guest is a real learning record (see store.rs),
// but it is one nobody has put a name to and one that disappears with its
// cookie, so a name on a public board is the one thing it should not get. The
// exclusion is a join against `users.guest` rather than a filter on the
// `guest~` prefix, and because claiming a guest is a rename, the week they
// spent trying the course arrives on the board with them the moment they
// register.

use std::collections::HashMap;

use crate::profile::Activity;

// Days are days since the unix epoch (see `profile::day_of`), and day 0 was a
// Thursday, so a day's weekday is `(day + 3) % 7`, counting from Monday.
const EPOCH_WEEKDAY: u32 = 3;
const WEEK: u32 = 7;

// The Monday on or before `day`, as a day. Sunday belongs to the week that
// began six days earlier, which is what makes a "week" here run Monday to
// Sunday inclusive.
//
// The saturating subtraction only bites in the first days of 1970, where the
// true Monday predates the epoch; clamping to day 0 there is meaningless
// rather than wrong, and it keeps the whole file in unsigned days.
pub fn week_start(day: u32) -> u32 {
    day.saturating_sub((day + EPOCH_WEEKDAY) % WEEK)
}

// The Monday the current week gives way to, which is when the board empties.
pub fn week_end(week_start: u32) -> u32 {
    week_start + WEEK
}

// One learner's week. `rank` is competition rank: equal XP shares a position
// and the next one down skips, so the board never claims a difference the
// numbers don't support.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Standing {
    pub rank: u32,
    pub username: String,
    // what the profile calls them; the username is what identifies them
    pub display: String,
    pub xp: u32,
}

pub struct Leaderboard {
    // the Monday this week began, as a day
    pub week_start: u32,
    pub standings: Vec<Standing>,
}

impl Leaderboard {
    // Rank one week's activity rows: `(username, display, activity)`, in any
    // order and several per user, exactly as the store hands them over.
    //
    // A learner with no XP this week is not on the board at all. The board
    // ranks what was earned since Monday, and a wall of zeroes ranks nothing.
    // It is also the honest reading of "hasn't started this week yet", which
    // an entry at position 4,000 would dress up as a placing.
    pub fn of(rows: Vec<(String, String, Activity)>, week_start: u32) -> Leaderboard {
        let mut weeks: HashMap<String, Standing> = HashMap::new();
        for (username, display, activity) in rows {
            let standing = weeks.entry(username.clone()).or_insert_with(|| Standing {
                rank: 0,
                username,
                display,
                xp: 0,
            });
            standing.xp += activity.xp();
        }

        let mut standings: Vec<Standing> = weeks.into_values().filter(|s| s.xp > 0).collect();
        // Most XP first; ties by name, so that two learners on the same total
        // hold the same order every time the page is opened. A HashMap's
        // iteration order is not stable across runs, and without the second
        // key the board would shuffle under a reader who changed nothing.
        standings.sort_by(|a, b| b.xp.cmp(&a.xp).then_with(|| a.username.cmp(&b.username)));

        let mut rank = 0;
        let mut previous_xp = None;
        for (index, standing) in standings.iter_mut().enumerate() {
            if previous_xp != Some(standing.xp) {
                rank = index as u32 + 1;
                previous_xp = Some(standing.xp);
            }
            standing.rank = rank;
        }

        Leaderboard {
            week_start,
            standings,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day_of_activity(lessons: u32, perfect: u32) -> Activity {
        Activity {
            lessons,
            perfect_lessons: perfect,
            exercises: lessons * 10,
            correct: lessons * 9,
            learned: Vec::new(),
            skills: Vec::new(),
        }
    }

    fn row(username: &str, lessons: u32) -> (String, String, Activity) {
        (
            username.to_string(),
            username.to_string(),
            day_of_activity(lessons, 0),
        )
    }

    // The board's whole clock. Day 4 was Monday 5 January 1970, so every
    // assertion here is a date that can be checked by hand.
    #[test]
    fn a_week_runs_from_monday_to_sunday() {
        assert_eq!(week_start(4), 4); // Monday is its own week's start
        assert_eq!(week_start(5), 4); // Tuesday
        assert_eq!(week_start(10), 4); // the Sunday after, still that week
        assert_eq!(week_start(11), 11); // and the next Monday turns it over
        assert_eq!(week_end(4), 11);
    }

    // A recent Monday, to prove the arithmetic doesn't only work near the
    // epoch: 1 700 000 000 is Tuesday 14 November 2023, whose Monday is the
    // 13th (day 19 674).
    #[test]
    fn a_modern_date_lands_on_its_own_monday() {
        let tuesday = crate::profile::day_of(1_700_000_000);
        assert_eq!(week_start(tuesday), 19_674);
        assert_eq!(crate::profile::day_start(19_674), 1_699_833_600);
    }

    // Every day of the week counts towards one total, and the XP schedule is
    // the profile's, not a second copy of it.
    #[test]
    fn a_standing_is_the_weeks_days_added_up() {
        let board = Leaderboard::of(vec![row("ren", 2), row("ren", 3)], 4);
        assert_eq!(board.standings.len(), 1);
        assert_eq!(board.standings[0].xp, 5 * crate::profile::XP_PER_LESSON);
    }

    #[test]
    fn the_board_is_ordered_by_xp_and_ranked_from_one() {
        let board = Leaderboard::of(vec![row("ren", 1), row("clover", 3), row("aiko", 2)], 4);
        let names: Vec<&str> = board
            .standings
            .iter()
            .map(|s| s.username.as_str())
            .collect();
        assert_eq!(names, ["clover", "aiko", "ren"]);
        assert_eq!(board.standings[0].rank, 1);
        assert_eq!(board.standings[2].rank, 3);
    }

    // Equal weeks are equal placings, and the position after a shared one
    // skips, or two learners tied for first would be followed by a "second"
    // who was in fact beaten by both.
    #[test]
    fn a_tie_shares_a_rank_and_the_next_one_skips_it() {
        let board = Leaderboard::of(vec![row("ren", 2), row("clover", 2), row("aiko", 1)], 4);
        let ranks: Vec<u32> = board.standings.iter().map(|s| s.rank).collect();
        assert_eq!(ranks, [1, 1, 3]);
        // and the tie is broken by name, so the order is the same every time
        assert_eq!(board.standings[0].username, "clover");
    }

    // A perfect lesson is worth double, and the board reads that through
    // `Activity::xp` rather than counting lessons itself.
    #[test]
    fn perfect_lessons_are_worth_what_the_profile_says_they_are() {
        let board = Leaderboard::of(
            vec![
                ("ren".into(), "Ren".into(), day_of_activity(2, 2)),
                ("aiko".into(), "Aiko".into(), day_of_activity(3, 0)),
            ],
            4,
        );
        // two perfect lessons (80) beat three ordinary ones (60)
        assert_eq!(board.standings[0].username, "ren");
        assert_eq!(
            board.standings[0].xp,
            2 * crate::profile::XP_PER_PERFECT_LESSON
        );
    }

    // Somebody who has done nothing since Monday has no placing to show, and
    // an empty week is an empty board rather than a failure.
    #[test]
    fn a_week_with_no_xp_in_it_is_not_a_placing() {
        let board = Leaderboard::of(vec![row("ren", 0)], 4);
        assert!(board.standings.is_empty());
        assert!(Leaderboard::of(Vec::new(), 4).standings.is_empty());
    }

    // The display name travels with the standing, because a board shows what
    // people call themselves, but the username is what identifies the row.
    #[test]
    fn a_standing_carries_both_names() {
        let board = Leaderboard::of(
            vec![("aiko".into(), "Aiko".into(), day_of_activity(1, 0))],
            4,
        );
        assert_eq!(board.standings[0].username, "aiko");
        assert_eq!(board.standings[0].display, "Aiko");
    }
}
