use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::card::Card;
use crate::course::Course;
use crate::dictionary::Dictionary;
use crate::exercise::{Ask, FlashcardDirection, Side};
use crate::leaderboard::{Leaderboard, week_end};
use crate::lesson::{Lesson, Task};
use crate::position::Position;
use crate::profile::{
    Counts, Follow, History, Profile, ProfileEdit, XP_PER_LESSON, XP_PER_PERFECT_LESSON, day_start,
};
use crate::quests::DailyQuests;
use crate::sentence::Phrasing;
use crate::skill::Skill;
use crate::user::{
    FLASHCARD_BATCH_SIZE, FlashcardReport, QuestionReport, SkillState, Submission, User,
};
use crate::word::{Stage, WordState};

const FEED_DAYS: usize = 60;

#[derive(Debug, Deserialize)]
pub struct SetCourseRequest {
    pub course_id: String,
}

#[derive(Debug, Serialize)]
pub struct CourseSummaryView {
    pub id: String,
    pub source_lang: String,
    pub target_lang: String,
}

impl CourseSummaryView {
    pub fn of(course: &Course) -> Self {
        Self {
            id: course.id.clone(),
            source_lang: course.source_lang.clone(),
            target_lang: course.target_lang.clone(),
        }
    }
}

// The two settings a learner may edit about their account. Neither body names
// an account: the session says who is asking, and the current password says
// they are still the person who signed in. There is deliberately no request
// for a username: it is the name a profile, a leaderboard row and every link
// to either are written against, so it is fixed at registration.
#[derive(Debug, Deserialize)]
pub struct SetPasswordRequest {
    pub current_password: String,
    pub new_password: String,
}

#[derive(Debug, Deserialize)]
pub struct SetEmailRequest {
    pub password: String,
    pub email: String,
}

/// Everything the owner of a profile may write, in one body: the editor is a
/// form, and a form is submitted whole. Sending a field back unchanged is
/// therefore normal and means what it says, while an absent one is a bad
/// request rather than "leave it alone", since a partial edit would make it
/// impossible to clear a field, which is the whole of removing a picture.
///
/// `avatar` is the exception, and null there does mean cleared: it is the one
/// field whose empty value is the absence of a thing rather than an empty
/// string. `title` and `course_id` are absent because neither is the
/// editor's to write (see `Profile` and `Store::save_course`).
#[derive(Debug, Deserialize)]
pub struct EditProfileRequest {
    pub display: String,
    pub bio: String,
    pub cefr: String,
    pub avatar: Option<String>,
}

impl EditProfileRequest {
    /// Check the submitted edit. The `Err` is the message the caller gets.
    pub fn into_edit(self) -> Result<ProfileEdit, String> {
        ProfileEdit::of(&self.display, &self.bio, &self.cefr, self.avatar.as_deref())
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateLessonRequest {
    pub skill: String,
    pub lesson: u8,
}

impl From<CreateLessonRequest> for Position {
    fn from(value: CreateLessonRequest) -> Self {
        Position::new(value.skill, value.lesson)
    }
}

#[derive(Debug, Deserialize)]
pub struct SubmitRequest {
    pub target: SubmitTarget,
    pub questions: Vec<QuestionItem>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SubmitTarget {
    Skill { skill: String, lesson: u8 },
    Castle { castle: usize },
}

#[derive(Debug, Deserialize)]
pub struct QuestionItem {
    pub ask: Ask,
    pub correct: bool,
    pub words: HashMap<String, bool>,
}

impl SubmitRequest {
    pub fn into_submission(self) -> Submission {
        let target = match self.target {
            SubmitTarget::Skill { skill, lesson } => {
                crate::lesson::Target::Skill(Position::new(skill, lesson))
            }
            SubmitTarget::Castle { castle } => crate::lesson::Target::Castle(castle),
        };
        Submission {
            target,
            questions: self
                .questions
                .into_iter()
                .map(|question| QuestionReport {
                    ask: question.ask,
                    correct: question.correct,
                    words: question.words,
                })
                .collect(),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct SubmitFlashcardsRequest {
    pub cards: Vec<FlashcardItem>,
}

#[derive(Debug, Deserialize)]
pub struct FlashcardItem {
    pub word: String,
    pub direction: FlashcardDirection,
    pub correct: bool,
}

impl SubmitFlashcardsRequest {
    pub fn into_reports(self) -> Vec<FlashcardReport> {
        self.cards
            .into_iter()
            .map(|card| FlashcardReport {
                word: card.word,
                direction: card.direction,
                correct: card.correct,
            })
            .collect()
    }
}

#[derive(Debug, Serialize)]
pub struct ErrorBody {
    pub error: String,
}

#[derive(Debug, Serialize)]
pub struct UserView {
    pub username: String,
    pub progress: HashMap<String, u8>,
    pub castles: usize,
    pub words: Vec<WordView>,
}

impl UserView {
    pub fn of(username: String, user: &User, timestamp: u64) -> Self {
        let mut words: Vec<_> = user
            .words
            .iter()
            .map(|(id, state)| WordView::of(id, state, timestamp))
            .collect();
        words.sort_by(|a, b| a.word.cmp(&b.word));
        Self {
            username,
            progress: user.progress.clone(),
            castles: user.castles,
            words,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct WordView {
    pub word: String,
    pub stage: Stage,
    pub streak: u32,
    pub lapses: u32,
    pub repromoting: bool,
    pub bank: Option<CardView>,
    pub recognition: Option<CardView>,
    pub production: Option<CardView>,
}

impl WordView {
    fn of(word: &str, state: &WordState, timestamp: u64) -> Self {
        let card = |value: Option<Card>| value.map(|c| CardView::of(c, timestamp));
        Self {
            word: word.into(),
            stage: state.stage,
            streak: state.streak,
            lapses: state.lapses,
            repromoting: state.repromoting,
            bank: card(state.bank),
            recognition: card(state.recognition),
            production: card(state.production),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct CardView {
    pub retrievability: f64,
    pub stability: f64,
    pub difficulty: f64,
    pub last_reviewed: u64,
}
impl CardView {
    fn of(c: Card, at: u64) -> Self {
        Self {
            retrievability: c.retrievability(at),
            stability: c.stability,
            difficulty: c.difficulty,
            last_reviewed: c.last_reviewed,
        }
    }
}

// One batch of standalone practice, not a deck the learner is expected to
// finish: the client asks again whenever it runs low. Because the cards are
// the most urgent ones the learner has, a client that asks for more before
// reporting the batch it holds is handed the same cards back: the verdicts are
// what reorder the vocabulary and make the next batch different. An empty
// `cards` therefore means the learner has met no vocabulary at all, and is
// the only thing that ever ends a run.
#[derive(Debug, Serialize)]
pub struct FlashcardDeckView {
    pub target_lang: String,
    pub cards: Vec<FlashcardView>,
}

#[derive(Debug, Serialize)]
pub struct FlashcardView {
    pub word: String,
    pub direction: FlashcardDirection,
    pub language_direction: String,
    pub front: String,
    pub back: String,
    pub example: Option<String>,
}

impl FlashcardDeckView {
    pub fn of(course: &Course, user: &User, timestamp: u64) -> Self {
        let cards = user
            .flashcards(timestamp)
            .into_iter()
            .filter_map(|(id, direction)| {
                let word = course.vocab.get(id)?;
                // One meaning, not the word's whole gloss list. A card is a
                // single question with a single answer: "hello, hi, hey" on
                // the back invites the learner to grade themselves against
                // whichever of the three they happened to think of, and on the
                // front it is worse still, because a source→target prompt
                // listing three synonyms does not say which one to produce.
                // `glosses` is ordered, so the first is the dictionary sense.
                // A word the glossary never defined has none, and a card whose
                // answer would be blank is no card at all.
                let meaning = word.glosses.first()?.clone();
                let (front, back) = match direction {
                    FlashcardDirection::TargetToSource => (word.word.clone(), meaning),
                    FlashcardDirection::SourceToTarget => (meaning, word.word.clone()),
                };
                Some(FlashcardView {
                    word: id.to_string(),
                    direction,
                    language_direction: course.flashcard_direction_of(direction),
                    front,
                    back,
                    example: course.example_for(id),
                })
            })
            .take(FLASHCARD_BATCH_SIZE)
            .collect();
        Self {
            target_lang: course.target_lang.clone(),
            cards,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct LessonView {
    pub lesson_id: String,
    pub target: LessonTargetView,
    pub tasks: Vec<TaskView>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LessonTargetView {
    Skill { skill: String, lesson: u8 },
    Castle { castle: usize },
}

impl LessonView {
    pub fn of(
        lesson_id: String,
        course: &Course,
        dictionary: &Dictionary,
        lesson: &Lesson,
    ) -> Option<Self> {
        let target = match &lesson.target {
            crate::lesson::Target::Skill(position) => LessonTargetView::Skill {
                skill: position.skill.clone(),
                lesson: position.lesson,
            },
            crate::lesson::Target::Castle(castle) => LessonTargetView::Castle { castle: *castle },
        };
        Some(Self {
            lesson_id,
            target,
            tasks: lesson
                .tasks
                .iter()
                .map(|t| TaskView::of(course, dictionary, t))
                .collect::<Option<_>>()?,
        })
    }
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", content = "task", rename_all = "snake_case")]
pub enum TaskView {
    Material(MaterialView),
    Exercise(ExerciseView),
}
impl TaskView {
    pub fn of(course: &Course, dictionary: &Dictionary, task: &Task) -> Option<Self> {
        if let Some(material) = task.material(course) {
            return Some(Self::Material(MaterialView {
                text: material.text.clone(),
            }));
        }
        let exercise = task.exercise(course)?;
        let new_words = task.new_words(course)?;
        // the two fields the client sorts on: whether to show a tile board,
        // and which way round to label the question
        let ask = exercise.ask;
        let kind = if ask.tiles() {
            "word_bank"
        } else {
            "translate"
        };
        let bank = if ask.tiles() {
            // the answer's own tiles and then the wrong ones, both cut at load
            // time; the client shuffles the board it is handed
            exercise
                .tiles
                .iter()
                .chain(&exercise.bank)
                .cloned()
                .collect()
        } else {
            Vec::new()
        };
        // the dictionary only speaks the target language, so it annotates
        // whichever side of this question is in it: the prompt while the
        // learner is answering, the answer for the feedback afterwards
        let answer = exercise.answers.first()?.text.clone();
        let spanish = |side: Side, text: &str| {
            if side == Side::Target {
                glosses(dictionary, text)
            } else {
                Vec::new()
            }
        };
        Some(Self::Exercise(ExerciseView {
            id: exercise.id.clone(),
            ask,
            kind: kind.to_string(),
            direction: course.direction_of(ask),
            // An answer's spans give the client precise per-word verdicts
            // where credit can be divided. This explicit list covers the
            // supported fallback, where a word with no span takes the
            // exercise's overall verdict: an unlocatable form, or any word of
            // a one-word question (see loader::wording).
            words: exercise.words,
            prompt_glosses: spanish(ask.shows(), &exercise.prompt),
            answer_glosses: spanish(ask.produces(), &answer),
            prompt: exercise.prompt,
            answers: exercise.answers,
            bank,
            introduces: task.introduces().to_vec(),
            new_words,
        }))
    }
}

#[derive(Debug, Serialize)]
pub struct MaterialView {
    pub text: String,
}

/// Every tip one lesson of a skill would show, without building the lesson.
/// The course map's "Tips" button reads this; material teaches nothing, so
/// showing it early is harmless.
#[derive(Debug, Serialize)]
pub struct TipsView {
    pub tips: Vec<MaterialView>,
}

impl TipsView {
    pub fn of(skill: &Skill, lesson: u8) -> Self {
        Self {
            tips: skill
                .material_for_lesson(lesson)
                .map(|(_, block)| MaterialView {
                    text: block.text.clone(),
                })
                .collect(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ExerciseView {
    pub id: String,
    pub ask: Ask,
    pub kind: String,
    pub direction: String,
    pub words: Vec<String>,
    pub prompt: String,
    // Every accepted answer, preferred first, as `{"text", "words"}`: the text
    // exactly as the learner should produce it, and the spans of it that prove
    // each word, `{"word": "hola", "start": 1, "end": 5}`. Offsets are UTF-16
    // code units, so `text.slice(start, end)` in the browser is the stretch
    // that word owns (see `sentence::Mark`).
    //
    // An empty `words` means grade the whole thing at once and give every word
    // in `words` above that verdict. A one-word question always looks like
    // this.
    pub answers: Vec<Phrasing>,
    pub bank: Vec<String>,
    pub introduces: Vec<String>,
    // The exact runs of `prompt` the learner is meeting for the first time.
    // Kept separate from answer marks (which grade) and prompt glosses (which
    // explain), and present even for the one-word introductions whose answers
    // intentionally carry no grading marks.
    pub new_words: Vec<crate::sentence::Mark>,
    // Dictionary annotations for whichever side is the target language: one of
    // the two, since a question shows one side and asks for the other. Prompt
    // glosses are available while answering; answer glosses
    // support the feedback afterwards. They cover untracked filler words as
    // well as the words the exercise grades.
    pub prompt_glosses: Vec<GlossView>,
    pub answer_glosses: Vec<GlossView>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct GlossView {
    pub text: String,
    pub meanings: Vec<String>,
}

fn glosses(dictionary: &Dictionary, text: &str) -> Vec<GlossView> {
    dictionary
        .annotate(text)
        .into_iter()
        .map(|gloss| GlossView {
            text: gloss.text.to_string(),
            meanings: gloss.meanings,
        })
        .collect()
}

#[derive(Debug, Serialize)]
pub struct SubmitView {
    pub correct: usize,
    pub total: usize,
    pub passed: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct SubmitFlashcardsView {
    pub correct: usize,
    pub total: usize,
}

#[derive(Debug, Serialize)]
pub struct CourseView {
    pub id: String,
    pub source_lang: String,
    pub target_lang: String,
    pub castles: Vec<CastleView>,
}
#[derive(Debug, Serialize)]
pub struct CastleView {
    pub castle: usize,
    pub state: &'static str,
    pub rows: Vec<RowView>,
}
#[derive(Debug, Serialize)]
pub struct RowView {
    pub skills: Vec<SkillView>,
}
#[derive(Debug, Serialize)]
pub struct SkillView {
    pub id: String,
    pub name: String,
    pub focus: String,
    pub state: SkillState,
    pub level: u8,
    pub lessons: u8,
    pub lessons_done: u8,
}

impl CourseView {
    pub fn of(course: &Course, user: &User) -> Self {
        let castles = course
            .castles()
            .iter()
            .map(|castle| {
                let state = if castle.castle < user.castles {
                    "passed"
                } else if user.castle_due(course) == Some(castle.castle) {
                    "available"
                } else {
                    "locked"
                };
                let rows = course.rows()[castle.rows.clone()]
                    .iter()
                    .map(|row| RowView {
                        skills: row
                            .iter()
                            .map(|&i| {
                                let skill = &course.skills()[i];
                                SkillView {
                                    id: skill.id.clone(),
                                    name: skill.name.clone(),
                                    focus: skill.focus.clone(),
                                    state: user.skill_state(course, &skill.id),
                                    level: user.skill_level(&skill.id),
                                    lessons: crate::skill::LESSONS_PER_LEVEL,
                                    lessons_done: user.lessons_done_in_level(&skill.id),
                                }
                            })
                            .collect(),
                    })
                    .collect();
                CastleView {
                    castle: castle.castle,
                    state,
                    rows,
                }
            })
            .collect();
        Self {
            id: course.id.clone(),
            source_lang: course.source_lang.clone(),
            target_lang: course.target_lang.clone(),
            castles,
        }
    }
}

/// The weekly board (see leaderboard.rs): everyone who has earned XP since
/// Monday, best first. The two timestamps are the week's own edges, so a
/// client can say which week it is looking at and when it turns over without
/// deriving Mondays from the browser's clock: the record is kept in UTC days,
/// and a reader east of UTC would otherwise label the board a day out.
#[derive(Debug, Serialize)]
pub struct LeaderboardView {
    pub week_start: u64,
    pub resets_at: u64,
    pub standings: Vec<StandingView>,
}

#[derive(Debug, Serialize)]
pub struct StandingView {
    /// competition rank: equal XP shares a place, and the next one skips
    pub rank: u32,
    /// who this is: what a link to their profile is built from
    pub username: String,
    /// what they call themselves, or the username where they haven't said
    pub display: String,
    /// XP earned since Monday 00:00 UTC, and the only thing ranked
    pub xp: u32,
}

impl LeaderboardView {
    pub fn of(board: &Leaderboard) -> Self {
        Self {
            week_start: day_start(board.week_start),
            resets_at: day_start(week_end(board.week_start)),
            standings: board
                .standings
                .iter()
                .map(|standing| StandingView {
                    rank: standing.rank,
                    username: standing.username.clone(),
                    display: standing.display.clone(),
                    xp: standing.xp,
                })
                .collect(),
        }
    }
}

/// Accounts a name matched, for the inbox's "start a new conversation" box.
///
/// This is the one place the server answers with a list of people nobody
/// named, so it is kept as small as the job needs: a prefix rather than a
/// substring, `SEARCH_LIMIT` of them, and only the two fields a row in the
/// box shows. Guests are absent, having nobody to write to.
///
/// Unlike opening a conversation, searching is a question with one answer
/// that changes nothing, which is exactly what a GET is for (see messages.rs
/// for the inbox's event feed and commands).
#[derive(Debug, Serialize)]
pub struct UserSearchView {
    pub users: Vec<FoundUserView>,
}

#[derive(Debug, Serialize)]
pub struct FoundUserView {
    /// who this is: what a thread and a profile link are both built from
    pub username: String,
    /// what they call themselves, or the username where they haven't said
    pub display: String,
}

impl UserSearchView {
    pub fn of(found: Vec<(String, String)>) -> Self {
        Self {
            users: found
                .into_iter()
                .map(|(username, display)| FoundUserView { username, display })
                .collect(),
        }
    }
}

/// Today's authenticated learner quests. Like the activity table they read,
/// the clock is UTC; both edges are served so the browser never has to guess
/// which local date owns progress close to midnight.
#[derive(Debug, Serialize)]
pub struct DailyQuestsView {
    pub day: u64,
    pub resets_at: u64,
    pub quests: Vec<QuestView>,
}

#[derive(Debug, Serialize)]
pub struct QuestView {
    pub id: &'static str,
    pub title: String,
    pub done: u32,
    pub total: u32,
}

impl DailyQuestsView {
    pub fn of(daily: DailyQuests) -> Self {
        Self {
            day: day_start(daily.day),
            resets_at: day_start(daily.day.saturating_add(1)),
            quests: daily
                .quests
                .into_iter()
                .map(|quest| QuestView {
                    id: quest.id,
                    title: quest.title,
                    done: quest.done,
                    total: quest.total,
                })
                .collect(),
        }
    }
}

/// Who this profile's reader is to its owner, and who its owner is to
/// everyone else. Assembled by the handler from the follows table and handed
/// to the view whole, because three of the four are one question asked from
/// different directions.
pub struct Social {
    pub followers: u32,
    pub following: u32,
    /// whether the account making this request follows the one it is reading.
    /// False for a signed-out reader, who is nobody in particular.
    pub viewer_follows: bool,
    /// every follow this user has ever made, live or since undone: the feed
    /// quotes it, so it is a log rather than the current edge
    pub log: Vec<Follow>,
}

#[derive(Debug, Serialize)]
pub struct ProfileView {
    pub username: String,
    pub display: String,
    pub title: Option<String>,
    pub bio: String,
    pub cefr: String,
    /// An absolute https URL to a picture on somebody else's server, checked
    /// on the way in (see `profile::avatar_url`); Mimi hosts no images. Null
    /// where they haven't linked one, which is most people.
    pub avatar: Option<String>,
    // the course they're learning, or null while they haven't picked: the one
    // authored field with a writer of its own (PUT /me/course)
    pub course_id: Option<String>,
    pub joined: u64,
    /// how many accounts follow this one, and how many it follows
    pub followers: u32,
    pub following: u32,
    /// whether the reader of this response follows it. False when nobody is
    /// signed in, and false on your own profile, since you do not follow
    /// yourself.
    pub viewer_follows: bool,
    /// Whether this process authenticated a request from the account during
    /// the rolling presence window. Unlike `last_active`, this is live,
    /// process-local state and says nothing about whether they studied.
    pub online: bool,
    pub last_active: Option<u64>,
    pub today: u64,
    pub streak: u32,
    pub xp_schedule: XpScheduleView,
    pub totals: TotalsView,
    pub languages: Vec<LanguageView>,
    pub days: Vec<DayView>,
}
#[derive(Debug, Serialize)]
pub struct XpScheduleView {
    pub lesson: u32,
    pub perfect_lesson: u32,
}
#[derive(Debug, Serialize)]
pub struct TotalsView {
    pub xp: u32,
    pub lessons: u32,
    pub exercises: u32,
    pub correct: u32,
    pub words: u32,
    pub skills: u32,
    pub days: u32,
}
#[derive(Debug, Serialize)]
pub struct LanguageView {
    pub id: String,
    pub code: String,
    pub source_code: String,
    pub score: u32,
    pub delta: i32,
    pub provisional: bool,
    pub words: u32,
    pub skills: u32,
    pub lessons: u32,
    pub since: u64,
    pub points: Vec<PointView>,
}
#[derive(Debug, Serialize)]
pub struct PointView {
    pub t: u64,
    pub v: u32,
}
/// One day of the feed. A day here is not necessarily a day of study: a day
/// whose only entry is a follow has no activity row behind it, and reads as
/// zeroes with a `followed` list, which is what happened.
#[derive(Debug, Serialize)]
pub struct DayView {
    pub t: u64,
    pub streak: u32,
    pub lessons: u32,
    pub exercises: u32,
    pub correct: u32,
    pub xp: u32,
    pub learned: Vec<String>,
    pub skills: Vec<String>,
    /// who this user started following that day. Undoing a follow does not
    /// remove it from here: the feed records what was done, not what is still
    /// true.
    pub followed: Vec<FollowView>,
    pub score: u32,
    pub delta: i32,
}

#[derive(Debug, Serialize)]
pub struct FollowView {
    /// what a link to their profile is built from
    pub username: String,
    /// and what they call themselves now, not when they were followed
    pub display: String,
}

impl ProfileView {
    pub fn of(
        username: String,
        profile: Profile,
        history: &History,
        social: Social,
        course_histories: &[(&Course, &History)],
        online: bool,
        today: u32,
    ) -> Self {
        let counts = history.counts();
        let (exercises, correct) = history.exercises();
        let since = day_start(crate::profile::day_of(profile.joined));
        let languages = course_histories
            .iter()
            .map(|(course, course_history)| {
                let course_counts = course_history.counts();
                let course_since = course_history
                    .days
                    .first()
                    .map_or(since, |day| day_start(day.day));
                LanguageView {
                    id: course.id.clone(),
                    code: course.target_lang.clone(),
                    source_code: course.source_lang.clone(),
                    score: course_counts.score(),
                    delta: course_history.week_delta(today),
                    provisional: course_history.provisional(),
                    words: course_counts.words,
                    skills: course_counts.skills,
                    lessons: course_counts.lessons,
                    since: course_since,
                    points: course_history
                        .score_points(course_since, today)
                        .into_iter()
                        .map(|(t, v)| PointView { t, v })
                        .collect(),
                }
            })
            .collect();
        // The feed is the activity record and the follow log read together,
        // so a day either of them mentions is a day of the feed. Following
        // somebody is not folded into the activity table: a row there is a day
        // the learner studied, and it is what the streak and "days studied"
        // are counted from, so a follow must not forge a link in a streak any
        // more than an empty lesson may (see `Activity::is_empty`).
        let mut followed: HashMap<u32, Vec<FollowView>> = HashMap::new();
        for follow in social.log {
            followed.entry(follow.day).or_default().push(FollowView {
                username: follow.username,
                display: follow.display,
            });
        }
        let studied: HashMap<u32, &crate::profile::Day> =
            history.days.iter().map(|day| (day.day, day)).collect();
        let mut dates: Vec<u32> = studied.keys().chain(followed.keys()).copied().collect();
        dates.sort_unstable_by(|a, b| b.cmp(a));
        dates.dedup();
        dates.truncate(FEED_DAYS);

        let floor = Counts::default().score();
        let days = dates
            .into_iter()
            .map(|date| {
                let day = studied.get(&date);
                // Where a day has no activity of its own, the score is
                // wherever the last active day left it: the same reading
                // `score_at` gives the graph, and the reason a follow-only day
                // shows a flat score rather than a fall to the floor.
                let score = day.map_or_else(
                    || history.score_at(date).unwrap_or(floor),
                    |day| day.counts.score(),
                );
                DayView {
                    t: day_start(date),
                    streak: day.map_or(0, |day| day.streak),
                    lessons: day.map_or(0, |day| day.activity.lessons),
                    exercises: day.map_or(0, |day| day.activity.exercises),
                    correct: day.map_or(0, |day| day.activity.correct),
                    xp: day.map_or(0, |day| day.activity.xp()),
                    learned: day.map_or_else(Vec::new, |day| day.activity.learned.clone()),
                    skills: day.map_or_else(Vec::new, |day| {
                        day.activity
                            .skills
                            .iter()
                            .map(|skill| {
                                skill
                                    .rsplit_once('\n')
                                    .map_or_else(|| skill.clone(), |(_, name)| name.to_string())
                            })
                            .collect()
                    }),
                    followed: followed.remove(&date).unwrap_or_default(),
                    score,
                    delta: score as i32
                        - history.score_at(date.saturating_sub(1)).unwrap_or(floor) as i32,
                }
            })
            .collect();
        Self {
            username,
            display: profile.display,
            title: profile.title,
            bio: profile.bio,
            cefr: profile.cefr,
            avatar: profile.avatar,
            course_id: profile.course_id,
            joined: profile.joined,
            followers: social.followers,
            following: social.following,
            viewer_follows: social.viewer_follows,
            online,
            last_active: history.last_active(),
            today: day_start(today),
            streak: history.streak(today),
            xp_schedule: XpScheduleView {
                lesson: XP_PER_LESSON,
                perfect_lesson: XP_PER_PERFECT_LESSON,
            },
            totals: TotalsView {
                xp: history.xp(),
                lessons: counts.lessons,
                exercises,
                correct,
                words: counts.words,
                skills: counts.skills,
                days: history.days.len() as u32,
            },
            languages,
            days,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::course::tests::{course_of, sentence, skill};
    use crate::exercise::Ask;
    use crate::lesson::{Target, Task};
    use crate::sentence::Wording;
    use crate::vocab::{Vocab, Word};
    use crate::word::WordState;

    #[test]
    fn daily_quests_publish_the_activity_days_utc_edges() {
        let value = serde_json::to_value(DailyQuestsView::of(DailyQuests::of(
            &crate::profile::Activity::default(),
            4,
        )))
        .unwrap();
        assert_eq!(value["day"], 4 * crate::profile::DAY);
        assert_eq!(value["resets_at"], 5 * crate::profile::DAY);
        assert_eq!(value["quests"].as_array().unwrap().len(), 3);
        assert_eq!(value["quests"][0]["id"], "earn_xp");
        assert_eq!(value["quests"][0]["done"], 0);
        assert_eq!(value["quests"][0]["total"], 40);
    }

    // The submit endpoint no longer has a pending lesson to consult. The
    // served response must therefore hand the client the exact stable target
    // and retrieval task it needs to return with the word verdicts.
    #[test]
    fn a_served_lesson_names_its_submission_target_ask_and_words() {
        let mut introduction = sentence("intro:1", &["hola"], 0);
        // A real loader-built one-word sentence has no answer grading marks:
        // the whole answer owns the verdict. Keep this fixture honest so the
        // assertion below proves the new-word location is not borrowed from
        // grading metadata.
        introduction.source = Wording::new(
            Phrasing {
                text: "en_hola".to_string(),
                words: Vec::new(),
            },
            Vec::new(),
        );
        let course = course_of(vec![skill("intro", 0, 0, &["hola"], 1)], vec![introduction]);
        let dictionary = Dictionary::from_entries(Vec::new());
        let lesson = Lesson {
            username: "sam".to_string(),
            target: Target::Skill(Position::new("intro", 1)),
            tasks: vec![Task::Exercise {
                sentence: 0,
                ask: Ask::BuildSource,
                introduces: vec!["hola".to_string()],
            }],
        };

        let value = serde_json::to_value(
            LessonView::of("unused".to_string(), &course, &dictionary, &lesson).unwrap(),
        )
        .unwrap();
        assert_eq!(
            value.get("target"),
            Some(&serde_json::json!({
                "kind": "skill",
                "skill": "intro",
                "lesson": 1
            }))
        );
        assert_eq!(
            value.pointer("/tasks/0/task/ask"),
            Some(&serde_json::json!("build_source"))
        );
        assert_eq!(
            value.pointer("/tasks/0/task/words"),
            Some(&serde_json::json!(["hola"]))
        );
        assert_eq!(
            value.pointer("/tasks/0/task/new_words"),
            Some(&serde_json::json!([{
                "word": "hola",
                "start": 3,
                "end": 7
            }]))
        );
        // This sentence tests one word, so its answer still has no grading
        // span: the new-word mark is a separate presentation contract.
        assert_eq!(
            value.pointer("/tasks/0/task/answers/0/words"),
            Some(&serde_json::json!([]))
        );
    }

    // Following somebody is something the user did on a day, so it belongs in
    // the feed, but it is not studying, so it must not arrive as a day of
    // activity. A follow-only day is therefore a real entry made of zeroes,
    // with the score sitting exactly where the last lesson left it.
    #[test]
    fn the_feed_carries_follows_on_days_with_no_study_in_them() {
        let course = course_of(
            vec![skill("intro", 0, 0, &["hola"], 1)],
            vec![sentence("intro:1", &["hola"], 0)],
        );
        let history = History::of(vec![(
            10,
            crate::profile::Activity {
                lessons: 2,
                learned: vec!["hola".to_string()],
                ..crate::profile::Activity::default()
            },
        )]);
        let social = Social {
            followers: 3,
            following: 1,
            viewer_follows: true,
            log: vec![Follow {
                day: 12,
                username: "ren".into(),
                display: "Ren".into(),
            }],
        };

        let value = serde_json::to_value(ProfileView::of(
            "sam".to_string(),
            Profile::new("sam", day_start(10)),
            &history,
            social,
            &[(&course, &history)],
            true,
            12,
        ))
        .unwrap();

        assert_eq!(value["followers"], 3);
        assert_eq!(value["viewer_follows"], true);
        assert_eq!(value["online"], true);
        // newest first, and the day nothing was studied is still a day
        assert_eq!(value["days"][0]["t"], day_start(12));
        assert_eq!(value["days"][0]["lessons"], 0);
        assert_eq!(value["days"][0]["streak"], 0);
        assert_eq!(value["days"][0]["followed"][0]["username"], "ren");
        assert_eq!(value["days"][0]["followed"][0]["display"], "Ren");
        // the score doesn't move on its own, so a day off is flat, not a fall
        assert_eq!(value["days"][0]["score"], value["days"][1]["score"]);
        assert_eq!(value["days"][0]["delta"], 0);
        assert_eq!(value["days"][1]["lessons"], 2);
        assert_eq!(value["days"][1]["followed"].as_array().unwrap().len(), 0);
        // and a follow earns nothing: it is not a lesson
        assert_eq!(value["totals"]["days"], 1);
        assert_eq!(value["streak"], 0);
    }

    // A follow on a day that was also studied joins that day rather than
    // splitting it in two.
    #[test]
    fn a_follow_on_a_studied_day_lands_on_the_day_it_happened() {
        let course = course_of(
            vec![skill("intro", 0, 0, &["hola"], 1)],
            vec![sentence("intro:1", &["hola"], 0)],
        );
        let history = History::of(vec![(
            10,
            crate::profile::Activity {
                lessons: 1,
                ..crate::profile::Activity::default()
            },
        )]);
        let view = ProfileView::of(
            "sam".to_string(),
            Profile::new("sam", day_start(10)),
            &history,
            Social {
                followers: 0,
                following: 1,
                viewer_follows: false,
                log: vec![Follow {
                    day: 10,
                    username: "ren".into(),
                    display: "Ren".into(),
                }],
            },
            &[(&course, &history)],
            false,
            10,
        );

        assert_eq!(view.days.len(), 1);
        assert_eq!(view.days[0].lessons, 1);
        assert_eq!(view.days[0].followed.len(), 1);
    }

    #[test]
    fn profile_language_scores_are_derived_per_course() {
        let mut spanish = course_of(
            vec![skill("intro", 0, 0, &["hola"], 1)],
            vec![sentence("intro:1", &["hola"], 0)],
        );
        spanish.id = "spanish_for_english".into();
        let mut french = course_of(
            vec![skill("intro", 0, 0, &["bonjour"], 1)],
            vec![sentence("intro:1", &["bonjour"], 0)],
        );
        french.id = "french_for_english".into();
        french.target_lang = "fr".into();
        let spanish_history = History::of(vec![(
            10,
            crate::profile::Activity {
                lessons: 2,
                learned: vec!["hola".into()],
                ..crate::profile::Activity::default()
            },
        )]);
        let french_history = History::of(vec![(
            11,
            crate::profile::Activity {
                lessons: 5,
                learned: vec!["bonjour".into(), "merci".into()],
                ..crate::profile::Activity::default()
            },
        )]);
        let overall = History::of(vec![
            (10, spanish_history.days[0].activity.clone()),
            (11, french_history.days[0].activity.clone()),
        ]);

        let view = ProfileView::of(
            "sam".into(),
            Profile::new("sam", day_start(10)),
            &overall,
            Social {
                followers: 0,
                following: 0,
                viewer_follows: false,
                log: Vec::new(),
            },
            &[(&spanish, &spanish_history), (&french, &french_history)],
            false,
            11,
        );

        assert_eq!(view.languages.len(), 2);
        assert_eq!(view.languages[0].id, "spanish_for_english");
        assert_eq!(view.languages[0].lessons, 2);
        assert_eq!(view.languages[1].id, "french_for_english");
        assert_eq!(view.languages[1].lessons, 5);
        assert_eq!(view.totals.lessons, 7);
    }

    #[test]
    fn a_flashcard_deck_contains_only_words_the_learner_has_encountered() {
        let course = course_of(
            vec![skill("intro", 0, 0, &["hola", "adios"], 1)],
            vec![
                sentence("intro:1", &["hola"], 0),
                sentence("intro:2", &["adios"], 0),
            ],
        );
        let mut user = User::new();
        user.words
            .insert("hola".to_string(), WordState::scaffolded(1));

        let value = serde_json::to_value(FlashcardDeckView::of(&course, &user, 2)).unwrap();
        assert_eq!(value["cards"].as_array().unwrap().len(), 1);
        assert_eq!(value["cards"][0]["word"], "hola");
        assert_eq!(value["cards"][0]["direction"], "target_to_source");
        assert_eq!(value["cards"][0]["language_direction"], "es->en");
        assert_eq!(value["cards"][0]["example"], "es_hola");
    }

    // A card is one question with one answer. The vocabulary carries up to
    // three meanings per word for grading, where any of them is a correct
    // answer; a flashcard cannot use them that way, because the learner is
    // grading themselves against what is written.
    #[test]
    fn a_flashcard_shows_one_meaning_and_not_the_whole_gloss_list() {
        let mut course = course_of(
            vec![skill("intro", 0, 0, &["hola"], 1)],
            vec![sentence("intro:1", &["hola"], 0)],
        );
        course.vocab = Vocab::new(vec![Word {
            id: "hola".to_string(),
            word: "hola".to_string(),
            forms: vec!["hola".to_string()],
            glosses: vec!["hello".to_string(), "hi".to_string(), "hey".to_string()],
        }]);
        let mut user = User::new();
        user.words
            .insert("hola".to_string(), WordState::scaffolded(1));

        let value = serde_json::to_value(FlashcardDeckView::of(&course, &user, 2)).unwrap();
        assert_eq!(value["cards"][0]["front"], "hola");
        assert_eq!(value["cards"][0]["back"], "hello");
    }

    // The gloss is the whole answer, so a word the glossary never defined
    // would leave one side of the card blank. Drop it instead.
    #[test]
    fn a_word_with_no_meaning_is_not_a_flashcard() {
        let mut course = course_of(
            vec![skill("intro", 0, 0, &["hola"], 1)],
            vec![sentence("intro:1", &["hola"], 0)],
        );
        course.vocab = Vocab::new(vec![Word {
            id: "hola".to_string(),
            word: "hola".to_string(),
            forms: vec!["hola".to_string()],
            glosses: Vec::new(),
        }]);
        let mut user = User::new();
        user.words
            .insert("hola".to_string(), WordState::scaffolded(1));

        let value = serde_json::to_value(FlashcardDeckView::of(&course, &user, 2)).unwrap();
        assert!(value["cards"].as_array().unwrap().is_empty());
    }
}
