use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, SystemTime};

use axum::{
    Extension, Json, Router,
    extract::{Path, Query, Request, State, ws::WebSocketUpgrade},
    http::{HeaderName, HeaderValue, Method, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post, put},
};
use rand::RngExt;
use tower_http::cors::{AllowOrigin, CorsLayer};

use crate::api::{
    CourseSummaryView, CourseView, CreateLessonRequest, DailyQuestsView, EditProfileRequest,
    ErrorBody, FlashcardDeckView, LeaderboardView, LessonView, ProfileView, SetCourseRequest,
    SetEmailRequest, SetPasswordRequest, Social, SubmitFlashcardsRequest, SubmitFlashcardsView,
    SubmitRequest, SubmitView, TipsView, UserSearchView, UserView,
};
use crate::auth::{
    AuthUser, ChangeEmailRequest, ChangePasswordRequest, CredentialError, CredentialService,
    LoginRequest, RegisterRequest,
};
use crate::course::Course;
use crate::dictionary::Dictionary;
use crate::leaderboard::Leaderboard;
use crate::lesson::Lesson;
use crate::messages::{self, Broker};
use crate::position::Position;
use crate::profile::{Activity, History, Profile, day_of, day_start};
use crate::quests::DailyQuests;
use crate::store::{Created, SessionIdentity, Store};
use crate::user::{FlashcardSubmissionError, SubmissionError, User};

pub struct AppState {
    courses: RwLock<Arc<HashMap<String, CourseContent>>>,
    store: Store,
    credentials: CredentialService,
    // Who is looking at their inbox right now, so that a message can reach
    // them while they are. Nothing is kept here — the database is the record
    // (see messages.rs).
    broker: Broker,
    // The last authenticated request seen from each recently present user.
    // Presence is deliberately process-local: it describes who this server
    // can see right now, not an account fact worth putting in SQLite.
    presence: RwLock<HashMap<String, u64>>,
    secure_cookies: bool,
    // The one origin a browser may reach this server from. CORS is built from
    // it, and so is the check on the inbox socket's handshake — which CORS
    // does not cover, so the two would otherwise disagree about who we serve.
    frontend_origin: String,
}

#[derive(Clone)]
struct CourseContent {
    course: Arc<Course>,
    dictionary: Arc<Dictionary>,
}

impl AppState {
    pub fn new(
        courses: HashMap<String, (Course, Dictionary)>,
        store: Store,
        credentials: CredentialService,
        secure_cookies: bool,
        frontend_origin: String,
    ) -> Arc<Self> {
        Arc::new(Self {
            courses: RwLock::new(Arc::new(Self::wrap_courses(courses))),
            store,
            credentials,
            broker: Broker::new(),
            presence: RwLock::new(HashMap::new()),
            secure_cookies,
            frontend_origin,
        })
    }

    fn wrap_courses(
        courses: HashMap<String, (Course, Dictionary)>,
    ) -> HashMap<String, CourseContent> {
        courses
            .into_iter()
            .map(|(id, (course, dictionary))| {
                (
                    id,
                    CourseContent {
                        course: Arc::new(course),
                        dictionary: Arc::new(dictionary),
                    },
                )
            })
            .collect()
    }

    // Swap one coherent wiki generation at once. A request clones the whole
    // catalog snapshot before selecting from it, so its course and dictionary
    // can never come from different rebuilds.
    pub fn replace_content(&self, courses: HashMap<String, (Course, Dictionary)>) {
        *self.courses.write().unwrap() = Arc::new(Self::wrap_courses(courses));
    }

    fn catalog(&self) -> Arc<HashMap<String, CourseContent>> {
        self.courses.read().unwrap().clone()
    }

    fn active_content(&self, username: &str) -> Result<(String, CourseContent), ApiError> {
        let course_id = self
            .store
            .load_profile(username)
            .map_err(db_error)?
            .and_then(|profile| profile.course_id)
            .ok_or_else(|| error(StatusCode::CONFLICT, "choose a course first"))?;
        let content = self.catalog().get(&course_id).cloned().ok_or_else(|| {
            error(
                StatusCode::CONFLICT,
                format!("the selected course '{course_id}' is not available"),
            )
        })?;
        Ok((course_id, content))
    }

    fn user(&self, username: &str, course_id: &str) -> Result<User, ApiError> {
        self.store
            .load_user(username, course_id)
            .map_err(db_error)?
            .ok_or_else(|| error(StatusCode::NOT_FOUND, format!("no user named '{username}'")))
    }

    fn account(&self, username: &str) -> Result<(), ApiError> {
        self.store
            .account_is_guest(username)
            .map_err(db_error)?
            .map(|_| ())
            .ok_or_else(|| error(StatusCode::NOT_FOUND, format!("no user named '{username}'")))
    }

    fn mark_online(&self, username: &str, timestamp: u64) {
        let mut presence = self.presence.write().unwrap();
        // Expired entries answer exactly the same as absent ones. Dropping
        // them while the map is already write-locked keeps this short-lived
        // index bounded by recent traffic rather than by every account that
        // has ever authenticated since the process started.
        presence.retain(|_, seen| timestamp.saturating_sub(*seen) <= ONLINE_SECONDS);
        presence.insert(username.to_string(), timestamp);
    }

    fn is_online(&self, username: &str, timestamp: u64) -> bool {
        self.presence
            .read()
            .unwrap()
            .get(username)
            .is_some_and(|seen| timestamp.saturating_sub(*seen) <= ONLINE_SECONDS)
    }
}

pub fn router(state: Arc<AppState>) -> Router {
    let private = Router::new()
        .route("/auth/me", get(get_session))
        .route("/me/keepalive", post(keep_alive))
        .route("/me", get(get_user))
        .route("/me/course", get(get_user_course).put(set_user_course))
        .route("/me/quests", get(get_daily_quests))
        .route("/me/flashcards", get(get_flashcards))
        .route("/me/flashcards/submit", post(submit_flashcards))
        .route("/me/profile", put(edit_user_profile))
        .route("/me/password", put(set_user_password))
        .route("/me/email", put(set_user_email))
        // Under `/users/…` because it names somebody else, but private
        // because it is something *you* do: the session says who is
        // following, so a path can never nominate a follower.
        .route(
            "/users/{username}/follow",
            put(follow_user).delete(unfollow_user),
        )
        .route("/me/lessons", post(create_lesson))
        .route("/me/lessons/{skill}/{lesson}/tips", get(get_lesson_tips))
        .route("/me/castles", post(create_castle))
        .route("/me/lessons/submit", post(submit_lesson))
        // The inbox: a websocket, and the whole of messaging's API (see
        // messages.rs). Under `/me` because an inbox is nobody else's.
        .route("/me/inbox", get(inbox_socket))
        // Who you might write to. Private, unlike a profile or the board:
        // reading somebody's page needs no account, but asking the server to
        // list people is a question only somebody using the inbox has.
        .route("/users", get(search_users))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            require_session,
        ));

    let frontend_origin = state
        .frontend_origin
        .parse::<HeaderValue>()
        .expect("MIMI_FRONTEND_ORIGIN must be a valid origin");
    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::exact(frontend_origin))
        .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE])
        .allow_headers([header::CONTENT_TYPE])
        .allow_credentials(true);

    Router::new()
        .route("/courses", get(get_courses))
        .route("/auth/register", post(register))
        .route("/auth/login", post(login))
        .route("/auth/guest", post(start_guest))
        .route("/auth/logout", post(logout))
        .route("/users/{username}/profile", get(get_user_profile))
        .route("/leaderboard", get(get_leaderboard))
        .merge(private)
        .layer(cors)
        .with_state(state)
}

const SESSION_COOKIE: &str = "mimi_session";
const SESSION_SECONDS: u64 = 30 * 24 * 60 * 60;
// A profile is online if and only if this process authenticated a request
// from it during this rolling window. Tabs ping halfway through the window,
// leaving room for an ordinary delayed timer without stretching the meaning
// of the indicator itself.
const ONLINE_SECONDS: u64 = 30;

// A guest's session is shorter than an account's because it is the *only*
// copy of them: there are no credentials to sign back in with, so when the
// cookie goes the record goes (see `Store::create_guest`). A week is long
// enough to come back tomorrow, and after the weekend, and short enough that
// records nobody ever claimed don't pile up forever.
const GUEST_SESSION_SECONDS: u64 = 7 * 24 * 60 * 60;

async fn require_session(
    State(state): State<Arc<AppState>>,
    mut request: Request,
    next: Next,
) -> Response {
    let timestamp = now();
    let Some(token) = session_cookie(request.headers()) else {
        return error(StatusCode::UNAUTHORIZED, "authentication required").into_response();
    };
    match state.store.load_session(token, timestamp) {
        Ok(Some(identity)) => {
            state.mark_online(&identity.username, timestamp);
            request.extensions_mut().insert(identity);
            next.run(request).await
        }
        Ok(None) => error(StatusCode::UNAUTHORIZED, "authentication required").into_response(),
        Err(error_value) => db_error(error_value).into_response(),
    }
}

fn session_cookie(headers: &axum::http::HeaderMap) -> Option<&str> {
    headers
        .get(header::COOKIE)?
        .to_str()
        .ok()?
        .split(';')
        .filter_map(|pair| pair.trim().split_once('='))
        .find_map(|(name, value)| (name == SESSION_COOKIE && !value.is_empty()).then_some(value))
}

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_secs()
}
type ApiError = (StatusCode, Json<ErrorBody>);
fn error(status: StatusCode, message: impl Into<String>) -> ApiError {
    (
        status,
        Json(ErrorBody {
            error: message.into(),
        }),
    )
}
fn db_error(e: rusqlite::Error) -> ApiError {
    eprintln!("database error: {e}");
    error(StatusCode::INTERNAL_SERVER_ERROR, "database error")
}

// Registering is also how a guest stops being one. The record they built
// while trying the course is renamed onto the name mimi_auth has just minted
// for them — see `Store::claim_guest` — so "save your progress" keeps their
// words, their place in the tree and every day of their streak.
async fn register(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Json(request): Json<RegisterRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let guest = guest_of(&state, &headers);
    let user = state
        .credentials
        .register(&request)
        .await
        .map_err(credential_error)?;
    match guest {
        Some(guest) => state
            .store
            .claim_guest(&guest, &user.username, now())
            .map_err(db_error)?,
        None => provision(&state, &user)?,
    }
    session_response(&state, StatusCode::CREATED, user)
}

async fn login(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Json(request): Json<LoginRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let guest = guest_of(&state, &headers);
    let user = state
        .credentials
        .login(&request)
        .await
        .map_err(credential_error)?;
    provision(&state, &user)?;
    // Signing in says the record that matters is the one behind these
    // credentials, so the guest record is discarded rather than merged.
    // Folding two learners together has no honest answer — of two stages for
    // the same word, neither is more true — and avoiding this is exactly what
    // the offer to register after each lesson was for.
    if let Some(guest) = guest {
        state.store.delete_account(&guest).map_err(db_error)?;
    }
    session_response(&state, StatusCode::OK, user)
}

// Start the course without an account: open a credential-less learning
// record and hand back a session for it. Everything downstream — the course
// map, the lesson builder, the profile — reads a guest as an ordinary
// learner, because a lesson is generated from the learner's own memory state
// and there is nowhere but the database for that state to live.
async fn start_guest(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> Result<Response, ApiError> {
    // Already somebody. Minting a second guest here would overwrite the
    // cookie that is the only way back to the first one's progress, so a
    // double-click — or a page that isn't sure whether it has a session —
    // gets an answer rather than a new identity.
    if let Some(identity) = current_session(&state, &headers)? {
        return Ok((
            [(header::CACHE_CONTROL, "no-store")],
            Json(ViewerView::from(identity)),
        )
            .into_response());
    }
    let timestamp = now();
    let (username, token) = state
        .store
        .create_guest(timestamp + GUEST_SESSION_SECONDS, timestamp)
        .map_err(db_error)?;
    Ok((
        StatusCode::CREATED,
        [
            (
                header::SET_COOKIE,
                session_cookie_header(&state, &token, GUEST_SESSION_SECONDS),
            ),
            (header::CACHE_CONTROL, "no-store".to_string()),
        ],
        Json(ViewerView {
            username,
            email: None,
            guest: true,
        }),
    )
        .into_response())
}

// The session this request carries, if it carries a live one.
fn current_session(
    state: &AppState,
    headers: &axum::http::HeaderMap,
) -> Result<Option<SessionIdentity>, ApiError> {
    let identity = match session_cookie(headers) {
        Some(token) => state.store.load_session(token, now()).map_err(db_error),
        None => Ok(None),
    }?;
    if let Some(identity) = &identity {
        state.mark_online(&identity.username, now());
    }
    Ok(identity)
}

// The guest whose record this request is carrying, if any — read *before*
// the credential service is called, because that call is what decides
// whether there is a name to carry the record onto. A database that won't
// answer reads as "no guest": the sign-in still works, and the worst it
// costs is a record that the next sweep collects.
fn guest_of(state: &AppState, headers: &axum::http::HeaderMap) -> Option<String> {
    let identity = current_session(state, headers).ok()??;
    identity.guest.then_some(identity.username)
}

fn provision(state: &AppState, user: &AuthUser) -> Result<(), ApiError> {
    match state
        .store
        .create_user(&user.username, now())
        .map_err(db_error)?
    {
        Created::Ok | Created::Taken => Ok(()),
    }
}

fn session_response(
    state: &AppState,
    status: StatusCode,
    user: AuthUser,
) -> Result<(StatusCode, [(HeaderName, String); 2], Json<ViewerView>), ApiError> {
    let timestamp = now();
    let token = state
        .store
        .create_session(
            &user.username,
            &user.email,
            timestamp + SESSION_SECONDS,
            timestamp,
        )
        .map_err(db_error)?;
    Ok((
        status,
        [
            (
                header::SET_COOKIE,
                session_cookie_header(state, &token, SESSION_SECONDS),
            ),
            (header::CACHE_CONTROL, "no-store".to_string()),
        ],
        Json(ViewerView::from(user)),
    ))
}

fn session_cookie_header(state: &AppState, token: &str, max_age: u64) -> String {
    format!(
        "{SESSION_COOKIE}={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age={max_age}{}",
        if state.secure_cookies { "; Secure" } else { "" }
    )
}

async fn get_session(Extension(identity): Extension<SessionIdentity>) -> impl IntoResponse {
    (
        [(header::CACHE_CONTROL, "no-store")],
        Json(ViewerView::from(identity)),
    )
}

// Idle tabs use the ordinary session middleware to record their presence;
// the endpoint itself has nothing to store and nothing to say.
async fn keep_alive() -> StatusCode {
    StatusCode::NO_CONTENT
}

// Who the caller is, in the one shape every `/auth` endpoint answers with —
// so a client has a single question to ask and a single answer to read.
#[derive(serde::Serialize)]
struct ViewerView {
    username: String,
    // null for a guest: no credentials behind them means no address either
    email: Option<String>,
    // whether this account is still unsaved, and so whether the client should
    // offer to save it
    guest: bool,
}

impl From<SessionIdentity> for ViewerView {
    fn from(value: SessionIdentity) -> Self {
        Self {
            username: value.username,
            email: value.email,
            guest: value.guest,
        }
    }
}

impl From<AuthUser> for ViewerView {
    fn from(value: AuthUser) -> Self {
        Self {
            username: value.username,
            email: Some(value.email),
            guest: false,
        }
    }
}

async fn logout(
    State(state): State<Arc<AppState>>,
    request: Request,
) -> Result<impl IntoResponse, ApiError> {
    // Signing a guest out is how they say they are finished with a record
    // nothing can sign back into, so it goes rather than lingering until the
    // sweep. Everyone else keeps theirs, which is the point of having one.
    if let Some(guest) = guest_of(&state, request.headers()) {
        state.store.delete_account(&guest).map_err(db_error)?;
    } else if let Some(token) = session_cookie(request.headers()) {
        state.store.delete_session(token).map_err(db_error)?;
    }
    let cookie = format!(
        "{SESSION_COOKIE}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0{}",
        if state.secure_cookies { "; Secure" } else { "" }
    );
    Ok((
        StatusCode::NO_CONTENT,
        [
            (header::SET_COOKIE, cookie),
            (header::CACHE_CONTROL, "no-store".to_string()),
        ],
    ))
}

fn credential_error(value: CredentialError) -> ApiError {
    match value {
        CredentialError::Rejected(status, message) => error(status, message),
        CredentialError::Unavailable => error(
            StatusCode::SERVICE_UNAVAILABLE,
            "authentication service unavailable",
        ),
    }
}

async fn get_courses(State(state): State<Arc<AppState>>) -> Json<Vec<CourseSummaryView>> {
    let mut courses: Vec<_> = state
        .catalog()
        .values()
        .map(|content| CourseSummaryView::of(&content.course))
        .collect();
    courses.sort_by(|a, b| a.id.cmp(&b.id));
    Json(courses)
}

async fn get_user(
    State(state): State<Arc<AppState>>,
    Extension(identity): Extension<SessionIdentity>,
) -> Result<Json<UserView>, ApiError> {
    let username = identity.username;
    state.account(&username)?;
    let course_id = state
        .store
        .load_profile(&username)
        .map_err(db_error)?
        .and_then(|profile| profile.course_id);
    let user = match course_id {
        Some(course_id) => state.user(&username, &course_id)?,
        None => User::new(),
    };
    Ok(Json(UserView::of(username, &user, now())))
}

async fn get_user_course(
    State(state): State<Arc<AppState>>,
    Extension(identity): Extension<SessionIdentity>,
) -> Result<Json<CourseView>, ApiError> {
    let username = identity.username;
    let (course_id, content) = state.active_content(&username)?;
    let user = state.user(&username, &course_id)?;
    Ok(Json(CourseView::of(&content.course, &user)))
}

async fn get_daily_quests(
    State(state): State<Arc<AppState>>,
    Extension(identity): Extension<SessionIdentity>,
) -> Result<Json<DailyQuestsView>, ApiError> {
    let username = identity.username;
    // Keep a stale session from turning a deleted account into a convincing
    // board of zeroes. Other private reads make the same existence check.
    let (course_id, _) = state.active_content(&username)?;
    state.user(&username, &course_id)?;
    let day = day_of(now());
    let activity = state
        .store
        .load_activity_on(&username, &course_id, day)
        .map_err(db_error)?
        .unwrap_or_default();
    Ok(Json(DailyQuestsView::of(DailyQuests::of(&activity, day))))
}

async fn get_flashcards(
    State(state): State<Arc<AppState>>,
    Extension(identity): Extension<SessionIdentity>,
) -> Result<Json<FlashcardDeckView>, ApiError> {
    let (course_id, content) = state.active_content(&identity.username)?;
    let user = state.user(&identity.username, &course_id)?;
    Ok(Json(FlashcardDeckView::of(&content.course, &user, now())))
}

// A profile is public, so the session is read rather than required: a
// signed-out reader gets the same page, with `viewer_follows` false. The one
// thing the identity buys is the state of the Follow button, which is why a
// request without one is not an error.
async fn get_user_profile(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Path(username): Path<String>,
) -> Result<Json<ProfileView>, ApiError> {
    let timestamp = now();
    state.account(&username)?;
    let rows = state.store.load_activity(&username).map_err(db_error)?;
    let mut by_day: HashMap<u32, Activity> = HashMap::new();
    let mut by_course: HashMap<String, Vec<(u32, Activity)>> = HashMap::new();
    let catalog = state.catalog();
    for (course_id, day, activity) in &rows {
        let mut display_activity = activity.clone();
        if let Some(content) = catalog.get(course_id) {
            display_activity.learned = display_activity
                .learned
                .iter()
                .map(|word| content.course.vocab.word_for(word))
                .collect();
        }
        // History deduplicates a cleared skill by its stored key. Prefix the
        // display name only in this cross-course projection so two unrelated
        // courses may both have a "Basics" without losing a profile count;
        // ProfileView removes the prefix again for the feed.
        display_activity.skills = display_activity
            .skills
            .iter()
            .map(|skill| format!("{course_id}\n{skill}"))
            .collect();
        by_day.entry(*day).or_default().absorb(display_activity);
        by_course
            .entry(course_id.clone())
            .or_default()
            .push((*day, activity.clone()));
    }
    let history = History::of(by_day.into_iter().collect());
    let profile = state
        .store
        .load_profile(&username)
        .map_err(db_error)?
        .unwrap_or_else(|| {
            Profile::new(
                &username,
                history.days.first().map_or(timestamp, |d| day_start(d.day)),
            )
        });
    if let Some(course_id) = &profile.course_id {
        by_course.entry(course_id.clone()).or_default();
    }
    let mut course_histories: Vec<_> = by_course
        .into_iter()
        .filter_map(|(id, rows)| {
            catalog
                .get(&id)
                .map(|content| (content.course.clone(), History::of(rows)))
        })
        .collect();
    course_histories.sort_by(|a, b| a.0.id.cmp(&b.0.id));
    let course_history_refs: Vec<_> = course_histories
        .iter()
        .map(|(course, history)| (course.as_ref(), history))
        .collect();
    let (followers, following) = state.store.follow_counts(&username).map_err(db_error)?;
    let viewer_follows = match current_session(&state, &headers)? {
        Some(identity) => state
            .store
            .follows(&identity.username, &username)
            .map_err(db_error)?,
        None => false,
    };
    let social = Social {
        followers,
        following,
        viewer_follows,
        log: state.store.follow_log(&username).map_err(db_error)?,
    };
    let online = state.is_online(&username, timestamp);
    Ok(Json(ProfileView::of(
        username,
        profile,
        &history,
        social,
        &course_history_refs,
        online,
        day_of(timestamp),
    )))
}

// The weekly board. Public and session-free, like a profile: a board is a
// thing you can read over somebody's shoulder, and nothing here depends on
// who is asking. Which row is "you" is a comparison against a username the
// client already holds, so asking for it would buy nothing.
//
// The whole board is served. There is no page and no cut-off: with a young
// user base the honest answer is the short one, and inventing a
// limit now would fix a number nobody has measured. When the response gets
// big enough to notice, the shape to reach for is a top slice plus the
// caller's own row — which needs the session this handler currently doesn't.
async fn get_leaderboard(
    State(state): State<Arc<AppState>>,
) -> Result<Json<LeaderboardView>, ApiError> {
    let week_start = crate::leaderboard::week_start(day_of(now()));
    let rows = state
        .store
        .load_activity_since(week_start)
        .map_err(db_error)?;
    Ok(Json(LeaderboardView::of(&Leaderboard::of(
        rows, week_start,
    ))))
}

// The one writable piece of a profile. The session middleware binds it to
// the owner; a path parameter can no longer select somebody else's account.
async fn set_user_course(
    State(state): State<Arc<AppState>>,
    Extension(identity): Extension<SessionIdentity>,
    Json(request): Json<SetCourseRequest>,
) -> Result<StatusCode, ApiError> {
    let username = identity.username;
    let course_id = request.course_id.trim();
    if !state.catalog().contains_key(course_id) {
        return Err(error(
            StatusCode::NOT_FOUND,
            format!("no available course with id '{course_id}'"),
        ));
    }
    state.account(&username)?;
    state
        .store
        .save_course(&username, course_id, now())
        .map_err(db_error)?;
    Ok(StatusCode::NO_CONTENT)
}

// The authored half of a profile, written whole (see `EditProfileRequest`).
// Bound to the owner by the session like every other writer here: there is no
// path parameter, so no request can nominate somebody else's profile to
// rewrite. A guest may edit theirs — the record is real while it lasts, and
// "Guest" is a placeholder rather than a name they chose.
async fn edit_user_profile(
    State(state): State<Arc<AppState>>,
    Extension(identity): Extension<SessionIdentity>,
    Json(request): Json<EditProfileRequest>,
) -> Result<StatusCode, ApiError> {
    let username = identity.username;
    let edit = request
        .into_edit()
        .map_err(|message| error(StatusCode::BAD_REQUEST, message))?;
    state.account(&username)?;
    state
        .store
        .save_profile_edit(&username, edit, now())
        .map_err(db_error)?;
    Ok(StatusCode::NO_CONTENT)
}

// Follow somebody, dated the day it happened — which is what puts it in the
// follower's activity feed. Idempotent: following twice is one follow, and
// the second call does not re-date the first.
async fn follow_user(
    State(state): State<Arc<AppState>>,
    Extension(identity): Extension<SessionIdentity>,
    Path(username): Path<String>,
) -> Result<StatusCode, ApiError> {
    let follower = check_follow(&state, &identity, &username)?;
    state
        .store
        .follow(follower, &username, day_of(now()))
        .map_err(db_error)?;
    Ok(StatusCode::NO_CONTENT)
}

// Stop following somebody. This ends the follow and nothing else: the feed
// entry from the day it started stays where it is, because the feed is a
// record of what the user did and this is not a claim that they didn't.
// Idempotent in the same way — unfollowing somebody you never followed is
// already true, so it is a 204 rather than an argument.
async fn unfollow_user(
    State(state): State<Arc<AppState>>,
    Extension(identity): Extension<SessionIdentity>,
    Path(username): Path<String>,
) -> Result<StatusCode, ApiError> {
    let follower = check_follow(&state, &identity, &username)?;
    state
        .store
        .unfollow(follower, &username)
        .map_err(db_error)?;
    Ok(StatusCode::NO_CONTENT)
}

// Who may follow whom, checked identically for both directions of the button
// so that the answer can't depend on which way it is being pressed. Returns
// the follower's name on success.
fn check_follow<'a>(
    state: &AppState,
    identity: &'a SessionIdentity,
    followee: &str,
) -> Result<&'a str, ApiError> {
    // Following is public and permanent — it goes on the follower's page and
    // stays there — and a guest is neither. Their record has a week to live
    // and no name behind it, which is the same reason the weekly board leaves
    // them off; registering is what turns them into somebody who can do this.
    if identity.guest {
        return Err(error(
            StatusCode::FORBIDDEN,
            "save your progress before following people",
        ));
    }
    if identity.username == followee {
        return Err(error(StatusCode::BAD_REQUEST, "you cannot follow yourself"));
    }
    match state.store.account_is_guest(followee).map_err(db_error)? {
        None => Err(error(
            StatusCode::NOT_FOUND,
            format!("no user named '{followee}'"),
        )),
        // A guest's page is readable, but there is nobody there to follow:
        // the record vanishes with its cookie, taking the follow with it.
        Some(true) => Err(error(
            StatusCode::FORBIDDEN,
            "that account cannot be followed",
        )),
        Some(false) => Ok(&identity.username),
    }
}

// The inbox, which is one socket and no endpoints. Everything a client can
// say and everything it is told lives in messages.rs; this is the handshake
// in front of it, and it decides two things.
//
// **A guest has no inbox.** The same rule as following, for the same reason:
// a record with a week to live and no name behind it has nobody to write to,
// and nobody could write back. Registering keeps everything they have done,
// so the answer is an offer rather than a wall.
//
// **The origin is checked here by hand.** A websocket handshake is not a CORS
// request — the browser sends it with the session cookie and no preflight,
// and the CORS layer above never sees it — so any page anywhere could
// otherwise open a socket as whoever is signed in and read their mail. This
// is the check that isn't happening for us.
async fn inbox_socket(
    State(state): State<Arc<AppState>>,
    Extension(identity): Extension<SessionIdentity>,
    headers: axum::http::HeaderMap,
    upgrade: WebSocketUpgrade,
) -> Result<Response, ApiError> {
    if identity.guest {
        return Err(error(
            StatusCode::FORBIDDEN,
            "save your progress before using messages",
        ));
    }
    let origin = headers
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok());
    if origin != Some(state.frontend_origin.as_str()) {
        return Err(error(StatusCode::FORBIDDEN, "unrecognized origin"));
    }
    Ok(upgrade.on_upgrade(move |socket| async move {
        messages::serve(socket, identity.username, &state.store, &state.broker).await
    }))
}

// How many accounts a search may name at once. Enough to pick out of, short
// enough that the box is a list rather than a directory.
const SEARCH_LIMIT: u32 = 8;

// Accounts whose name starts with `q`, for the inbox's new-conversation box.
// An empty query matches nobody rather than everybody: a box nobody has typed
// in is not a request to enumerate the users of the site.
async fn search_users(
    State(state): State<Arc<AppState>>,
    Extension(identity): Extension<SessionIdentity>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<UserSearchView>, ApiError> {
    let prefix = query.q.unwrap_or_default();
    let prefix = prefix.trim();
    if prefix.is_empty() {
        return Ok(Json(UserSearchView::of(Vec::new())));
    }
    let found = state
        .store
        .search_accounts(prefix, SEARCH_LIMIT)
        .map_err(db_error)?
        .into_iter()
        // there is no conversation to start with yourself, so the box does
        // not offer one
        .filter(|(username, _)| username != &identity.username)
        .collect();
    Ok(Json(UserSearchView::of(found)))
}

#[derive(serde::Deserialize)]
struct SearchQuery {
    q: Option<String>,
}

// The account settings, which are the two things `mimi_auth` holds. Both are
// proxies with a session in front: the account being edited is the one the
// cookie names, never a field of the body, so a request can only ever reach
// its own credentials. The rules — length, the common-password list, what a
// valid address is — stay in `mimi_auth`, which is the one place they can be
// enforced for the wiki as well as for us; the message it refuses with is
// what the learner reads.
//
// **A username is not here**, deliberately: it addresses a profile, a
// leaderboard row and every link either of them appears in, and renaming an
// account is a migration rather than a setting.
async fn set_user_password(
    State(state): State<Arc<AppState>>,
    Extension(identity): Extension<SessionIdentity>,
    headers: axum::http::HeaderMap,
    Json(request): Json<SetPasswordRequest>,
) -> Result<StatusCode, ApiError> {
    let username = credentialed(&identity)?;
    state
        .credentials
        .change_password(&ChangePasswordRequest {
            login: username.clone(),
            current_password: request.current_password,
            new_password: request.new_password,
        })
        .await
        .map_err(credential_error)?;
    // The old password is out of use, so the sessions opened with it go too
    // — `mimi_auth` can retire a credential but cannot reach the cookies its
    // consumers hold. The browser that asked keeps its own (see
    // `Store::delete_other_sessions`); a request that somehow arrived
    // without a readable cookie can't have got past the middleware.
    if let Some(token) = session_cookie(&headers) {
        state
            .store
            .delete_other_sessions(&username, token)
            .map_err(db_error)?;
    }
    Ok(StatusCode::NO_CONTENT)
}

// Answers with the viewer shape rather than a 204, because the address it
// carries is the one the client was displaying a moment ago.
async fn set_user_email(
    State(state): State<Arc<AppState>>,
    Extension(identity): Extension<SessionIdentity>,
    Json(request): Json<SetEmailRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let username = credentialed(&identity)?;
    let user = state
        .credentials
        .change_email(&ChangeEmailRequest {
            login: username.clone(),
            password: request.password,
            new_email: request.email.trim().to_string(),
        })
        .await
        .map_err(credential_error)?;
    // Sessions carry a copy of the address for `/auth/me` to answer from, so
    // the copy follows the record — on this browser and on any other.
    state
        .store
        .update_session_email(&username, &user.email)
        .map_err(db_error)?;
    Ok((
        [(header::CACHE_CONTROL, "no-store")],
        Json(ViewerView::from(user)),
    ))
}

// A guest has no credentials to edit — `mimi_auth` has never heard of them
// (see AGENTS.md) — so there is nothing here to change and no current
// password they could prove themselves with. Registering is the way in, and
// it keeps the record they have already built.
fn credentialed(identity: &SessionIdentity) -> Result<String, ApiError> {
    if identity.guest {
        return Err(error(
            StatusCode::FORBIDDEN,
            "a guest has no account settings; create an account to keep your progress",
        ));
    }
    Ok(identity.username.clone())
}

async fn create_lesson(
    State(state): State<Arc<AppState>>,
    Extension(identity): Extension<SessionIdentity>,
    request: Option<Json<CreateLessonRequest>>,
) -> Result<(StatusCode, Json<LessonView>), ApiError> {
    let username = identity.username;
    let (course_id, content) = state.active_content(&username)?;
    let user = state.user(&username, &course_id)?;
    let course = content.course;
    let dictionary = content.dictionary;
    let position = match request {
        Some(Json(r)) => r.into(),
        None => user.next_lesson(&course).ok_or_else(|| {
            if user.castle_due(&course).is_some() {
                error(StatusCode::CONFLICT, "a castle test is due")
            } else {
                error(StatusCode::NOT_FOUND, "no lesson is currently available")
            }
        })?,
    };
    if !course.has_lesson(&position) {
        return Err(error(
            StatusCode::NOT_FOUND,
            format!("the course has no lesson at {position}"),
        ));
    }
    let lesson = Lesson::build(&course, &user, &username, &position).ok_or_else(|| {
        error(
            StatusCode::FORBIDDEN,
            format!("the lesson at {position} is still locked"),
        )
    })?;
    serve_lesson(&course, &dictionary, lesson)
}

// The tips a lesson carries, served on their own. The map's "Tips" button
// reads this so a learner can read the lesson's material without starting
// it — a GET, because nothing is built, stored or consumed here.
async fn get_lesson_tips(
    State(state): State<Arc<AppState>>,
    Extension(identity): Extension<SessionIdentity>,
    Path((skill, lesson)): Path<(String, u8)>,
) -> Result<Json<TipsView>, ApiError> {
    let username = identity.username;
    let (course_id, content) = state.active_content(&username)?;
    let user = state.user(&username, &course_id)?;
    let course = content.course;
    let position = Position::new(skill, lesson);
    if !course.has_lesson(&position) {
        return Err(error(
            StatusCode::NOT_FOUND,
            format!("the course has no lesson at {position}"),
        ));
    }
    if !user.may_take(&course, &position) {
        return Err(error(
            StatusCode::FORBIDDEN,
            format!("the lesson at {position} is still locked"),
        ));
    }
    let skill = course
        .skill(&position.skill)
        .expect("has_lesson guarantees the skill exists");
    Ok(Json(TipsView::of(skill, position.lesson)))
}

async fn create_castle(
    State(state): State<Arc<AppState>>,
    Extension(identity): Extension<SessionIdentity>,
) -> Result<(StatusCode, Json<LessonView>), ApiError> {
    let username = identity.username;
    let (course_id, content) = state.active_content(&username)?;
    let user = state.user(&username, &course_id)?;
    let course = content.course;
    let dictionary = content.dictionary;
    let castle = user.castle_due(&course).ok_or_else(|| {
        error(
            StatusCode::FORBIDDEN,
            "no castle test is currently available",
        )
    })?;
    let lesson = Lesson::castle(&course, &user, &username, castle).ok_or_else(|| {
        error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "no questions available for castle",
        )
    })?;
    serve_lesson(&course, &dictionary, lesson)
}

fn serve_lesson(
    course: &Course,
    dictionary: &Dictionary,
    lesson: Lesson,
) -> Result<(StatusCode, Json<LessonView>), ApiError> {
    if lesson.tasks.is_empty() {
        return Err(error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "no tasks available",
        ));
    }
    let id = format!("{:016x}", rand::rng().random::<u64>());
    let view = LessonView::of(id.clone(), course, dictionary, &lesson).ok_or_else(|| {
        error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "lesson refers to missing content",
        )
    })?;
    Ok((StatusCode::CREATED, Json(view)))
}

async fn submit_lesson(
    State(state): State<Arc<AppState>>,
    Extension(identity): Extension<SessionIdentity>,
    Json(request): Json<SubmitRequest>,
) -> Result<Json<SubmitView>, ApiError> {
    let username = identity.username;
    let submission = request.into_submission();
    let timestamp = now();
    let (course_id, content) = state.active_content(&username)?;
    let course = content.course;
    let applied = state
        .store
        .update_user(&username, &course_id, day_of(timestamp), |user| {
            let result = user.submit_lesson(&course, &submission, timestamp);
            let activity = result.as_ref().map_or_else(
                |_| Activity::default(),
                |outcome| Activity::of_lesson(outcome, outcome.cleared_skill.clone()),
            );
            (result, activity)
        })
        .map_err(db_error)?
        .ok_or_else(|| error(StatusCode::NOT_FOUND, format!("no user named '{username}'")))?
        .map_err(submission_error)?;
    Ok(Json(SubmitView {
        correct: applied.correct,
        total: applied.total,
        passed: applied.passed,
    }))
}

async fn submit_flashcards(
    State(state): State<Arc<AppState>>,
    Extension(identity): Extension<SessionIdentity>,
    Json(request): Json<SubmitFlashcardsRequest>,
) -> Result<Json<SubmitFlashcardsView>, ApiError> {
    let username = identity.username;
    let reports = request.into_reports();
    let timestamp = now();
    let (course_id, content) = state.active_content(&username)?;
    let course = content.course;
    let applied = state
        .store
        .update_user(&username, &course_id, day_of(timestamp), |user| {
            (
                user.submit_flashcards(&course, &reports, timestamp),
                Activity::default(),
            )
        })
        .map_err(db_error)?
        .ok_or_else(|| error(StatusCode::NOT_FOUND, format!("no user named '{username}'")))?
        .map_err(flashcard_submission_error)?;
    Ok(Json(SubmitFlashcardsView {
        correct: applied.correct,
        total: applied.total,
    }))
}

fn flashcard_submission_error(value: FlashcardSubmissionError) -> ApiError {
    error(StatusCode::BAD_REQUEST, value.to_string())
}

fn submission_error(value: SubmissionError) -> ApiError {
    let status = match value {
        SubmissionError::NoSuchLesson(_) | SubmissionError::NoSuchCastle(_) => {
            StatusCode::NOT_FOUND
        }
        SubmissionError::LessonLocked(_) | SubmissionError::CastleLocked(_) => {
            StatusCode::FORBIDDEN
        }
        _ => StatusCode::BAD_REQUEST,
    };
    error(status, value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderMap;

    #[test]
    fn the_session_cookie_is_selected_by_its_exact_name() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::COOKIE,
            HeaderValue::from_static("theme=paper; mimi_session=secret; other=1"),
        );
        assert_eq!(session_cookie(&headers), Some("secret"));

        headers.insert(
            header::COOKIE,
            HeaderValue::from_static("not_mimi_session=secret"),
        );
        assert_eq!(session_cookie(&headers), None);
    }

    #[test]
    fn presence_is_exactly_the_thirty_seconds_after_an_authenticated_request() {
        let state = test_state();
        assert!(!state.is_online("sam", 100));

        state.mark_online("sam", 100);
        assert!(state.is_online("sam", 100));
        assert!(state.is_online("sam", 130));
        assert!(!state.is_online("sam", 131));

        // A later request starts a fresh rolling window; no persisted record
        // or study activity participates in the answer.
        state.mark_online("sam", 200);
        assert!(state.is_online("sam", 230));
        assert!(!state.is_online("sam", 231));
    }

    // A guest never touches the credential service, so the whole path is local.
    #[tokio::test]
    async fn a_guest_gets_a_session_that_the_private_routes_accept() {
        let state = test_state();
        let response = start_guest(State(state.clone()), HeaderMap::new())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
        let set_cookie = response
            .headers()
            .get(header::SET_COOKIE)
            .unwrap()
            .to_str()
            .unwrap();
        assert!(set_cookie.contains("HttpOnly"));
        let mut headers = HeaderMap::new();
        headers.insert(
            header::COOKIE,
            set_cookie
                .split(';')
                .next()
                .unwrap()
                .parse::<HeaderValue>()
                .unwrap(),
        );

        // the session names a guest, and the learning record behind it is
        // one the ordinary endpoints can serve
        let identity = current_session(&state, &headers).unwrap().unwrap();
        assert!(identity.guest);
        assert!(identity.email.is_none());
        let _ = get_user(State(state.clone()), Extension(identity))
            .await
            .unwrap();

        // and asking again returns the same guest rather than stranding them
        // behind a cookie we just overwrote
        let again = start_guest(State(state.clone()), headers).await.unwrap();
        assert_eq!(again.status(), StatusCode::OK);
        assert!(again.headers().get(header::SET_COOKIE).is_none());
    }

    #[tokio::test]
    async fn daily_quests_read_the_authenticated_learners_activity_today() {
        let state = test_state();
        state.store.create_user("sam", now()).unwrap();
        state
            .store
            .save_course("sam", "spanish_for_english", now())
            .unwrap();
        let today = day_of(now());
        state
            .store
            .update_user("sam", "spanish_for_english", today, |_| {
                (
                    (),
                    Activity {
                        lessons: 2,
                        perfect_lessons: 1,
                        exercises: 18,
                        correct: 16,
                        learned: Vec::new(),
                        skills: Vec::new(),
                    },
                )
            })
            .unwrap();

        let Json(view) = get_daily_quests(
            State(state),
            Extension(SessionIdentity {
                username: "sam".into(),
                email: Some("sam@example.com".into()),
                guest: false,
            }),
        )
        .await
        .unwrap();

        assert_eq!(view.day, day_start(today));
        assert_eq!(view.resets_at, day_start(today + 1));
        for quest in view.quests {
            let expected = match quest.id {
                "earn_xp" => 60,
                "complete_lessons" => 2,
                "correct_answers" => 16,
                "perfect_lesson" => 1,
                id => panic!("unexpected quest {id}"),
            };
            assert_eq!(quest.done, expected);
        }
    }

    #[tokio::test]
    async fn course_selection_accepts_only_the_live_catalog() {
        let state = test_state();
        state.store.create_user("sam", now()).unwrap();
        let identity = SessionIdentity {
            username: "sam".into(),
            email: Some("sam@example.com".into()),
            guest: false,
        };

        let missing = set_user_course(
            State(state.clone()),
            Extension(identity.clone()),
            Json(SetCourseRequest {
                course_id: "french_for_english".into(),
            }),
        )
        .await
        .unwrap_err();
        assert_eq!(missing.0, StatusCode::NOT_FOUND);

        set_user_course(
            State(state.clone()),
            Extension(identity),
            Json(SetCourseRequest {
                course_id: "spanish_for_english".into(),
            }),
        )
        .await
        .unwrap();
        assert_eq!(
            state
                .store
                .load_profile("sam")
                .unwrap()
                .unwrap()
                .course_id
                .as_deref(),
            Some("spanish_for_english")
        );
    }

    // Who may follow whom, from the handler's side. The rules exist because a
    // follow is public and permanent — it goes on the follower's page and
    // stays there — so both ends of one have to be somebody who will still be
    // there tomorrow.
    #[tokio::test]
    async fn following_is_between_named_accounts_and_never_with_yourself() {
        let state = test_state();
        state.store.create_user("sam", now()).unwrap();
        state.store.create_user("ren", now()).unwrap();
        let sam = SessionIdentity {
            username: "sam".into(),
            email: Some("sam@example.com".into()),
            guest: false,
        };
        let follow = |identity: SessionIdentity, who: &str| {
            follow_user(
                State(state.clone()),
                Extension(identity),
                Path(who.to_string()),
            )
        };

        assert_eq!(
            follow(sam.clone(), "sam").await.unwrap_err().0,
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            follow(sam.clone(), "nobody").await.unwrap_err().0,
            StatusCode::NOT_FOUND
        );

        follow(sam.clone(), "ren").await.unwrap();
        assert!(state.store.follows("sam", "ren").unwrap());
        // pressing it twice is still one follow, and still one feed entry
        follow(sam.clone(), "ren").await.unwrap();
        assert_eq!(state.store.follow_log("sam").unwrap().len(), 1);

        // A guest is at neither end: their record has a week to live and no
        // name behind it, so a follow either way would evaporate with it.
        let (guest, _) = state.store.create_guest(now() + 100, now()).unwrap();
        assert_eq!(
            follow(sam.clone(), &guest).await.unwrap_err().0,
            StatusCode::FORBIDDEN
        );
        let as_guest = SessionIdentity {
            username: guest,
            email: None,
            guest: true,
        };
        assert_eq!(
            follow(as_guest, "ren").await.unwrap_err().0,
            StatusCode::FORBIDDEN
        );

        // and unfollowing ends the follow without unsaying it
        unfollow_user(
            State(state.clone()),
            Extension(sam),
            Path("ren".to_string()),
        )
        .await
        .unwrap();
        assert!(!state.store.follows("sam", "ren").unwrap());
        assert_eq!(state.store.follow_log("sam").unwrap().len(), 1);
    }

    // The profile a reader is served depends on who is reading it in exactly
    // one way: whether the Follow button is already pressed.
    #[tokio::test]
    async fn a_profile_reports_the_follow_state_of_whoever_is_reading_it() {
        let state = test_state();
        state.store.create_user("sam", now()).unwrap();
        state.store.create_user("ren", now()).unwrap();
        let token = state
            .store
            .create_session("sam", "sam@example.com", now() + 100, now())
            .unwrap();
        state.store.follow("sam", "ren", day_of(now())).unwrap();

        let signed_out = get_user_profile(
            State(state.clone()),
            HeaderMap::new(),
            Path("ren".to_string()),
        )
        .await
        .unwrap();
        assert_eq!(signed_out.followers, 1);
        assert!(!signed_out.viewer_follows);
        assert!(!signed_out.online);

        let mut headers = HeaderMap::new();
        headers.insert(
            header::COOKIE,
            format!("{SESSION_COOKIE}={token}").parse().unwrap(),
        );
        let as_sam = get_user_profile(State(state.clone()), headers, Path("ren".to_string()))
            .await
            .unwrap();
        assert!(as_sam.viewer_follows);
        // A public profile request only becomes an authenticated request when
        // its optional session is valid, and that is enough to mark its
        // reader present even though the profile being read is somebody else's.
        assert!(state.is_online("sam", now()));
    }

    // The one field that becomes a URL in every reader's browser is checked
    // before it is stored, not on the way out.
    #[tokio::test]
    async fn an_edit_is_refused_whole_when_the_avatar_is_not_a_picture() {
        let state = test_state();
        state.store.create_user("sam", now()).unwrap();
        let sam = SessionIdentity {
            username: "sam".into(),
            email: Some("sam@example.com".into()),
            guest: false,
        };
        let edit = |avatar: &str| EditProfileRequest {
            display: "Sam".to_string(),
            bio: "learning Spanish".to_string(),
            cefr: "B1".to_string(),
            avatar: Some(avatar.to_string()),
        };

        assert_eq!(
            edit_user_profile(
                State(state.clone()),
                Extension(sam.clone()),
                Json(edit("javascript:alert(1)")),
            )
            .await
            .unwrap_err()
            .0,
            StatusCode::BAD_REQUEST
        );
        // nothing was written, not even the fields that were fine
        assert_eq!(state.store.load_profile("sam").unwrap().unwrap().bio, "");

        edit_user_profile(
            State(state.clone()),
            Extension(sam),
            Json(edit("https://cdn.example.com/sam.png")),
        )
        .await
        .unwrap();
        let profile = state.store.load_profile("sam").unwrap().unwrap();
        assert_eq!(profile.display, "Sam");
        assert_eq!(
            profile.avatar.as_deref(),
            Some("https://cdn.example.com/sam.png")
        );
    }

    fn test_state() -> Arc<AppState> {
        let skill = crate::course::tests::skill("row0", 0, 0, &["hola"], 36);
        let course = crate::course::tests::course_of(
            vec![skill],
            crate::course::tests::solo_sentences(&["hola"], 0),
        );
        let courses = HashMap::from([(
            "spanish_for_english".to_string(),
            (course, Dictionary::from_entries(Vec::new())),
        )]);
        AppState::new(
            courses,
            Store::memory().unwrap(),
            CredentialService::new("http://127.0.0.1:1"),
            false,
            "http://localhost:4773".to_string(),
        )
    }
}
