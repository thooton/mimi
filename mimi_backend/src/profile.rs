// The public profile: who a user says they are, and the record of what they
// have actually done.
//
// The two halves are kept apart on purpose. `Profile` is authored (a display
// name, a bio, a self-reported CEFR level, a link to a picture) and the server
// stores it and hands it back. Nothing in it is checked against reality,
// because none of it can be; what is checked, in `ProfileEdit`, is
// that each field is the kind of thing it claims to be, which is a different
// question and the only one with an answer. Everything else on the profile
// page is derived, and the thing it is derived from is one table: activity.
//
// One row per user, course and day. Not one row per lesson: a profile
// never asks "which lesson was that", it asks how many, on which days, in
// which course, and what came out of it. Overall streaks fold course rows on
// the same day together; language scores keep them apart. `Activity` is the
// whole of one such course-day row.
//
// What a row stores is deltas: what happened that day and nothing more.
// Running totals (how many concepts you know, how many units you've cleared,
// what your score is) are cumulative sums over the rows before it, computed
// on the way out in `History`. That way a change to how a score is calculated
// re-scores the whole history rather than only the days recorded since, the
// same way retrievability is computed when a request is served rather than
// stored.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use crate::user::Outcome;

// unix seconds in a day; days are the unit the whole file works in
pub const DAY: u64 = 86_400;

// where a language's score starts, before any of it has been learnt
const FLOOR: u32 = 400;

// What a finished lesson is worth. XP is a reward rather than a measurement,
// so the activity record stores the facts (lessons and perfect lessons) and
// derives XP from
// this schedule. These are public because the API publishes the same values
// for clients that want to preview the award from a lesson result.
pub const XP_PER_LESSON: u32 = 20;
pub const XP_PER_PERFECT_LESSON: u32 = XP_PER_LESSON * 2;

// a score built on fewer lessons than this is too little evidence to be worth
// trusting, and says so
const PROVISIONAL_LESSONS: u32 = 30;

// the day a timestamp falls on: whole days since the unix epoch, which is
// midnight UTC to midnight UTC. This is the key an activity row is stored
// under, so "everything that happened on the 14th" is one primary-key lookup
// and a range of days is a range of integers.
pub fn day_of(timestamp: u64) -> u32 {
    (timestamp / DAY) as u32
}

// midnight UTC at the start of a day, as unix seconds: the inverse of `day_of`,
// and what the client dates a feed entry by
pub fn day_start(day: u32) -> u64 {
    day as u64 * DAY
}

// --- what a user says about themselves ---

// The authored half of a profile. None of it is checked against anything:
// a bio is a claim, and so is "B2 Spanish". The numbers that can be checked
// are all derived from activity instead, and live in `History`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub display: String,
    // a badge in front of the name ("Tutor"), if they have one. Granted
    // rather than claimed, which is why `ProfileEdit` below cannot write it:
    // a field anybody may type into is not a badge.
    pub title: Option<String>,
    pub bio: String,
    // self-reported, and the outside standard the derived score is read
    // against; the score is ours, CEFR is everyone's
    pub cefr: String,
    // Where the user's picture lives: an absolute https URL to somebody else's
    // server, never a file of ours. Mimi hosts no images, since uploads mean
    // storage, moderation and a bill. The cost is that the string is a claim
    // like the bio (the picture can change or vanish under us) and has to be
    // checked hard before it is handed to a browser as an `<img src>`. See
    // `avatar_url`.
    pub avatar: Option<String>,
    // The exact course the user is learning. A target-language code is not
    // enough: Spanish for English speakers and Spanish for French speakers
    // are different courses with different trees and memories.
    pub course_id: Option<String>,
    // when the account was created, unix seconds
    pub joined: u64,
}

impl Profile {
    // the profile a new account gets: their name and nothing else. A blank
    // profile is a choice rather than a chore: nothing prompts them to fill it
    // in and nothing scores them for having done so.
    pub fn new(username: &str, joined: u64) -> Profile {
        Profile {
            display: username.to_string(),
            title: None,
            bio: String::new(),
            cefr: String::new(),
            avatar: None,
            course_id: None,
            joined,
        }
    }

    // overwrite everything the owner is allowed to say about themselves
    pub fn apply(&mut self, edit: ProfileEdit) {
        self.display = edit.display;
        self.bio = edit.bio;
        self.cefr = edit.cefr;
        self.avatar = edit.avatar;
    }
}

// --- editing it ---

// What the owner of a profile may write, already checked: the only way to
// build one is `ProfileEdit::of`, so a value of this type is a promise that
// the limits below have been applied. `course_id` is not here, since it has a
// writer of its own as the account's course choice rather than anything a
// reader is shown, and neither is `title`, which is granted.

// A name has to fit beside a face on a card and in a leaderboard row, and
// nothing is gained by letting it run past that.
const MAX_DISPLAY: usize = 32;
// Long enough for a few sentences about yourself, short enough that a profile
// stays a page about a person rather than an essay hosted on one.
const MAX_BIO: usize = 300;
// A URL, not a document. Real image URLs are well under this; the limit is
// here so that a stored profile can never be a payload.
const MAX_AVATAR: usize = 300;

// The whole of the scale, and the only values the field is allowed to take.
// This does not check the claim, which nobody can. It checks that the claim is
// a CEFR level at all, so the page can render it as a badge next to a score
// instead of printing whatever somebody typed.
const CEFR_LEVELS: [&str; 6] = ["A1", "A2", "B1", "B2", "C1", "C2"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileEdit {
    pub display: String,
    pub bio: String,
    pub cefr: String,
    pub avatar: Option<String>,
}

impl ProfileEdit {
    // Check one submitted edit. The error is the message the caller is given
    // verbatim, so each says which field is wrong and what would be right.
    //
    // Everything is trimmed and everything is measured in characters rather
    // than bytes: a limit that counted bytes would give a learner writing in
    // Spanish a shorter bio than one writing in English, for no reason
    // anybody could explain.
    pub fn of(
        display: &str,
        bio: &str,
        cefr: &str,
        avatar: Option<&str>,
    ) -> Result<ProfileEdit, String> {
        let display = display.trim();
        if display.is_empty() {
            return Err("a display name cannot be empty".into());
        }
        if display.chars().count() > MAX_DISPLAY {
            return Err(format!(
                "a display name may be at most {MAX_DISPLAY} characters"
            ));
        }
        // A name is one line. Control characters in it would let somebody
        // draw a blank name, or a name with a newline in the middle of a
        // leaderboard row.
        if display.chars().any(char::is_control) {
            return Err("a display name cannot contain control characters".into());
        }
        let bio = bio.trim();
        if bio.chars().count() > MAX_BIO {
            return Err(format!("a bio may be at most {MAX_BIO} characters"));
        }
        // A bio is a paragraph, so newlines are content; nothing else in the
        // control range is.
        if bio.chars().any(|c| c.is_control() && c != '\n') {
            return Err("a bio cannot contain control characters".into());
        }
        let cefr = cefr.trim().to_ascii_uppercase();
        if !cefr.is_empty() && !CEFR_LEVELS.contains(&cefr.as_str()) {
            return Err(format!(
                "a CEFR level must be one of {} (or blank)",
                CEFR_LEVELS.join(", ")
            ));
        }
        Ok(ProfileEdit {
            display: display.to_string(),
            bio: bio.to_string(),
            cefr,
            avatar: avatar.map(avatar_url).transpose()?.flatten(),
        })
    }
}

// Check a linked-to avatar, hard, because this string is handed to every
// reader's browser as a URL to fetch. `Ok(None)` is a blank field, which is
// how a picture is removed.
//
// The rules are narrower than what a URL may legally be, because this is an
// image address rather than the web:
//
// - https only. Not because http would break anything we serve, but
//   because a `javascript:`, `data:` or `blob:` string in this field is an
//   injection waiting for a client that forgets to check, and enumerating the
//   dangerous schemes is a losing game next to naming the one safe one. It
//   also keeps a page served over TLS from being downgraded to mixed content
//   the browser blocks anyway.
// - Printable ASCII, minus the characters that get reinterpreted. No
//   spaces, no control characters, no newline that could smuggle a second
//   line into a header, and none of `<>"'` or a backtick, which are what a
//   naive client rendering this into markup would be broken by. A non-ASCII
//   host is rejected rather than punycoded: doing that properly is a library,
//   and percent-encoding is always available to whoever needs it.
// - A real host, with nobody hiding in front of it. `@` is banned outright, so
//   `https://images.example.com@evil.invalid/x.png`, which fetches from
//   `evil.invalid` while reading as the opposite, cannot be stored at all.
pub fn avatar_url(raw: &str) -> Result<Option<String>, String> {
    let url = raw.trim();
    if url.is_empty() {
        return Ok(None);
    }
    if url.chars().count() > MAX_AVATAR {
        return Err(format!(
            "an avatar URL may be at most {MAX_AVATAR} characters"
        ));
    }
    // the scheme is the one part of a URL that is case-insensitive, and
    // `HTTPS://` is the same promise as `https://`
    let Some(rest) = url
        .get(..8)
        .filter(|scheme| scheme.eq_ignore_ascii_case("https://"))
        .map(|_| &url[8..])
    else {
        return Err("an avatar URL must be an absolute https:// URL".into());
    };
    if !url
        .bytes()
        .all(|b| b.is_ascii_graphic() && !br#"<>"'`\{}|^"#.contains(&b))
    {
        return Err("an avatar URL may only contain printable ASCII URL characters".into());
    }
    // everything before the path, query or fragment is the host we are asking
    // the reader's browser to talk to
    let host = rest
        .split(['/', '?', '#'])
        .next()
        .expect("split always yields one part");
    if host.contains('@') {
        return Err("an avatar URL may not carry credentials before its host".into());
    }
    // A bare name is either a host on the reader's own network or a typo, and
    // an image worth linking to lives on the public internet. This also rules
    // out `https://localhost`, which is a fetch against whoever is reading.
    if !host.contains('.') || host.starts_with('.') || host.ends_with('.') {
        return Err("an avatar URL must name a full host, e.g. https://example.com/me.png".into());
    }
    Ok(Some(url.to_string()))
}

// --- who they follow ---

// One follow, as a profile reads it: who was followed, and on which day.
//
// The record is kept whether or not the follow is still live (see store.rs),
// because this is a log: following somebody is something the user did on a
// day, and unfollowing them later does not mean it never happened, no more
// than un-learning a word could remove the lesson from the record above.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Follow {
    pub day: u32,
    // who was followed: what a link to their profile is built from
    pub username: String,
    // and what they call themselves, read at serving time rather than copied
    // when the follow happened, so a renamed person is not remembered under a
    // name they have stopped using
    pub display: String,
}

// --- what they did ---

// One day of one user's activity in one course: every lesson they finished
// there that day rolled into one record. Finishing a lesson adds its numbers
// to the course-day row (see `absorb`), so it grows through the day and is
// read back whole.
//
// Only deltas live here. "How many concepts does this user know" is not a
// field; it is the sum of `learned` over every row up to that day, which
// `History` computes.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Activity {
    pub lessons: u32,
    // The subset of `lessons` answered without a single mistake. This must be
    // recorded per lesson: once several lessons have been folded into a day,
    // its aggregate correct/total counts cannot reveal which were perfect.
    pub perfect_lessons: u32,
    // exercises answered, and how many of those were right. Material isn't
    // answered, so it doesn't count towards either.
    pub exercises: u32,
    pub correct: u32,
    // the concepts the user met for the first time that day, in the order
    // they met them: the point of the day, and what the feed quotes
    #[serde(default)]
    pub learned: Vec<String>,
    // units finished that day, by name (from the course outline)
    #[serde(default)]
    pub skills: Vec<String>,
}

impl Activity {
    // What one finished lesson comes to. The server records this against the
    // day the lesson was actually finished; the seeded example account (see
    // seed.rs) records the same thing against the day it is pretending to
    // have been, which is the whole reason this is one function.
    pub fn of_lesson(outcome: &Outcome, skill: Option<String>) -> Activity {
        Activity {
            lessons: 1,
            // A lesson with no questions has no answers to perfect. The
            // serving path currently rejects empty lessons, but keeping the
            // condition here makes the award's meaning explicit.
            perfect_lessons: u32::from(outcome.total > 0 && outcome.correct == outcome.total),
            exercises: outcome.total as u32,
            correct: outcome.correct as u32,
            learned: outcome.learned.clone(),
            skills: skill.into_iter().collect(),
        }
    }

    // A day with nothing in it isn't a day the user was active, and must not
    // be stored: an empty row would be an unbroken link in a streak that was
    // in fact broken. Submitting a lesson with no exercises and no new
    // concepts is the case this guards.
    pub fn is_empty(&self) -> bool {
        self.lessons == 0
            && self.exercises == 0
            && self.learned.is_empty()
            && self.skills.is_empty()
    }

    // fold another lesson's worth of activity into this day
    pub fn absorb(&mut self, other: Activity) {
        self.lessons += other.lessons;
        self.perfect_lessons += other.perfect_lessons;
        self.exercises += other.exercises;
        self.correct += other.correct;
        self.learned.extend(other.learned);
        self.skills.extend(other.skills);
    }

    // what the day was worth, in XP
    pub fn xp(&self) -> u32 {
        let normal_lessons = self.lessons - self.perfect_lessons;
        normal_lessons * XP_PER_LESSON + self.perfect_lessons * XP_PER_PERFECT_LESSON
    }
}

// What the user had got through as of some day: the raw material of a score,
// and a running total rather than anything stored.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Counts {
    // individual things known: words, endings, particles
    pub words: u32,
    // units cleared end to end
    pub skills: u32,
    pub lessons: u32,
}

impl Counts {
    // The single number the graph is drawn from.
    //
    // A language gets a score the way a Duolingo course does, because it is
    // the only way to put "two years of Spanish" and "six weeks of French" on
    // one axis, which is what a graph of several of them needs. The weights
    // say what the app thinks learning is: concepts dominate, because they
    // are what you actually know; a unit is a milestone worth a visible jump;
    // a lesson is the grind that moves it slowly.
    //
    // Deliberately not a function of the FSRS state, even though that is the
    // richer signal: a score that fell while you slept, because retrievability
    // decayed, would read as a punishment for not studying, and this number is
    // shown to other people. What decays belongs in the lesson builder, where
    // forgetting is acted on.
    pub fn score(&self) -> u32 {
        let raw = FLOOR as f64
            + self.words as f64 * 1.6
            + self.skills as f64 * 40.0
            + self.lessons as f64 * 0.5;
        raw.round() as u32
    }
}

// One day of the record, as the profile reads it: what happened, where that
// left the user, and how long a run it was part of.
#[derive(Debug, Clone)]
pub struct Day {
    pub day: u32,
    pub activity: Activity,
    // the running totals at the end of this day, so a score is a plain read
    pub counts: Counts,
    // how many days in a row had been kept up to and including this one
    pub streak: u32,
}

// A user's whole activity record, oldest day first, with the running totals
// filled in. Everything the profile reports is a read over this.
pub struct History {
    pub days: Vec<Day>,
}

impl History {
    // Build the record from the stored rows, in whatever order they arrive.
    // The rows are deltas; this is the one pass that turns them into totals,
    // and every derivation below reads the result rather than re-scanning.
    //
    // The one thing it edits on the way through is repeated unit clears: a
    // user sitting on the last lesson of the course re-reports finishing its
    // unit every time they re-take it (see `User::submit_lesson`). A unit is
    // cleared once, on the day it was first cleared, so the later mentions
    // are dropped here rather than being kept out of the log: the rows record
    // what was reported, and deciding what it means belongs on the way out.
    pub fn of(mut rows: Vec<(u32, Activity)>) -> History {
        rows.sort_by_key(|(day, _)| *day);
        let mut counts = Counts::default();
        let mut cleared: HashSet<String> = HashSet::new();
        let mut days: Vec<Day> = Vec::with_capacity(rows.len());
        for (day, mut activity) in rows {
            activity
                .skills
                .retain(|skill| cleared.insert(skill.clone()));
            counts.words += activity.learned.len() as u32;
            counts.skills += activity.skills.len() as u32;
            counts.lessons += activity.lessons;
            // a run continues only across consecutive days; any gap at all
            // starts the count again at one
            let streak = match days.last() {
                Some(prev) if prev.day + 1 == day => prev.streak + 1,
                _ => 1,
            };
            days.push(Day {
                day,
                activity,
                counts,
                streak,
            });
        }
        History { days }
    }

    // where the user stands now
    pub fn counts(&self) -> Counts {
        self.days.last().map_or(Counts::default(), |d| d.counts)
    }

    // The streak the user is on, which is not the same as the last day's
    // run: a streak is a live thing that a missed day breaks. Today not being
    // over yet is why yesterday still counts: someone who studied yesterday
    // and hasn't opened the app this morning has not lost anything.
    pub fn streak(&self, today: u32) -> u32 {
        match self.days.last() {
            Some(last) if last.day + 1 >= today => last.streak,
            _ => 0,
        }
    }

    // the last day the user did anything, as unix seconds
    pub fn last_active(&self) -> Option<u64> {
        self.days.last().map(|d| day_start(d.day))
    }

    // the totals over the whole record
    pub fn xp(&self) -> u32 {
        self.days.iter().map(|d| d.activity.xp()).sum()
    }

    pub fn exercises(&self) -> (u32, u32) {
        self.days.iter().fold((0, 0), |(total, right), d| {
            (total + d.activity.exercises, right + d.activity.correct)
        })
    }

    // Where the score stood at the end of `day`: the last recorded day at or
    // before it, since a day with no activity leaves the score exactly where
    // the day before it did. `None` if the record doesn't reach back that far,
    // which is how "this predates the account" is spelled.
    pub fn score_at(&self, day: u32) -> Option<u32> {
        self.days
            .iter()
            .take_while(|d| d.day <= day)
            .last()
            .map(|d| d.counts.score())
    }

    // Movement over the last week, in points: what the profile shows beside
    // the current score. A week the user was away is a flat 0 rather than a
    // fall, since the score doesn't decay (see `Counts::score`).
    pub fn week_delta(&self, today: u32) -> i32 {
        let now = self.counts().score() as i32;
        now - self.score_at(today.saturating_sub(7)).unwrap_or(FLOOR) as i32
    }

    // too few lessons for the score to mean much yet
    pub fn provisional(&self) -> bool {
        self.counts().lessons < PROVISIONAL_LESSONS
    }

    // The score history as a line that spans the whole axis, `(timestamp,
    // score)` oldest first: an anchor on the day the user joined (nothing
    // learnt yet, so the floor), a sample for every day they were active, and
    // a last one holding the current score out to the right-hand edge. Idle
    // days in between need no sample, since the score doesn't move on its own.
    //
    // One point per date, at day resolution like everything else in the record.
    // Several of the three sources can land on the same day (someone who joined
    // and studied the same morning, or a today they have already studied) and a
    // graph wants the day's final score, so a repeat overwrites rather than
    // stacking a vertical step onto one date.
    pub fn score_points(&self, since: u64, today: u32) -> Vec<(u64, u32)> {
        let mut points: Vec<(u64, u32)> = Vec::with_capacity(self.days.len() + 2);
        let mut push = |t: u64, v: u32| match points.last_mut() {
            Some(last) if last.0 == t => last.1 = v,
            _ => points.push((t, v)),
        };
        push(since, Counts::default().score());
        for day in &self.days {
            push(day_start(day.day), day.counts.score());
        }
        push(day_start(today), self.counts().score());
        points
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn active(day: u32, lessons: u32, learned: &[&str]) -> (u32, Activity) {
        (
            day,
            Activity {
                lessons,
                perfect_lessons: 0,
                exercises: lessons * 10,
                correct: lessons * 9,
                learned: learned.iter().map(|c| c.to_string()).collect(),
                skills: Vec::new(),
            },
        )
    }

    #[test]
    fn a_day_is_midnight_utc_to_midnight_utc() {
        assert_eq!(day_of(0), 0);
        assert_eq!(day_of(DAY - 1), 0);
        assert_eq!(day_of(DAY), 1);
        assert_eq!(day_start(day_of(1_700_000_123)), 1_699_920_000);
    }

    #[test]
    fn absorbing_adds_a_lesson_to_the_day() {
        let mut day = Activity::default();
        assert!(day.is_empty());
        day.absorb(active(0, 1, &["a"]).1);
        day.absorb(active(0, 1, &["b"]).1);
        assert!(!day.is_empty());
        assert_eq!(day.lessons, 2);
        assert_eq!(day.perfect_lessons, 0);
        assert_eq!(day.exercises, 20);
        assert_eq!(day.correct, 18);
        assert_eq!(day.learned, ["a", "b"]);
    }

    // a submission that taught nothing and asked nothing is not a day of
    // activity, and storing it would forge a link in a streak
    #[test]
    fn a_day_with_nothing_in_it_is_empty() {
        assert!(Activity::default().is_empty());
        assert!(
            !Activity {
                skills: vec!["Unit 1".to_string()],
                ..Activity::default()
            }
            .is_empty()
        );
    }

    // the stored rows are deltas; the totals are the running sums of them
    #[test]
    fn counts_accumulate_over_the_days() {
        let history = History::of(vec![
            active(10, 2, &["a", "b"]),
            active(11, 1, &["c"]),
            active(12, 3, &[]),
        ]);
        assert_eq!(history.days[0].counts.words, 2);
        assert_eq!(history.days[1].counts.words, 3);
        assert_eq!(
            history.counts(),
            Counts {
                words: 3,
                skills: 0,
                lessons: 6
            }
        );
    }

    #[test]
    fn rows_are_sorted_before_they_are_summed() {
        let history = History::of(vec![active(12, 1, &["c"]), active(10, 1, &["a"])]);
        assert_eq!(history.days[0].day, 10);
        assert_eq!(history.days[0].counts.words, 1);
    }

    // a run is consecutive days and nothing else
    #[test]
    fn a_gap_starts_the_streak_again() {
        let history = History::of(vec![
            active(10, 1, &[]),
            active(11, 1, &[]),
            active(13, 1, &[]),
            active(14, 1, &[]),
        ]);
        let streaks: Vec<u32> = history.days.iter().map(|d| d.streak).collect();
        assert_eq!(streaks, [1, 2, 1, 2]);
    }

    // the streak the user is on: today's run, or yesterday's if today hasn't
    // happened yet, and nothing at all once a day has been missed
    #[test]
    fn the_current_streak_survives_today_but_not_a_missed_day() {
        let history = History::of(vec![active(10, 1, &[]), active(11, 1, &[])]);
        assert_eq!(history.streak(11), 2); // studied today
        assert_eq!(history.streak(12), 2); // studied yesterday, today is young
        assert_eq!(history.streak(13), 0); // missed a day
        assert_eq!(History::of(Vec::new()).streak(13), 0);
    }

    // an idle day holds the score where the last active day left it
    #[test]
    fn a_score_is_read_back_at_any_day() {
        let history = History::of(vec![active(10, 1, &["a"]), active(12, 1, &["b"])]);
        assert_eq!(history.score_at(9), None); // before the record starts
        assert_eq!(history.score_at(10), history.score_at(11));
        assert!(history.score_at(12).unwrap() > history.score_at(11).unwrap());
    }

    // a week away is flat, not a fall: the score is what you have learnt, and
    // you don't unlearn it by taking a holiday
    #[test]
    fn the_week_delta_is_movement_not_decay() {
        let history = History::of(vec![active(100, 4, &["a"]), active(101, 4, &["b"])]);
        assert!(history.week_delta(101) > 0);
        assert_eq!(history.week_delta(120), 0);
    }

    #[test]
    fn totals_add_up_over_the_record() {
        let history = History::of(vec![active(10, 2, &[]), active(11, 1, &[])]);
        assert_eq!(history.exercises(), (30, 27));
        assert_eq!(history.xp(), 3 * XP_PER_LESSON);
        assert_eq!(history.last_active(), Some(day_start(11)));
    }

    #[test]
    fn a_perfect_lesson_gets_the_perfect_award() {
        let outcome = Outcome {
            correct: 10,
            total: 10,
            learned: Vec::new(),
            cleared_skill: None,
            passed: None,
        };
        let mut day = Activity::of_lesson(&outcome, None);
        assert_eq!(day.perfect_lessons, 1);
        assert_eq!(day.xp(), XP_PER_PERFECT_LESSON);

        // Perfection belongs to each lesson, not the day's aggregate result.
        day.absorb(active(10, 1, &[]).1);
        assert_eq!(day.xp(), XP_PER_PERFECT_LESSON + XP_PER_LESSON);
    }

    #[test]
    fn a_lesson_with_any_mistake_gets_the_normal_award() {
        let outcome = Outcome {
            correct: 9,
            total: 10,
            learned: Vec::new(),
            cleared_skill: None,
            passed: None,
        };
        let activity = Activity::of_lesson(&outcome, None);
        assert_eq!(activity.perfect_lessons, 0);
        assert_eq!(activity.xp(), XP_PER_LESSON);
    }

    #[test]
    fn an_empty_record_reports_nothing_rather_than_failing() {
        let history = History::of(Vec::new());
        assert_eq!(history.counts(), Counts::default());
        assert_eq!(history.counts().score(), FLOOR);
        assert_eq!(history.last_active(), None);
        assert_eq!(history.xp(), 0);
        assert_eq!(history.week_delta(100), 0);
        assert!(history.provisional());
    }

    // --- editing the authored half ---

    #[test]
    fn an_edit_trims_what_it_stores_and_normalises_the_level() {
        let edit = ProfileEdit::of("  Sam  ", "  learning Spanish  ", " b2 ", None).unwrap();
        assert_eq!(edit.display, "Sam");
        assert_eq!(edit.bio, "learning Spanish");
        assert_eq!(edit.cefr, "B2");
        assert_eq!(edit.avatar, None);
        // a blank level is the ordinary state of the field, not an error
        assert_eq!(ProfileEdit::of("Sam", "", "", None).unwrap().cefr, "");
    }

    // Nobody can check the claim "B2 Spanish"; what can be checked is that
    // it is a level at all, so the page can render it as a badge rather than
    // printing whatever was typed.
    #[test]
    fn a_level_outside_the_scale_is_not_a_level() {
        assert!(ProfileEdit::of("Sam", "", "fluent", None).is_err());
        assert!(ProfileEdit::of("Sam", "", "B3", None).is_err());
    }

    #[test]
    fn a_name_has_to_be_a_name() {
        assert!(ProfileEdit::of("   ", "", "", None).is_err());
        assert!(ProfileEdit::of(&"x".repeat(MAX_DISPLAY + 1), "", "", None).is_err());
        assert!(ProfileEdit::of("Sam\nJones", "", "", None).is_err());
        // the limits count characters, so an accent costs what a letter costs
        assert!(ProfileEdit::of(&"é".repeat(MAX_DISPLAY), "", "", None).is_ok());
        assert!(ProfileEdit::of("Sam", &"é".repeat(MAX_BIO), "", None).is_ok());
        assert!(ProfileEdit::of("Sam", &"x".repeat(MAX_BIO + 1), "", None).is_err());
    }

    // A bio is a paragraph; a name is a line.
    #[test]
    fn a_bio_may_have_line_breaks_but_no_other_control_characters() {
        assert!(ProfileEdit::of("Sam", "one\ntwo", "", None).is_ok());
        assert!(ProfileEdit::of("Sam", "one\u{7}two", "", None).is_err());
    }

    // The one field that is handed to every reader's browser as something to
    // fetch, so this is the list that matters.
    #[test]
    fn an_avatar_must_be_an_https_url_and_nothing_cleverer() {
        assert_eq!(
            avatar_url("  https://cdn.example.com/me.png?v=2  ").unwrap(),
            Some("https://cdn.example.com/me.png?v=2".to_string())
        );
        // a scheme is case-insensitive; the rest is stored exactly as typed
        assert!(avatar_url("HTTPS://cdn.example.com/me.png").is_ok());
        // blank clears the picture rather than failing
        assert_eq!(avatar_url("   ").unwrap(), None);

        // the schemes that make this field an injection if anything downstream
        // forgets to look
        assert!(avatar_url("javascript:alert(1)").is_err());
        assert!(avatar_url("data:image/svg+xml;base64,AAAA").is_err());
        assert!(avatar_url("//cdn.example.com/me.png").is_err());
        // and plain http, which a page served over TLS could not load anyway
        assert!(avatar_url("http://cdn.example.com/me.png").is_err());

        // reads as example.com, fetches from evil.invalid
        assert!(avatar_url("https://cdn.example.com@evil.invalid/me.png").is_err());
        // a host on the reader's own machine or network is not a picture
        assert!(avatar_url("https://localhost/me.png").is_err());

        // characters that break out of an attribute, or a header
        assert!(avatar_url("https://cdn.example.com/\"onerror=x").is_err());
        assert!(avatar_url("https://cdn.example.com/a b.png").is_err());
        assert!(avatar_url("https://cdn.example.com/a\nb.png").is_err());
        assert!(avatar_url("https://cdn.example.com/ñ.png").is_err());

        assert!(
            avatar_url(&format!(
                "https://cdn.example.com/{}.png",
                "x".repeat(MAX_AVATAR)
            ))
            .is_err()
        );
    }

    // the weights are the app's opinion of what learning is; this pins them
    #[test]
    fn a_score_weights_concepts_over_lessons() {
        let concepts = Counts {
            words: 10,
            skills: 0,
            lessons: 0,
        };
        let lessons = Counts {
            words: 0,
            skills: 0,
            lessons: 10,
        };
        assert!(concepts.score() > lessons.score());
        assert_eq!(Counts::default().score(), FLOOR);
    }
}
