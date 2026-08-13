/* The backend API (see mimi_backend/src/server.rs). All bodies are JSON;
   errors come back as { error: string } with a suitable status. The base
   URL is only configurable because the two servers run on different ports
   in development. */

const API = import.meta.env.PUBLIC_MIMI_API ?? 'http://localhost:4772';

export interface ApiPosition {
  skill: string;
  lesson: number;
}

/** how far up the three-mode ladder a word has climbed: word banks only,
    then unaided es->en, then es->en and en->es both (and forever) */
export type ApiStage = 'scaffolding' | 'recognition' | 'recognition_production';

/** one FSRS card — a word's memory state *in a single mode*. Retrievability
    is the decayed probability of recall, computed when the request is served. */
export interface ApiCard {
  retrievability: number;
  stability: number;
  difficulty: number;
  last_reviewed: number;
}

/** one word's place on the ladder, plus a card per mode. A mode that has
    never been attempted has no card yet and comes back `null` — the three are
    updated in isolation, so a word bank's verdict never touches the others. */
export interface ApiWord {
  word: string;
  stage: ApiStage;
  /** consecutive successes toward the next rung of the ladder */
  streak: number;
  /** consecutive failures at the stage's hardest mode, counting toward demotion */
  lapses: number;
  /** demoted, so the shorter re-promotion streaks apply */
  repromoting: boolean;
  bank: ApiCard | null;
  recognition: ApiCard | null;
  production: ApiCard | null;
}

export interface ApiUser {
  username: string;
  progress: Record<string, number>;
  castles: number;
  words: ApiWord[];
}

export type ApiFlashcardDirection = 'target_to_source' | 'source_to_target';

export interface ApiFlashcard {
  /** stable vocabulary id returned on submission */
  word: string;
  /** target->source is recognition; source->target is production */
  direction: ApiFlashcardDirection;
  /** language codes, e.g. "es->en". Served, but the player deliberately does
   *  not label a card with it — the front already says which language it is. */
  language_direction: string;
  front: string;
  back: string;
  /** an authored target-language usage, when the course has one */
  example: string | null;
}

/** One batch of an endless run, not a deck to finish. Ask again only after
 * submitting the batch you hold: the server always answers with the learner's
 * most urgent cards, and it is the verdicts that make them urgent no longer,
 * so two calls in a row return the same cards. No cards at all means the
 * learner has yet to encounter any vocabulary. */
export interface ApiFlashcardDeck {
  target_lang: string;
  cards: ApiFlashcard[];
}

export interface ApiFlashcardResponse {
  word: string;
  direction: ApiFlashcardDirection;
  correct: boolean;
}

export interface ApiFlashcardResult {
  correct: number;
  total: number;
}

export type ApiSkillState = 'completed' | 'available' | 'locked';
export type ApiCastleState = 'passed' | 'available' | 'locked';

export interface ApiSkill {
  id: string;
  name: string;
  focus: string;
  state: ApiSkillState;
  level: number;
  lessons: number;
  lessons_done: number;
}

export interface ApiCourseRow {
  skills: ApiSkill[];
}

export interface ApiCastle {
  castle: number;
  state: ApiCastleState;
  rows: ApiCourseRow[];
}

export interface ApiCourse {
  id: string;
  source_lang: string;
  target_lang: string;
  castles: ApiCastle[];
}

/** One currently usable wiki course, as advertised by the backend. */
export interface ApiCourseSummary {
  id: string;
  source_lang: string;
  target_lang: string;
}

/** One quest measured from today's activity row. `done` is deliberately not
    capped at `total`: it is the fact the backend recorded, and completion is
    the simple `done >= total` comparison. */
export interface ApiQuest {
  /** stable definition id, unlike the human-facing title */
  id: string;
  title: string;
  done: number;
  total: number;
}

export interface ApiDailyQuests {
  /** midnight UTC at the start of the activity day, in unix seconds */
  day: number;
  /** midnight UTC at the start of the next one */
  resets_at: number;
  quests: ApiQuest[];
}

/** one piece of a material task, in presentation order */
export interface ApiMaterial {
  text: string;
}

export interface ApiGloss {
  text: string;
  meanings: string[];
}

/** The retrieval task the backend uses to route each word verdict to the
    corresponding spaced-repetition card. */
export type ApiAsk =
  | 'build_source'
  | 'build_target'
  | 'write_source'
  | 'write_target';

/** Where one concept sits inside an answer: `text.slice(start, end)` is the
    stretch of it that proves that concept, which is what makes per-word
    grading possible.

    Offsets are UTF-16 code units — ordinary JavaScript string indices, so
    `slice` is exactly right and no decoding is needed. The backend converts
    from its own byte offsets before serving (see `sentence::Mark`). */
export interface ApiMark {
  word: string;
  start: number;
  end: number;
}

/** One accepted answer: the text as the learner should produce it, plus the
    spans of it that prove each concept.

    **An empty `words` means grade the whole answer at once**, and give every
    concept in the exercise's `words` that one verdict. That is how a
    single-concept exercise always arrives — the sentence is that concept's
    question, so there is no credit to divide — and it is also what happens to
    a concept used in a form the backend could not locate. */
export interface ApiAnswer {
  text: string;
  words: ApiMark[];
}

export interface ApiExercise {
  id: string;
  /** Returned with the question because submissions are self-describing: the
      backend does not retain a copy of the lesson after serving it. */
  ask: ApiAsk;
  /** Every vocabulary concept this exercise tests. Where an answer carries
      spans, they give the more precise per-concept verdict; where it carries
      none, grading falls back to the result for the whole exercise, and this
      list is what keeps those concepts in the report. */
  words: string[];
  /** how the exercise is answered: "translate" types the answer out,
      "word_bank" assembles it from the tokens in `bank`. Any other kind is
      answered by typing, as "translate". */
  kind: string;
  direction: string;
  prompt: string;
  /** accepted answers, canonical first */
  answers: ApiAnswer[];
  /** word-bank exercises only: the tokens the answer is assembled from,
      distractors included. Each may be used at most once, so a word appearing
      twice in the answer appears twice here. **Not in display order** — the
      canonical answer's own tokens come first, so the client has to scramble
      them (see wordBank.ts). Leading/trailing punctuation is stripped so it
      cannot reveal a token's position. Empty on other kinds. */
  bank?: string[];
  /** words the user meets for the first time here, so the client can mark
      them new. Answering is what creates their cards, so a wrong answer is
      recorded as a wrong answer. */
  introduces: string[];
  /** Exact spans of `prompt` to highlight as new. These are presentation
      metadata, independent of both answer `words` (grading) and
      `prompt_glosses` (dictionary hints), and therefore exist even when a
      one-word exercise deliberately has no answer grading span. */
  new_words: ApiMark[];
  prompt_glosses: ApiGloss[];
  answer_glosses: ApiGloss[];
}

/** A lesson is part authored script and part generated, but the client
    doesn't care which is which: it plays the tasks in order. */
export type ApiTask =
  | { kind: 'material'; task: ApiMaterial }
  | { kind: 'exercise'; task: ApiExercise };

/** Stable address returned by the backend and echoed on submission. */
export type ApiLessonTarget =
  | { kind: 'skill'; skill: string; lesson: number }
  | { kind: 'castle'; castle: number };

export interface ApiLesson {
  /** Present for display/debugging only; the backend no longer uses a pending
      lesson record when applying a submission. */
  lesson_id: string;
  target: ApiLessonTarget;
  tasks: ApiTask[];
}

/** every tip one lesson of a skill carries (the map's "Tips" button) */
export interface ApiTips {
  tips: ApiMaterial[];
}

/* --- the public profile ---

   Who a user says they are, plus everything the backend's activity record
   says they have done. The split is worth knowing about when reading these:
   `display`/`title`/`bio`/`cefr` are authored and stored verbatim, and every
   number is derived from the day-by-day record on the way out — so no total
   here can have drifted from the days that made it. */

/** one sample of a language's score, at midnight UTC on the day it was taken */
export interface ApiPoint {
  /** unix **seconds** — the whole API dates things this way; the UI wants
      milliseconds, so profile.ts multiplies once on the way in */
  t: number;
  v: number;
}

/** One course, scored so that several of them can share an axis. `id` is the
    course identity; `code` supplies its language name and flag. */
export interface ApiLanguage {
  /** the course id, e.g. "spanish_for_english" */
  id: string;
  /** ISO 639-1 target language, e.g. "es" */
  code: string;
  /** ISO 639-1 language the course teaches from, e.g. "en" */
  source_code: string;
  score: number;
  /** movement over the last week, in points */
  delta: number;
  /** too few lessons for the score to be worth trusting yet */
  provisional: boolean;
  words: number;
  skills: number;
  lessons: number;
  /** when they joined — the graph's left edge */
  since: number;
  /** oldest first, one sample per active day, anchored at both ends so the
      line spans the axis */
  points: ApiPoint[];
}

/** somebody this user started following, in a feed entry */
export interface ApiFollow {
  /** what /u/<name> is built from */
  username: string;
  /** what they call themselves *now*, not when they were followed */
  display: string;
}

/** One day of the record: what they did, and where it left them.
 *
 *  **Not necessarily a day of study.** Following somebody is dated but is not
 *  studying — it earns nothing, breaks nothing and keeps no streak — so a day
 *  whose only entry is a follow arrives with every number at zero and the
 *  score sitting where the last lesson left it. */
export interface ApiActivityDay {
  t: number;
  /** consecutive active days up to and including this one; 0 on a day with
      no study behind it */
  streak: number;
  lessons: number;
  exercises: number;
  correct: number;
  xp: number;
  /** what the day taught, in the target language — empty on a review day,
      which is most of them once a course has been finished */
  learned: string[];
  /** skills finished that day, by name */
  skills: string[];
  /** who they started following that day. **Unfollowing does not remove
      this**: the feed records what was done, not what is still true. */
  followed: ApiFollow[];
  score: number;
  /** how far this day moved the score */
  delta: number;
}

/** Backend-owned lesson rewards. Keeping both values in the contract lets
    lesson results show the larger perfect award without duplicating policy. */
export interface ApiXpSchedule {
  lesson: number;
  perfect_lesson: number;
}

export interface ApiProfile {
  username: string;
  display: string;
  title: string | null;
  bio: string;
  cefr: string;
  /** An absolute https URL to a picture on somebody else's server — Mimi
      hosts no images. The backend checks it hard before storing it (scheme,
      character set, and no credentials hiding in front of the host), and the
      client checks it again on the way in: see safeAvatar in profile.ts.
      Null for the great majority of people, who have linked none. */
  avatar: string | null;
  /** the exact course they're learning — null until they pick one. */
  course_id: string | null;
  joined: number;
  /** Live, process-local presence: the server authenticated a request from
      this account during the last 30 seconds. Independent of study days. */
  online: boolean;
  /** null if they have never done anything */
  last_active: number | null;
  /** the **server's** idea of what day it is (midnight UTC). Dates are read
      against this rather than the browser's clock, so a reader in another
      timezone sees the same "yesterday" the record was written with. */
  today: number;
  /** the run ending today or yesterday; 0 once broken */
  streak: number;
  /** how many accounts follow this one, and how many it follows */
  followers: number;
  following: number;
  /** whether whoever asked for this response follows it. False when nobody
      is signed in, and false on your own profile. */
  viewer_follows: boolean;
  xp_schedule: ApiXpSchedule;
  totals: {
    xp: number;
    lessons: number;
    exercises: number;
    correct: number;
    words: number;
    skills: number;
    /** days they were active at all */
    days: number;
  };
  languages: ApiLanguage[];
  /** **newest first**, and capped by the backend at its most recent 60 days */
  days: ApiActivityDay[];
}

/** The client's verdict for one question. `ask` selects the memory card to
    update; the target on the containing submission identifies progress. */
export interface ApiResponse {
  ask: ApiAsk;
  correct: boolean;
  words: Record<string, boolean>;
}

export interface ApiSubmitResult {
  correct: number;
  /** exercises only — material isn't answered, so it isn't scored */
  total: number;
  /** present for castle tests, null for ordinary lessons */
  passed: boolean | null;
}

/* --- the weekly leaderboard ---

   One global board ranking the XP earned since Monday 00:00 UTC, and nothing
   else. The backend sums it out of the same activity rows the profile is
   derived from when the request is served, so there is no board to be stale:
   see mimi_backend/src/leaderboard.rs. Guests are not ranked — a record with
   no name behind it and a week to live shouldn't hold a public placing — but
   registering claims the record, so the week carries onto the board with the
   name they chose. */

/** one learner's week */
export interface ApiStanding {
  /** competition rank: equal XP shares a place, and the next one skips it,
      so two people on the same total are 1 and 1, and the next is 3 */
  rank: number;
  /** identifies the row, and what /u/<name> is built from */
  username: string;
  /** what they call themselves; falls back to the username */
  display: string;
  xp: number;
}

export interface ApiLeaderboard {
  /** midnight UTC on the Monday this week began, in unix **seconds** */
  week_start: number;
  /** and the Monday it empties on — the board's own clock, so the page never
      has to work out which week it is from the browser's timezone */
  resets_at: number;
  /** best first. Everyone who has earned XP this week is here; a learner who
      hasn't started one is absent rather than sitting at the bottom on 0 */
  standings: ApiStanding[];
}

/** Who the backend thinks we are — the one shape every /auth endpoint answers
    with. A **guest** is a real learning record with no credentials behind it
    (see mimi_backend/AGENTS.md): everything works for them, but the record
    lives only as long as the cookie, so the UI offers to save it. */
export interface ApiViewer {
  username: string;
  /** null for a guest: no credentials means no address either */
  email: string | null;
  guest: boolean;
}

/** one account a search matched, for the inbox's new-conversation box */
export interface ApiFoundUser {
  username: string;
  display: string;
}

export interface ApiUserSearch {
  users: ApiFoundUser[];
}

/** Resolve a browser-only API address for a primitive that needs a URL rather
    than the request helper below. The inbox's EventSource is the one such
    primitive: it still reaches the same backend and keeps the HTTP(S) scheme. */
export function apiUrl(path: string): string {
  return `${API}${path}`;
}

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(`${API}${path}`, {
    ...init,
    // The backend owns the browser session in an HttpOnly cookie. JavaScript
    // never reads or stores it; fetch only opts into sending it cross-port in
    // local development.
    credentials: 'include',
    // the content-type goes only where a body goes: the backend reads a json
    // content-type as a promise of a body, and rejects an empty one (a POST
    // with no body is how "the lesson the user is on" is spelled)
    headers: init?.body ? { 'content-type': 'application/json' } : {},
  });
  if (!res.ok) {
    let detail = res.statusText;
    try {
      const body = await res.json();
      if (body?.error) detail = body.error;
    } catch {
      /* not JSON; keep the status text */
    }
    throw new Error(`${init?.method ?? 'GET'} ${path}: ${detail}`);
  }
  // a 204 is a successful reply with no body to parse (setActiveCourse)
  if (res.status === 204) return undefined as T;
  return res.json() as Promise<T>;
}

/**
 * The private learning record belonging to the current backend session.
 */
export function ensureUser(): Promise<ApiUser> {
  return request<ApiUser>('/me');
}

/** the course map (outline + per-node state) for a user */
export function fetchCourse(): Promise<ApiCourse> {
  return request<ApiCourse>('/me/course');
}

/** Every valid course in the backend's current coherent wiki snapshot. */
export function fetchCourses(): Promise<ApiCourseSummary[]> {
  return request<ApiCourseSummary[]>('/courses');
}

/** Three server-selected quests measured from the current learner's activity
    today. Definitions and the UTC boundary both belong to the backend. */
export function fetchQuests(): Promise<ApiDailyQuests> {
  return request<ApiDailyQuests>('/me/quests');
}

/** The next batch of FSRS-backed practice, built only from words this learner
    has already encountered. See ApiFlashcardDeck for why this must follow the
    submission of the batch before it. */
export function fetchFlashcards(): Promise<ApiFlashcardDeck> {
  return request<ApiFlashcardDeck>('/me/flashcards');
}

/** Save each card's first verdict. Its direction selects recognition or
    production on the backend; standalone practice never advances a lesson. */
export function submitFlashcards(
  cards: ApiFlashcardResponse[],
): Promise<ApiFlashcardResult> {
  return request<ApiFlashcardResult>('/me/flashcards/submit', {
    method: 'POST',
    body: JSON.stringify({ cards }),
  });
}

/** anyone's public profile — no account needed to read one, so this takes
    the username it is looking at rather than defaulting to ours */
export function fetchProfile(username: string): Promise<ApiProfile> {
  return request<ApiProfile>(`/users/${encodeURIComponent(username)}/profile`);
}

/** Keep this tab present while it remains signed in. The authenticated
    request itself is the signal; the server stores no body and returns 204. */
export function keepAlive(): Promise<void> {
  return request<void>('/me/keepalive', { method: 'POST' });
}

/** This week's board. Public like a profile: reading one needs no account,
    and which row is yours is a comparison the client makes itself. */
export function fetchLeaderboard(): Promise<ApiLeaderboard> {
  return request<ApiLeaderboard>('/leaderboard');
}

/** Everything the owner of a profile may write. The editor is a form, so it
    is submitted whole: a field sent back unchanged means what it says, and
    the only way to clear one is to send it empty (null, for the avatar). The
    backend checks all four and rejects the edit entire if any is wrong — so a
    bad avatar URL never lands a half-applied profile. */
export interface ApiProfileEdit {
  display: string;
  bio: string;
  /** one of A1…C2, or empty for "not saying" */
  cefr: string;
  /** an absolute https URL, or null to remove the picture */
  avatar: string | null;
}

/** Save the authored half of the signed-in user's profile (204 on success).
    A 400 carries the reason in its message, which is what the dialog shows. */
export function updateProfile(edit: ApiProfileEdit): Promise<void> {
  return request<void>('/me/profile', {
    method: 'PUT',
    body: JSON.stringify(edit),
  });
}

/** Accounts whose username or display name starts with `q` — the inbox's
    "start a new conversation" box (see inbox.ts for the messaging feed and
    commands).

    Signed-in callers only, unlike a profile or the board: reading somebody's
    page needs no account, but asking for a list of people is a question only
    somebody writing a message has. An empty query matches nobody. */
export function searchUsers(q: string): Promise<ApiUserSearch> {
  return request<ApiUserSearch>(`/users?q=${encodeURIComponent(q)}`);
}

/** Follow somebody (204). The session says who is following, so this only
    names who is being followed. Idempotent, and dated on the backend: the
    day it first happens is the day it appears in your activity feed. */
export function followUser(username: string): Promise<void> {
  return request<void>(`/users/${encodeURIComponent(username)}/follow`, {
    method: 'PUT',
  });
}

/** Stop following somebody (204). This ends the follow and nothing else —
    the entry in your activity feed stays, because it is a record of what you
    did rather than a claim about what is still true. */
export function unfollowUser(username: string): Promise<void> {
  return request<void>(`/users/${encodeURIComponent(username)}/follow`, {
    method: 'DELETE',
  });
}

/** Select the exact course the signed-in user is learning (204). */
export function setActiveCourse(
  courseId: string,
): Promise<void> {
  return request<void>('/me/course', {
    method: 'PUT',
    body: JSON.stringify({ course_id: courseId }),
  });
}

/**
 * Build a personalized skill lesson. With no address, the backend chooses the
 * next due lesson; with one, it serves any reached lesson in that skill.
 */
export function createLesson(
  position?: ApiPosition,
): Promise<ApiLesson> {
  return request<ApiLesson>('/me/lessons', {
    method: 'POST',
    ...(position ? { body: JSON.stringify(position) } : {}),
  });
}

export function createCastle(): Promise<ApiLesson> {
  return request<ApiLesson>('/me/castles', {
    method: 'POST',
  });
}

/**
 * All the material a lesson would show, without starting it — the course
 * map's "Tips" button. Any reached lesson may be read this way.
 */
export function fetchTips(
  position: ApiPosition,
): Promise<ApiTips> {
  return request<ApiTips>(
    `/me/lessons/${encodeURIComponent(position.skill)}/${position.lesson}/tips`,
  );
}

/** Hand the client's self-describing verdicts back. The backend validates the
    target against current progress before applying any of them. */
export function submitLesson(
  target: ApiLessonTarget,
  questions: ApiResponse[],
): Promise<ApiSubmitResult> {
  return request<ApiSubmitResult>('/me/lessons/submit', {
    method: 'POST',
    body: JSON.stringify({ target, questions }),
  });
}

export function registerUser(
  username: string,
  email: string,
  password: string,
): Promise<ApiViewer> {
  return request<ApiViewer>('/auth/register', {
    method: 'POST',
    body: JSON.stringify({ username, email, password }),
  });
}

export function loginUser(login: string, password: string): Promise<ApiViewer> {
  return request<ApiViewer>('/auth/login', {
    method: 'POST',
    body: JSON.stringify({ login, password }),
  });
}

/** Start the course without an account. Safe to call with a session already
    in hand — the backend answers with whoever that is rather than minting a
    second guest and stranding the first one's progress. */
export function startGuestUser(): Promise<ApiViewer> {
  return request<ApiViewer>('/auth/guest', { method: 'POST' });
}

export function fetchViewer(): Promise<ApiViewer> {
  return request<ApiViewer>('/auth/me');
}

/** Change the signed-in account's password (204 on success). The current one
    authorises it — the backend proxies to the credential service, which owns
    every rule about what a password may be, so the message a refusal carries
    is that service's and is worth showing verbatim.

    Every *other* session on the account is closed by this, on the reasoning
    that a password is often changed because somebody else might know the old
    one. The browser that asked keeps its own, so nothing needs re-signing in
    here. */
export function changePassword(
  currentPassword: string,
  newPassword: string,
): Promise<void> {
  return request<void>('/me/password', {
    method: 'PUT',
    body: JSON.stringify({
      current_password: currentPassword,
      new_password: newPassword,
    }),
  });
}

/** Move the account's address, authorised by the password. Answers with the
    viewer, whose `email` is the one now on file — the caller should hand it
    back to the auth store rather than assume the string it sent. */
export function changeEmail(
  password: string,
  email: string,
): Promise<ApiViewer> {
  return request<ApiViewer>('/me/email', {
    method: 'PUT',
    body: JSON.stringify({ password, email }),
  });
}

export function logoutUser(): Promise<void> {
  return request<void>('/auth/logout', { method: 'POST' });
}
