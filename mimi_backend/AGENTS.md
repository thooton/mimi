# AGENTS.md: mimi_backend

A guide for AI agents (and humans) working on this codebase. The source files carry long
comments explaining *why* things are the way they are; this file is the map, not the
territory. When it and the code disagree, the code is right and this file needs fixing.

## What this project is

Backend for a work-in-progress Duolingo-style language learning app (Rust, edition 2024,
axum, SQLite). Its distinguishing feature is that lesson content is not fixed: each
lesson is **generated per-user, on demand**, by combining
[FSRS](https://github.com/open-spaced-repetition) spaced repetition with a
difficulty-targeting algorithm based on the "Eighty-Five Percent Rule for Optimal
Learning."

**Grading happens on the client**, not the server: lessons are served *with* their
answers (a learner who wants to cheat already can, and instant feedback can't wait for a
round trip), and the client reports per-exercise and per-word verdicts back.

`cargo run` reads the wiki at `http://mimi.localhost:4771/api.php`, waiting and retrying
once a second if it is not online yet, uses the private credential service at
`http://127.0.0.1:4770`, and serves on `127.0.0.1:4772`.
Override these with `WIKI_API`, `MIMI_AUTH_URL`, `MIMI_FRONTEND_ORIGIN`,
`HOST`/`PORT`, and `DB_PATH`; set `MIMI_SECURE_COOKIES=true` behind HTTPS. Repeat
`--language NAME=CODE` for language names outside the built-in table. `cargo test` runs
everything.

## Project status

Several pieces are stubs. Don't "fix" these without being asked:

- **Glossary coverage.** The wiki glossary is the single source for both vocabulary and
  tap-to-gloss annotations. It is currently small, so unlisted scenery words remain
  unannotated until authors fill it in; this is expected, not a reason to restore the
  Wiktionary-derived `spanish.dict` runtime path.
- **Old databases are disposable.** No migrations; delete `mimi.db` when a shape changes.
- **`title` is the one authored field with no writer.** It is a granted badge ("Tutor"),
  and a field anybody may type into is not a badge. Everything else a person says about
  themselves is editable, `display`/`bio`/`cefr`/`avatar` through `PUT /me/profile`, and
  `course_id` through its own narrow writer.
- **Mimi hosts no images.** An avatar is a URL to somebody else's server, checked hard on
  the way in (`profile::avatar_url`) because it becomes an `<img src>` in every reader's
  browser. Uploads would mean storage, moderation and a bill; that is not a missing
  feature to add.
- **Only `Again`/`Good` ratings** on `Card`, deliberately: the app only knows "wrong" vs
  "right".
- **Every valid wiki course is served.** `GET /courses` is the live catalog; learner
  state and activity are partitioned by its stable `<target>_for_<source>` course ids.

## Architecture

```
src/
├── main.rs       snapshot wiki → assemble course/dictionary → poll → serve
├── server.rs     routes, handlers, AppState. Wiring only: load, check, hand to a view
├── api.rs        every request/response body, each with an `of(...)` that builds it
├── store.rs      SQLite: users (incl. guests), profiles, sessions, follows, messages,
│                 day-by-day activity, one mutex'd connection
├── profile.rs    Profile/ProfileEdit (authored, checked) + Activity/Counts/Day/History
│                 (derived) + Follow
├── messages.rs   the inbox: its SSE feed, HTTP commands, and the broker that routes a
│                 message to whoever is looking
├── leaderboard.rs the weekly board: Monday arithmetic + one week's activity ranked
├── quests.rs      three rotating daily goals measured from today's activity
├── wiki.rs       read-only MediaWiki client, with coherent timestamp-pinned reads
├── snapshot.rs   cached wiki page set + incremental refresh
├── convert.rs    pure snapshot → loader definitions + flat glossary dictionary
├── loader.rs     converted definitions → validated Course
├── user.rs       ★ a learner: their words and their progress; applies lessons
├── word.rs       ★ one word for one user: the ladder + its three FSRS cards
├── lesson.rs     ★ a served lesson, and the Builder that fills its review section
├── sentence.rs   ★ a bidirectional sentence, its served phrasings, bracket expansion
│                 and word location
├── card.rs       FSRS memory model for one (word, mode) pair
├── course.rs     the sentence pool + word index + the tree + word-bank tiles; and
│                 `exercise()`, which builds a question on demand
├── exercise.rs   Exercise + Mode + Ask + Side, and what a word-bank tile may be
├── skill.rs      a themed batch of words with a grammar focus; Castle; the level maths
├── vocab.rs      the word list: id, dictionary form, `forms`, `glosses`
├── position.rs   (skill, lesson), an address, not a learner's identity
├── dictionary.rs target→source dictionary; glosses target-language text
└── gloss.rs      a span of text + its possible meanings
```

The four starred files are the heart of it, and the split is deliberate: **`word.rs` owns
one word's rules, `user.rs` owns everything spanning several words, `lesson.rs` owns
choosing what to serve, and `sentence.rs` owns turning authored prose into something
gradeable.** `user.rs` never touches a stage or a counter directly; `lesson.rs` reads a
user only through `allows`, `probability` and `due`.

The frontend (`../mimi_frontend`, Astro + React) talks to this server cross-origin in
development. CORS allows only `MIMI_FRONTEND_ORIGIN` and permits credentials because the
backend session is an HttpOnly cookie. `mimi_auth` is not a browser-facing service.

## Domain vocabulary

- **Word**, the atom of spaced repetition, a `String` (`"hola"`). Deliberately not one
  inflection: `comer` is asked through whichever of `como`/`comí`/`comen` a sentence
  uses, and every verdict lands on the same card. Declared by the wiki glossary and a
  skill that teaches it.
- **Skill**, a themed batch of words that also carries a grammar `focus`. The unit of
  authoring, and **a partition of the vocabulary**: every word belongs to exactly one.
  Every skill has `SKILL_LESSONS` (36) lessons, six per level, six levels.
- **Row**, skills side by side, unlocked together and doable in any order. **Row is the
  course's only ordinal**: there is no total order over skills, but rows are strictly
  ordered and the sentence pool is sorted by row, so "everything up to row n" is a prefix.
- **Castle**, a run of rows sealed by a test. Passing it is what opens the next stretch.
- **Sentence**, the same thing said in both languages, **with no direction**. Two
  `Wording`s, `source` and `target`. The review pool is sentences, not questions.
- **Wording**, one side of a sentence: a `preferred` phrasing plus `alternatives`. The
  preferred one is *shown* (as a prompt, as the answer, as a bank's correct tiles); the
  alternatives are only ever *accepted*.
- **Side**, `Source` (the language the learner has) or `Target` (the one they're
  learning). This is the converted course's `source_lang`/`target_lang`, and outside
  `Course::direction_of` it is the only way the server refers to either language.
- **Ask**, one of the four ways to ask a sentence: `BuildSource`, `BuildTarget`,
  `WriteSource`, `WriteTarget`. `Build` taps tiles, `Write` types unaided, and the suffix
  is the side the learner produces (they are shown the other). `Ask::INTRODUCTION` is
  `BuildSource`, the gentlest of the four.
- **Mode**, which retrieval task an `Ask` is: `Scaffolding` (either word bank),
  `Recognition` (`WriteSource`), `Production` (`WriteTarget`). Declared in difficulty
  order, and that order **is** the `Ord` derive.
- **Card**, FSRS state for **one (word, mode) pair**, born from that mode's first
  verdict.
- **WordState**, one word for one user: its `stage` on the ladder, the
  `streak`/`lapses`/`repromoting` counters, and up to three cards.
- **User**, one learner's state in one course: a map of word → `WordState`, a map of
  skill → lessons done, and a count of castles passed. The store partitions these by
  `(username, course_id)`. **Progress is a set, not a point.**
- **Exercise**, one materialized question: `id`, `ask`, `prompt`, the `words` it grades,
  `row`, `skill`, the `answers` accepted (preferred first, each with its spans), and for
  word banks the correct `tiles` and a `bank` of distractors. A served introduction also
  has `new_words`, the dedicated spans of its prompt the client highlights. **Built on
  demand and never stored**, see below.
- **Phrasing**, one accepted answer as it goes on the wire: the `text`, plus a `Mark` per
  word saying which stretch of it that word owns. Settled at load, never at request time.
- **Task**, one step of a served lesson: `Material { skill, index }` or
  `Exercise { sentence, ask, introduces }`. A short-lived pointer resolved against the
  request's `Arc<Course>` while the response is built; it is never persisted.
- **Position**, `(skill, lesson)`, the lesson **1-based**. An address for a lesson to
  build, not a statement about where a learner is.

## Sentences, and why exercises aren't stored

A sentence is authored once, in both languages, pointing neither way:

```jsonc
{
    "words": ["hola"],
    "preferred_source": "[Hello/Hi]!",
    "preferred_target": "¡Hola!"
}
```

That one sentence is **four questions**, tiles either way round, typed either way round,
and which of them a learner meets depends on where each of its words sits on the
ladder. A question whose availability differs per learner cannot be a thing on a shelf,
so:

- the **course holds sentences**, sorted by row, indexed by word;
- a lesson's short-lived **tasks name a sentence and an `Ask`** while its response is
  materialized;
- **`Course::exercise(sentence, ask)`** builds the question, prompt, answers, tiles,
  the moment somebody is going to see it. Grading returns a self-describing word delta,
  so the server never rebuilds the question after a reload.

The lesson builder never builds one at all. A candidate's difficulty is a question about
the sentence's *words* and the ask's *mode*, which is why `User::allows` and
`User::probability` take `(&[String], Mode)` rather than an `&Exercise`.

Ids are generated, never authored: `greetings:1:bs` is the first sentence of the
`greetings` skill asked as `BuildSource`. The four tags are `bs`, `bt`, `ws`, `wt`.

Both sides may use **bracket groups**, which are a compact way of writing several
wordings at once: `[Hello/Hi]!` expands to a preferred "Hello!" and an accepted "Hi!",
first branches first. Several groups take the cartesian product; a group with no `/` is
rejected rather than guessed at (`[/s]` is how an optional ending is written).

## The HTTP API

JSON throughout; errors are `{"error": "..."}`. Shapes live in [api.rs](src/api.rs), each
next to the `of(...)` that builds it.

- **`POST /auth/register`** `{"username","email","password"}` → 201 and a backend
  session cookie. Proxies credential creation to `mimi_auth` and provisions the learning
  record: or **claims the caller's guest record**, if they have one (see below).
- **`POST /auth/login`** `{"login","password"}` → 200 and a backend session cookie.
  `login` may be the username or email. `GET /auth/me` returns the session identity and
  `POST /auth/logout` revokes it.
- **`POST /auth/guest`**, no body, 201 and a session cookie for a brand-new
  credential-less account. Called again with a live session it answers 200 with that
  identity and sets no cookie, so a double-click can't strand the first guest's
  progress behind a cookie it overwrote.

All four answer with the same viewer shape: `{"username", "email", "guest"}`, where
`email` is null for a guest.

The two halves of a forgotten password are public for the obvious reason, and neither
answers with a viewer: they are the only account routes with no session and no password
behind them.
- **`POST /auth/forgot`** `{"login"}` → **204, always**, whether or not that login has an
  account. `mimi_auth` answers identically and this passes the silence on: an email
  address is a login, so any difference would let anybody test who has signed up. Do not
  add one. Nothing changes on the account; the old password keeps working until a token
  is actually spent, which is what makes an unrequested reset email harmless.
- **`POST /auth/reset`** `{"token","new_password"}` → 204. The token comes from the
  emailed link, works once, and expires an hour after it was issued; `mimi_auth` owns all
  of that, and the password rules, so its refusal is the message worth showing. On
  success **every** session on the account is deleted (`Store::delete_all_sessions`),
  keeping none, unlike `PUT /me/password`, which spares the browser that asked: here
  nobody proved themselves with a cookie, and the reason to reset a forgotten password is
  often that somebody else has been using it. It deliberately does not sign anybody in,
  so the frontend sends them to `/login` afterwards.
- **`GET /me`**, `progress`, `castles`, and every word met: ladder
  `stage`, counters, and its up-to-three cards. The **tuning dashboard** for `word.rs`.
- **`GET /me/course`**, the course map: castles → rows → skills. A castle
  is `passed`/`available`/`locked`; a skill is `completed`/`available`/`locked`, with its
  level and how far into that level it is.
- **`GET /me/flashcards`**, a batch of up to 20 urgent vocabulary cards, drawn only from
  the learner's `words` map: the exact set they have encountered. Each carries a stable
  word id, a `target_to_source`/`source_to_target` direction, display text, language codes
  and an authored target-language example when one exists. The display text is **one**
  meaning: a word's first gloss, not the list of up to three that grading accepts, since
  a self-graded card needs a single answer and a source→target prompt has to name a single
  word to produce. A word with no gloss is therefore no card. Practice is endless, so a
  client calls this repeatedly, **after** submitting the batch it holds, never before.
  The endpoint always answers with the most urgent cards, and it is the submitted verdicts
  that make them urgent no longer; ask twice in a row and you get the same cards twice.
  An empty `cards` means the learner has encountered no vocabulary at all.
- **`POST /me/flashcards/submit`**, `{"cards":[{"word":"hola","direction":
  "target_to_source","correct":true}]}` applies each first verdict without moving the
  course tree. Direction routes the verdict to recognition or production. Empty runs,
  duplicates, unknown words and words this learner has not encountered are rejected
  before any card changes. Duplicates are rejected *per request*, so a long sitting that
  comes back round to a word it already practiced is fine, that is a second batch, and
  a second review.
- **`GET /users/{username}/profile`**, the authored half verbatim, everything else
  derived on the way out: `streak`, `totals`, `languages` and `days` (the feed, newest
  first, capped at 60). Words appear by their dictionary form via `Vocab::word_for`. Also
  returns `today`, the server's own midnight UTC, so the client dates the graph against
  the record's clock, not the browser's, and `xp_schedule` so it can preview an award.
  Public, but the session is *read* if one is sent: `followers`/`following` are the same
  for everybody, and `viewer_follows` is the state of the reader's own Follow button
  (false signed out, and false on your own profile). A feed day carries `followed`
  alongside what was studied, see the follows section below.
- **`POST /me/keepalive`** → 204. Private and bodyless. Every learner-site tab sends it
  every 15 seconds so an otherwise idle account remains present. The request itself is
  the signal: all authenticated requests refresh a process-local timestamp, and a
  profile's `online` boolean is true if and only if that timestamp is no more than 30
  seconds old. Presence never enters SQLite and disappears on server restart.
- **`GET /leaderboard`**, the weekly board: `{"week_start", "resets_at", "standings"}`,
  best first, where a standing is `{"rank", "username", "display", "xp"}`. Public and
  session-free like a profile, and **the whole board**, no page, no cut-off. `rank` is
  competition rank (a tie shares a place, the next one skips). Everything is summed from
  the activity rows since Monday when the request is served, so nothing is stored and
  nothing happens when the week turns over. **Guests are not ranked** (see below).
- **`GET /me/inbox`**, the inbox's **Server-Sent Events** feed: a `threads`
  snapshot immediately, then live `message` and `read` events (see the messages section
  below). **`GET /me/inbox/with/{username}`** opens and returns one `thread`, **`POST`**
  to the same path sends `{"body":"…"}`, and **`PUT`** to its `/read` child marks
  watched arrivals read. All are refused with 403 for a guest and use the same CORS
  policy as the rest of the credentialed HTTP API.
- **`GET /users?q=`**, `{"users":[{"username","display"}]}`, accounts whose username or
  display name *starts with* `q`, at most `SEARCH_LIMIT` (8) of them, guests and the
  caller excluded. The inbox's "start a new conversation" box, and the only piece of
  messaging used to find a correspondent. Private, unlike a profile or the board: reading somebody's
  page needs no account, but asking the server to list people is a question only somebody
  writing a message has. An empty `q` matches nobody rather than everybody.
- **`GET /me/quests`**, three daily quests selected from the XP, lessons, correct-answer
  and perfect-lesson pool: `{"day", "resets_at", "quests":[{"id", "title", "done",
  "total"}]}`. Private but available to guests. Definitions rotate deterministically at
  midnight UTC and progress is measured from that day's row for the active course, so
  neither quests nor completion flags are stored and there is no reset job.
- **`GET /courses`**, every usable course in the current coherent wiki generation as
  `[{"id":"spanish_for_english","source_lang":"en","target_lang":"es"}]`. Public:
  it is the learner site's course catalog, including before a guest account exists.
- **`PUT /me/course`** `{"course_id": "spanish_for_english"}` → 204. The account's
  *course choice*,
  which is why it has a writer of its own rather than going through the profile editor:
  it is set from the course chooser, not from a form about who you are. 404 when the id
  is absent from the current catalog or there is no such user.
- **`PUT /me/profile`** `{"display", "bio", "cefr", "avatar"}` → 204. The authored half,
  **written whole**, the editor is a form, so a field sent back unchanged means what it
  says and an omitted one is a bad request; that is what makes clearing a field possible
  at all (`avatar: null` removes the picture). The session names the account, so no body
  can rewrite somebody else's. Checked in `ProfileEdit::of`: a display name is 1–32
  characters on one line, a bio at most 300, and `cefr` is one of A1…C2 or blank,
  nobody can check the *claim* "B2 Spanish", but that it is a level at all is checkable
  and lets the page render it as a badge. Everything counts characters, not bytes. 400
  rejects the edit **entire**, with the reason as its message, so a bad URL never leaves
  a half-applied profile.
- **`PUT /users/{username}/follow`** → 204, **`DELETE`** the same path → 204. Under
  `/users/…` because it names somebody else, but private because it is something *you*
  do: the session says who is following and a path can never nominate a follower. Both
  are idempotent, and both refuse the same things, yourself (400), a name that isn't
  there (404), a guest at either end (403). See the follows section below.
- **`PUT /me/password`** `{"current_password", "new_password"}` → 204, and
  **`PUT /me/email`** `{"password", "email"}` → 200 with the viewer shape, whose `email`
  is the address as `mimi_auth` stored it (folded). The **account settings**, and both
  are proxies to `mimi_auth` with a session in front: the account edited is the one the
  cookie names, never a field of the body. Every rule, the length floor, the
  common-password list, what a valid address is, "it must be different", belongs to
  `mimi_auth`, and its refusal is passed through with its status (401 wrong password,
  400 a rule, 409 an address already registered). 403 for a guest, who has no
  credentials to edit. A password change also **ends every other session on the
  account** (`Store::delete_other_sessions`), which `mimi_auth` cannot do for itself;
  an email change rewrites the address every live session carries, so `/auth/me` on
  another device stops answering with the old one. **There is no writer for a
  username**: it addresses a profile and names a leaderboard row, so changing one is a
  migration, not a setting.
- **`POST /me/lessons`**, builds and immediately materializes a lesson,
  returns 201 `{"lesson_id", "target", "tasks"}`. The target is the same stable
  skill/lesson or castle object the client returns on submit. **No body** = wherever
  "continue" goes (404 if nothing is
  available, 409 if a castle is due first); **`{"skill", "lesson"}`** = any lesson they
  have reached (404 no such lesson, 403 locked). Tasks are tagged
  `{"kind": "material" | "exercise", "task": {...}}` and played in order. **Answers are
  included**, spans and all, because the client grades. Nothing is stored server-side.
- **`GET /me/lessons/{skill}/{lesson}/tips`**, every material block that
  lesson would show, as `{"tips": [{"text": ...}]}`, without building or storing
  anything. The course map's "Tips" button reads it. Same reachability rules as building
  the lesson.
- **`POST /me/castles`**, builds the castle test the learner is up
  against. 403 if none is due.
- **`POST /me/lessons/submit`**, returns the self-describing result:
  `{"target":{"kind":"skill","skill":"greetings","lesson":1},"questions":[{"ask":
  "build_source","correct":true,"words":{"hola":true}}]}`. A castle target is
  `{"kind":"castle","castle":0}`. The `Ask` routes each word to its own FSRS mode;
  the response is `{"correct","total","passed"}` (`passed` is null for a skill).
  Empty lessons/questions, repeated words, unknown words and targets the learner cannot
  take are rejected before any state changes.

### Guests

A **guest** is an account with no credentials behind it, so the course can be started
before anyone has decided to sign up. It is a row in `users`, `profiles` and `activity`
like anybody else's, and it has to be, because a lesson is generated from the learner's
own FSRS state and there is nowhere else for that state to live. Everything downstream
reads a guest without knowing it is one; `users.guest` only governs what may *happen* to
the record:

- **Naming.** `guest~<16 hex>`. The `~` is load-bearing: `mimi_auth`'s `validUsername`
  permits only letters, digits and `._-`, so no credentialed account can ever be given a
  name in this shape, and the claim below can never collide. `mimi_auth` is not involved
  in a guest at all, it is the one place credentials live, and a guest has none.
- **Claiming** (`Store::claim_guest`) is a **rename**, not a copy: registering while
  holding a guest session moves users, profiles and activity onto the new name and
  clears the flag, so words, place in the tree and streak all carry over. Anything
  already under the new name is deleted first, `mimi_auth` has just minted it, so a
  record under it can only be a leftover from a discarded credential database.
- **Discarding.** Signing in to an existing account, or signing out, deletes the guest
  record. Merging two learners has no honest answer (of two stages for one word, neither
  is more true), and a signed-out guest can never be reached again.
- **Lifetime.** `GUEST_SESSION_SECONDS` is a week rather than `SESSION_SECONDS`' month,
  because the cookie is the *only* copy of a guest. `Store::create_guest` sweeps up
  guests with no surviving session on its way past, so there is no background task.
- **Not ranked.** The weekly board leaves guests off (`Store::load_activity_since` joins
  against `users.guest`). A record nobody has put a name to, which expires with its
  cookie, has no business holding a public placing, and because claiming is a *rename*,
  the week a guest had already put in arrives on the board with them when they register.
  This is the one place a guest is *not* read as an ordinary learner.

An exercise on the wire also carries `ask`, `words`, `kind`
(`"translate"`/`"word_bank"`) and `direction` (`"es->en"`/`"en->es"`), rendered in
`TaskView::of` from the exercise, the `Ask` and the course's language codes. `ask` comes
back with the verdict; `words` lets the client apply the overall verdict to a tagged word
that could not be marked precisely in the answer; the others let it label the question
and decide whether to draw a tile board. **`Course::direction_of` is the only place in
the server that names a language.**

An introduction additionally carries `new_words`: one `{word,start,end}` for every word
its `introduces` list announces, pointing into the preferred target-language `prompt`.
These are presentation spans, the learner site colours those exact runs purple, and
are deliberately separate from both kinds of annotation already on the wire: answer
spans divide grading credit, while `prompt_glosses` are optional dictionary hints. A new
word needs its span even when it has no gloss, and a one-word exercise has a new-word span
even though its answer deliberately has no grading spans. Assembly only admits a sentence
as a first contact when it can locate every word that sentence might introduce.

An answer is **text plus spans**, not a string with markup written into it:

```jsonc
{ "text": "¡Hola, Juan!", "words": [{ "word": "hola", "start": 1, "end": 5 }] }
```

`text.slice(start, end)` is the stretch that proves `hola`, and that is what makes
**per-word grading** possible. It is not decoration and it is not authored, the loader
records the spans `sentence::locate` found (see `Phrasing::of`), and a word is marked in
**every** wording of a side or in none of them, so grading can't depend on which one the
learner hit.

**An exercise testing one word carries no spans at all, and is graded all or nothing.**
Spans exist to divide credit, and one word has nothing to divide it with: the sentence
*is* that word's question, and the rest of it is scenery the learner still had to produce.
Marking it anyway would score `nina` right for answering "The girl." with "Juan girl", a
right word inside a wrong sentence, reported to the ladder as a clean success. So
`loader::wording` skips locating entirely below two tagged words, which is also the
cheaper path and the common one. Nothing was added at either end to express this: an
answer with no spans already meant "give every word the overall verdict", which is exactly
the rule. Partial credit is for answers with something to be partial about.

**Offsets are UTF-16 code units**, because the only thing that reads them is a browser
and `String.prototype.slice` counts in exactly those. Rust counts bytes, `locate` returns
byte spans, and `Phrasing::of` is the single place the two conventions meet, get it
wrong and every answer with an accent before it grades half a word. `sentence.rs` has the
tests, including an astral character, where a count of `chars` would also be wrong.

This replaced an inline-marker string (`"¡[hola=Hola], Juan!"`), and the reasons are
worth keeping: the server rendered a little language on every request and the client
parsed it straight back out, an authored `[` had no representation, and locating that is
a fixed property of a sentence was being redone per request. Now the sentence carries its
finished phrasings, and its word bank's tiles, cut at load in `Wording::new`, and
serving one is a clone. `Wording`'s fields are private for that reason: `tiles` is derived
from `preferred`, and a board that can't spell the answer it grades against is the bug
that guards against.

## How the spaced repetition system works

The one-sentence version: **three genuinely different retrieval tasks, three cards per
word, one per mode; a per-word ladder decides which modes are *legal*, FSRS decides
*when* to serve them, and a verdict updates only the card of the mode that produced it,
never cross-updates.** Recognition and recall have different difficulties, one stability
value can't summarize both, and a shared card corrupted the 85% targeting. Word banks are
the most guessable, so their evidence is quarantined hardest.

### Layer 1: the memory model (card.rs)

`retrievability(timestamp)` is the estimated probability of recall *right now*, from the
FSRS forgetting curve; a never-reviewed card returns 0.0. It is **derived, never stored**.
`good`/`again` consume the card (it's `Copy`) and return the updated one.

### Layer 2: the ladder (word.rs)

```
                 streak                        streak
SCAFFOLDING ───────────────▶ RECOGNITION ───────────────▶ RECOGNITION + PRODUCTION
    ◀── lapses / decay ────────────── ◀── lapses / decay ──────┘
```

`WordState::record(mode, correct, timestamp)` is **the single place a word changes**: it
applies the verdict to that mode's card, then to the rung.

- **Stage legality.** Scaffolding serves only word banks; Recognition only `WriteSource`
  (graduation *retires* scaffolding); RecognitionProduction serves both typed modes
  **forever**, retiring recognition would halve the builder's candidate pool for exactly
  the user's best-known words.
- **Introduction sets the starting rung.** A word is introduced by `Ask::INTRODUCTION`, a
  word bank, so `Stage::introduced_by` lands everything at the bottom. The other arms are
  defensive.
- **A success at the top mode or a harder one advances the streak** (proving you can type
  it counts at least as much as proving you can tap it); a failure at any legal mode
  resets it, and failures at the top mode also count as `lapses`. A *failure* at a
  forbidden mode, only a castle or standalone flashcard can produce one, moves no
  counter.
- **Graduation is deliberately easy** (the ladder is optimistic): `STREAK_GRADUATE_*` = 2.
- **Demotion is the corrector**: `LAPSE_DEMOTE` (2) consecutive top-mode failures, or a
  card decaying below `DEMOTE_R` (0.45, clearly below FSRS's 0.7–0.9 band, so merely-due
  cards don't bounce). The decay check is a sweep over all words at the end of every
  `submit_lesson`, which is what makes "back from vacation" slide words down on its own.
  A mode with no card can't be forgotten, so missing cards are skipped, and a word whose
  *both* typed cards have decayed slides two rungs in one sweep.
- **Re-promotion is quick** (`repromoting` arms `STREAK_REPROMOTE_S`/`_R` = 2), because
  relearning is faster than learning. The demoted mode's card is left alone, its
  honestly-decayed R is the right prior, and on re-promotion it is maximally urgent, so
  the urgency machinery reschedules it for free.
- **Castles bypass the ladder**, so `record_test` is asymmetric on purpose: a success
  above a word's stage counts in full (card and rung), and a failure is dropped
  *entirely, card included*, letting it through would mint a damaged production card
  that the word would inherit on graduating weeks later.

### Layer 3: building a lesson (lesson.rs)

**Nothing is authored.** There is no pattern language and no scripted slot: a lesson is
`material → introductions → review`, and the only thing the course data decides is which
words the skill teaches and in what order.

`Lesson::build(course, user, username, position)` assembles the lesson at `position`,
which is **not necessarily the one the user is on**: any reached lesson may be re-taken
(anything further is locked → `None`). Everything is relative to the lesson's *skill*,
not the user, its holes draw only on rows at or before it. Re-doing an early skill must
not confront the user with a later one's vocabulary, however well they now know it.

**Introductions.** The skill's word list is the introduction queue. Lesson 1 front-loads
every still-new word that fits; later lessons take `Skill::words_for_lesson`, which
splits the list evenly over the first `INTRODUCTION_LESSONS` (12), levels 0 and 1, with
the remainder falling to the earlier ones. A word's first contact is the **gentlest
sentence using it**, the one grading the fewest words, which is a solo sentence wherever
the course has one, asked as `Ask::INTRODUCTION`. It is *asked*, not told, so the first
card is rated by an answer the learner actually gave: material teaches nothing, and this
is the only path into memory. A re-take skips words it already introduced.

**An introduction may meet several words at once.** Where the course never uses a word by
itself, its gentlest sentence grades its neighbours too, one answer meets all of them, and
`introduces` names every one that is new to this learner. The queue then skips whatever an
earlier introduction already covered, that skip is load-bearing, not tidiness: without it
a word would be introduced twice in one lesson, and *a lesson never grading a word twice*
is what the whole 85% arithmetic assumes. A word bank is what makes this survivable; it
constrains the answer enough to be solvable by somebody who has met neither word.

**Setup.** Everything the lesson is teaching right now is added to `used` before the
review section starts, **the holes are for review, and only review**. The arithmetic
would *mostly* handle this by itself, but "mostly" isn't a rule, and a **re-taken**
lesson's own words *are* in memory by then, so the most decayed of them would top the
urgent list.

Two filters decide what a hole may hold before any arithmetic: the **era rule** (nothing
past the lesson's own row) and the **legality gate** (`User::allows`: every word the
sentence grades must have been met *and* allow the ask's mode, all-or-nothing, so "never
production first" can't leak through a sentence drill, and neither can a word from a
row-mate skill they skipped).

**Phase 1 (`take_urgent`).** Walk `user.due()`, one entry per (word, mode) the ladder
allows, lowest success probability first. An attempted mode sorts by its card's
retrievability; a freshly unlocked mode with no card yet sorts by its derived
first-attempt probability, so it is **urgent**, serving it is the only way its card is
ever born (without this, "no card → never due → never served → no card" starves the mode
forever). For each, pick the (sentence, ask) **of that mode** whose success probability is
closest to 85%. Per-mode urgency matters: an RP word with a fresh recognition card and a
decayed production one must be served a *production* question. Stop when full, or when,
after at least `MIN_URGENT` (2) picks, `needed_average()` exceeds `MAX_NEEDED` (92.5%).
That cutoff exists because compensating for very hard exercises with trivially easy ones
would technically hit the average but isn't good for learning.

**Phase 2 (`top_up`).** Compute every legal reached candidate's probability once, then
repeatedly take the non-overlapping one closest to `needed_average()` capped at 92.5%,
until full or no candidate avoids repeating a word.

**Ties break at random.** Both phases shuffle their candidates before a *stable* sort or
`min_by`. Every sentence appears up to four times and the two word banks always score
identically, so without this a learner would only ever see banks facing one way.

**Ordering (`order`).** Easiest first (a warm-up), second-easiest last (end on a high
note), the rest shuffled, **the review section only**; material and introductions keep
their place at the front.

A question's **success probability** (`User::probability`) is the product, over its
words, of each word's `probability(mode)`. A word whose stage allows the mode but has no
card there yet contributes a **derived first-pick probability**, `C_RECOGNITION` (0.75)
× R(bank) or `C_PRODUCTION` (0.60) × R(recognition), which drives both scheduling and
targeting. That derivation is a pick probability only and is **never written to state**.
The independence assumption is acceptable because a lesson never grades the same word
twice.

Care when changing this:

- The **no-repeated-words rule** (`used`/`overlaps`) is load-bearing: it is what makes
  the independence assumption reasonable, and both phases rely on it.
- A lesson can come out **smaller than `LESSON_SIZE`**, the no-repeat rule may exhaust
  the pool. Intended; padding with repeats would be worse. The consequence worth knowing:
  **the more a lesson introduces, the fewer holes it can fill.**
- `needed_average` divides by the remaining holes and is only ever called while some
  remain.
- Building **teaches nothing**: a word gets a card only when a lesson is *submitted*.

### Castles (lesson.rs, user.rs)

`Lesson::castle` is a different algorithm on purpose:

- **Even sampling, not urgency.** A castle asks about the stretch as a whole, the way a
  teacher samples a term's material. A test aimed at what you are worst at is a better
  diagnostic and a worse test.
- **No 85% targeting.** The whole point is to find out.
- **Recognition and production only.** No word banks: a bank is guessable enough that
  passing one proves little. Which of the two a sentence is asked as is part of the
  sample.
- **The ladder is bypassed**, and `WordState::record_test` is what keeps that fair on the
  way out.

`CASTLE_SIZE` (20) questions, `CASTLE_PASS` (0.8) to pass. Failing costs nothing but a
retry with a fresh sample.

### Standalone flashcards (user.rs, api.rs)

Flashcards practice the same word memories as lessons; they are not a parallel scheduler.
`User::flashcards` starts from `User.words`, so a wiki word or an unlocked skill is never
enough by itself, the learner must already have answered an introduction. It chooses one
card per encountered word, most urgent first. A Scaffolding word gets the gentler
target→source recognition card; later rungs use the least-retrievable legal typed mode.
Thus target→source always updates recognition and source→target always updates
production.

**A run has no end.** `User::flashcards` returns every encountered word, and the API hands
out `FLASHCARD_BATCH_SIZE` (20) of them at a time; the client comes back for another batch
for as long as the learner keeps going, and stops by leaving the page. The batch size is
therefore not a session length, it is how far ahead of the learner the server commits,
kept short so that the *next* batch is chosen from a vocabulary the last batch's verdicts
have already reordered. A learner with fewer than 20 words will come back round to the
same ones; that is what endless means with a small vocabulary, and each pass is a real
review at whatever retrievability the previous pass left behind.

This puts an ordering requirement on the client that nothing here can enforce: report a
batch, *then* ask for the next one. Asking first returns the same cards, because they are
still the most urgent ones. `mimi_frontend`'s `flashcards.ts` does this the moment every
card in a batch has a first verdict, which is typically several retries before its queue
runs dry, so the round trip hides behind practice the learner was doing anyway.

The browser re-queues an `Again` until the card is cleared but submits only its first
verdict, just as lesson retries do. `User::submit_flashcards` validates the whole batch
before applying each direction through ordinary `WordState::record`, so right and wrong
answers alike land on exactly the recognition or production card they tested. A forbidden
failure still moves no ladder counter. The submission performs the normal decay sweep and
changes neither lesson progress nor profile activity/XP.

### Applying it (`User::submit_lesson`)

Every submitted question takes its verdict, that is what reviewing *is*. Its `Ask`
selects the mode, and its word map routes one verdict per word into `WordState::record`
(or `record_test` for a castle), so a sentence the user nearly got right doesn't punish
the words they did produce. Validation finishes before any card moves. Then the decay
sweep runs, and **only if the submitted lesson was the one the user is on** does progress
advance. Finishing a skill's last lesson clears it, which is a profile milestone and what
opens the next row.

## Profiles and the activity record (profile.rs)

Two halves, and keeping them apart is the whole design.

**Authored**, `display`, `title`, `bio`, `cefr`, `avatar`, `course_id`, is what the
user typed, stored and handed back: a bio is a claim, and so is "B2 Spanish". Nothing is
checked *against reality*, but a field is checked for being the **kind of thing** it
claims to be, which is a different question, `cefr` must be a level, `display` must be
one line, and `avatar` must be a URL safe to hand a browser. A separate table from
`users` on purpose: an account with no profile row is perfectly meaningful.

`ProfileEdit::of` is the only way to build an edit, so holding one *is* the promise that
the limits were applied; `PUT /me/profile` writes four of the fields and
`PUT /me/course` writes `course_id` alone. `title` has no writer at all, it is a
granted badge, and a field anybody may type into is not a badge.

**The avatar is a link, never a file.** Mimi hosts no images, so the field is an
absolute https URL to somebody else's server, and `profile::avatar_url` is
deliberately stricter than URL syntax: https only (naming the one safe scheme stays
right where a blocklist of `javascript:`/`data:`/`blob:` is a list somebody forgets to
extend), printable ASCII minus the characters that get reinterpreted in markup, a host
with a dot in it, and **no `@`**, so
`https://images.example.com@evil.invalid/x.png`, which fetches from `evil.invalid`
while reading as the opposite, cannot be stored. The frontend checks the same rule
again on the way out (`safeAvatar`), because the thing being rendered is one stranger's
string in another stranger's browser.

**Derived** is everything else, all from `activity(username, course_id, day, data)` keyed
by `(username, course_id, day)`, **one row per user, course and day**. Public profile
totals and streaks fold courses together by day, while each language graph reads only its
own rows.
Not one row per lesson: nothing ever asks about a single lesson afterwards, and everything
a profile *does* ask (how long is the streak, how did the score move, what was learnt in
June) is a scan over days. A row stores **deltas only**; running totals are cumulative
sums computed on the way out in `History::of`, the same way retrievability is computed per
request rather than stored, so a change to the score weights re-scores the whole history.

- **The score** is `400 + words×1.6 + skills×40 + lessons×0.5`. Deliberately *not* a
  function of FSRS state: a score that fell while you slept would read as punishment for
  not studying, and this number is shown to other people. Forgetting belongs in the lesson
  builder, which is where it is acted on.
- **XP** is 20 for a completed lesson, or 40 when every exercise was correct. Perfect
  lessons are counted per lesson in the row, because once several are folded into a day
  its aggregate correct/total can't reveal which were perfect.
- **`History::of` deduplicates skill clears**, because a user re-taking the last lesson of
  a skill re-reports clearing it every time. The rows stay a faithful log; deciding what
  it *means* happens on the way out.

`Store::update_user` takes the course id and day, and its closure returns
`(result, Activity)`, so that course's memory update and activity row land under one lock,
a lesson that moved cards but left no trace would break the streak.

Daily quests read this record at its natural resolution: one primary-key lookup for the
current UTC day. Three of four definitions are selected from the day number, advancing by
three positions each day; this turns over one quest while keeping two familiar, visits
every definition equally over four days, and needs no stored assignment. New-word and
practice-time quests are deliberately absent: the first becomes impossible after course
completion and the second is not a fact the activity row records.

### Following (store.rs, the `follows` table)

Following somebody is **an activity**, and it is dated: it appears in the follower's own
feed on the day it happened, next to the lessons they finished. That single decision is
what the design falls out of.

- **The table is the live edge and the log at once.** `follows(follower, followee, day,
  following)` keyed by the pair: a first follow inserts the row, an unfollow sets
  `following = 0`, and **the row is never deleted**. Counts and the button read the edge
  (`following = 1`); the feed reads every row. *Unfollowing somebody is not a claim that
  you never followed them*, the feed records what was done, the way the activity table
  does, and deciding what a log *means* belongs on the way out.
- **`day` is only written on the first insert**, so follow → unfollow → follow leaves the
  one entry it should, on the date it actually happened. Pressing a button twice is not
  a second event to report.
- **It is not in the activity table**, and must not be: a row there is a day the learner
  *studied*, and it is what the streak and "days studied" are counted from. A follow
  forging a link in a streak is the same bug an empty lesson would be (see
  `Activity::is_empty`). `ProfileView::of` therefore merges the two on the way out, the
  feed's days are the union, and a day with a follow and no lesson arrives as zeroes
  with a `followed` list, its score held flat at wherever the last active day left it.
- **Both ends must be somebody.** Guests can neither follow nor be followed (403): the
  record has a week to live and no name behind it, which is the same reason the weekly
  board leaves them off. `delete_account` clears follows at both ends, so a discarded
  guest never leaves a feed quoting an account that is gone.
- **A followee's display name is read when the feed is served**, not copied at follow
  time, so somebody who renames themselves is not remembered under a name they dropped.

### Messages (messages.rs, the `messages` and `reads` tables)

Private messages between two accounts. `/inbox` on the frontend is a thread list and one
open conversation; live changes arrive through one EventSource per open page.

- **A thread is a pair of people, and nothing else.** `messages.thread` is their two
  usernames sorted and joined with `\n` (no username can contain one), so there is no
  thread record to create before the first message and none to tidy up after the last,
  `Store::load_threads` derives the list from the messages themselves. Whoever writes
  first, both of them address the same conversation.
- **Events flow down; commands go up.** `GET /me/inbox` is an SSE feed carrying
  `threads`/`message`/`read`; HTTP `GET`/`POST`/`PUT` commands open, send and read.
  `Broker` is the routing table, username → their live feeds, holding no messages and
  no history, and a learner with no page open simply has no entry, which is the same
  thing as being offline. EventSource reconnects itself; the fresh `threads` snapshot
  makes the client reopen its visible conversation and recover anything missed. The SSE
  response sends `X-Accel-Buffering: no`, and `deployment/nginx.conf` also disables
  buffering, caching, compression, and the short read timeout on this exact endpoint;
  without that, NGINX may hold events instead of forwarding them as they arrive.
- **Every send publishes to both ends, the sender's own included.** The tab that wrote a
  message learns it landed the same way every other tab does, so the client has one path
  into its state rather than an optimistic copy and a real one that have to agree. The
  two ends get *different* events for the same row, because `with` names the other person
  from the reader's side.
- **The message is stored first and delivered from what was stored**, so what is on two
  screens is a row rather than two hopeful copies of it. `id` is the ordering, `sent_at`
  is for display, and two messages in the same second still have an order.
- **Unread is one number per side**, `reads(reader, thread, last_read)`: a thread is
  unread when its newest id is past that and the newest message is not the reader's own.
  Opening a conversation marks it read, and so does the client saying `read` when
  something arrives in the one on screen, without that, watching a message land would
  leave a dot behind on the next reload. The marker only ever moves forward (`MAX`),
  because two tabs can report it in either order.
- **A refusal is a command response, not a closed feed**: an empty message, one over
  `MAX_BODY` (2000 characters), a name that isn't an account, a guest, or yourself comes
  back as the ordinary JSON error shape while EventSource stays up.
- **Guests are not here at all.** The route turns them away for the reason they are not
  on the board and cannot be followed: nobody is behind the record to write to, and it
  goes when its cookie does. `delete_account` clears messages at both ends anyway.
- **A conversation is opened at its end**, capped at `THREAD_LIMIT` (200) messages, and
  there is no way to ask for what came before. That is a real limit rather than an
  oversight: paging is a second protocol, a cursor, a request for it, a client that
  knows when it has reached the top, and this one was worth having first.

### The weekly board (leaderboard.rs)

The same rows read the other way round. A profile asks "everything this user did"; the
board asks "everything everyone did this week", which is the transposed question, hence
the `activity_by_day` index, and hence `Store::load_activity_since`, the only query in
the codebase that reads across users.

- **Nothing is stored and nothing resets.** "Resets Monday" is not an event, it is
  `week_start` choosing a different range of days: a board nobody asked for costs
  nothing, and a server switched off over the weekend has no catching up to do.
- **XP comes from `Activity::xp`**, not from a second schedule. SQL cannot see into the
  JSON blob to know that a perfect lesson pays double, so the rows arrive unaggregated
  and are summed in Rust. Retuning `XP_PER_LESSON` re-scores this week's board and every
  profile together.
- **Competition rank**: equal XP shares a place and the next one skips, so the board
  never claims a difference the numbers don't support. Ties sort by username, because a
  `HashMap`'s order is not stable and a board that reshuffled under a reader who changed
  nothing would look broken.
- **No XP this week, no row.** A wall of zeroes ranks nothing.
- Guests are excluded, see the guests section above for why, and for what happens the
  moment one registers.

**There is no example account.** `seed.rs` and the invented learner `aiko` were removed:
a fresh database has no accounts in it, and every number the profile page and the
leaderboard show belongs to somebody who actually earned it. The consequence to expect is
that a new install has nothing to look at until you register and finish a lesson, that
is the intended trade, not a regression. `Store::put_account` went with it, so the only
ways into the tables are now registration, the guest path, and `update_user`.

## Wiki course data

Runtime content comes from `mimi_editor`. `wiki.rs` takes a coherent snapshot,
`snapshot.rs` incrementally refreshes its reachable course/skill/tips/glossary pages,
`convert.rs` projects those pages into loader definitions plus a flat tap-to-gloss map,
and `loader::assemble` applies all course validation. The glossary is read twice:
grouped lemma → forms/translations for graded vocabulary, flattened form → translations
for sentence annotations. A glossary larger than one wiki page is spread over
`Glossary:<course>/<letter>` subpages, so both readings go through
`convert::glossary_entries`, which unions the page the course names with everything
beneath it, in title order. Nothing on the wiki lists those segments, which is why
`snapshot::refresh_at` seeds its walk with every glossary title as well as every course
title: a segment nobody linked is still part of the glossary. Both readings keep only the
first `convert::GLOSSES` (3) meanings of anything: a wiki glossary is written for a reader
with time, and a tap on a word should answer in a line rather than a column. A successful
rebuild swaps both projections together; a bad poll or bad edit leaves the previous
generation serving and is retried once a second until it works again. An idle, healthy
wiki is still polled every five seconds.

When a spelling is both its own lemma and another lemma's form, the lemma owns its tap
gloss completely. Form definitions are merged only when no lemma with that spelling
exists; otherwise an ordinary dictionary word such as `como` would be explained partly
as itself and partly as an inflection of `comer`.

Glosses are capitalised to match the word they sit under, "El niño" glosses as "The", not
"the", in `dictionary::matching_case`, which copies a capital across but never a lower
case, so a meaning spelt with a capital of its own ("I", "Marco") keeps it.

A word carries two spelling lists. `forms` is every target-language spelling of it;
`glosses` is the same on the source side, so both directions can be graded word by word.
**Neither is pruned for ambiguity**, deliberately: a sentence is only ever searched for
the handful of words it tags, so a form is confusing only when two of *those* words offer
it: `sentence::locate` drops exactly those clashes, per search.

**A wiki sentence is filed under one word; a converted one tags every word of its skill
it uses.** An author writes "Yo como pan" under `comer`, but it exercises `pan` just as
much, and a verdict the scheduler never hears about is a review FSRS cannot see, so
`convert::tag_words_used` searches each sentence's `preferred_target` for the skill's own
vocabulary and tags what it finds. It searches with `sentence::locate`, the same matcher
the loader marks spans with, which is what makes an added tag one that can actually be
graded word by word; because the search set is the whole skill, a form two of its words
share is dropped rather than guessed at. Only the skill's own words are looked for:
anything else is scenery, and a word tagged before its skill is reached would be graded
on a learner who has never met it. **Nothing is held back to keep a sentence down to one
word**: a course whose author never writes `pan` without `comer` is a course where the
two are met together, and the transform saying otherwise would cost the scheduler exactly
the evidence the tagging exists to give it.

The converted definition of a skill:

```jsonc
{
    "id": "greetings",
    "name": "Greetings",
    "focus": "Short everyday greetings and farewells.",
    "words": ["hola", "adios"],          // also the introduction queue, in order
    "material": [                        // tips, each attached to one lesson
        { "lesson": 1, "text": "Use **hola** to say hello and **adiós** to say goodbye." }
    ],
    "sentences": [
        {
            "words": ["hola"],           // every word of this skill it uses; filed-under
                                         // word first. The rest is scenery
            "preferred_source": "[Hello/Hi]!",
            "alternative_sources": [],   // optional
            "preferred_target": "¡Hola!",
            "alternative_targets": []    // optional
        }
    ]
}
```

Assembly is **where content is validated**, so bad data stops the server at boot, or
rejects a hot reload while the previous course remains live. It rejects: a word defined twice or with no
`forms`; a word belonging to no skill or to two; castles out of order, an empty row, a
castle with no rows; a skill the layout doesn't place or a layout entry with no definition; a
skill teaching no words or a word not in the word list; material for lesson 0 or past
`SKILL_LESSONS`; a sentence tagging no words, tagging a word outside its skill, or with a
blank `preferred_source`/`preferred_target`; malformed brackets (unclosed, nested, or a
group with no `/`) and runaway expansions (over 64); and **a word no sentence uses at
all**, which nothing could introduce. It does *not* require a word to have a sentence to
itself: one that only ever appears alongside its neighbours is introduced together with
them. Conversion tests exercise the same assembly boundary with representative wiki
snapshots.

**Numbering:** rows and castles are 0-based (they are indices); lessons are 1-based, on
the wiki and in the API.

## Conventions and invariants

- **Toolchain:** plain Cargo. Dependencies are `axum` + `tokio`, `tower-http` (CORS),
  `serde`/`serde_json`, `rand`, `rs-fsrs`, `rusqlite`, and the small `futures-util`
  stream combinators used to join the inbox snapshot to its live SSE feed. Prefer not
  to add more.
- **Testing:** unit tests live in the same file as the code, with descriptive names and
  comments explaining *why* a case matters. The builder's output is randomized, so its
  tests assert on properties (no repeated words, respects the row, accuracy near target,
  opener/closer easiest) rather than exact orderings. Handlers are tested by calling them
  directly with `State(...)`/`Json(...)`; conversion and assembly with wiki-shaped fixtures. Shared
  fixtures live on the types they build (`Exercise::scaffolding`, `WordState::at`) or in
  `course::tests` (`skill`, `sentence`, `solo_sentences`, `course_of`) and `user::tests`
  (`user_with_reviews`). **Keep fixtures honest**, a word at the top of the
  ladder has all three cards, and one that doesn't will quietly aim the builder at the
  wrong question.
- **Comment style:** this codebase explains *intent* generously. Match it, and keep the
  prose in `lesson.rs`, `word.rs` and `course.rs` in sync if you change the algorithm.
- **Time:** always unix seconds as `u64`. Days are derived by dividing by 86400.
- **Tuning constants** sit at the top of the file that uses them: `LESSON_SIZE`, `TARGET`,
  `MAX_NEEDED`, `MIN_URGENT`, `CASTLE_SIZE` in `lesson.rs`; `STREAK_*`, `LAPSE_DEMOTE`,
  `DEMOTE_R`, `C_RECOGNITION`, `C_PRODUCTION` in `word.rs`; `CASTLE_PASS` in `user.rs`;
  `LESSONS_PER_LEVEL`, `MAX_LEVEL`, `ROW_GATE_LEVEL` in `skill.rs`; `BANK_DISTRACTORS` in
  `course.rs`; `MAX_EXPANSIONS` in `sentence.rs`; `FLOOR`, `XP_PER_*` and
  `PROVISIONAL_LESSONS` in `profile.rs`. Change values there, not inline.
  `GET /users/{username}` is the dashboard they're tuned against.
- **Don't break these invariants:**
  - `Mode`'s variant order is its difficulty order. Reordering it silently changes
    behaviour everywhere.
  - `Course::by_word` and `introductions` store indices into the sorted `sentences` vec,
    so the sort must happen before the indexes are built (it does, in `Course::new`) and
    `sentences` must never be reordered afterwards.
  - **Exercises are built, never stored.** Nothing may hold one across a request or put
    one in the database; a short-lived task holds `(sentence, ask)` only while the lesson
    response is materialized. Submission carries `(ask, word verdicts)` instead.
  - A word bank's tiles are seeded by the exercise id, so they are identical every time,
    a reshuffle would hand the client tiles it never showed, and make a re-taken lesson a
    different lesson. Tiles come from the produced side's language and no later row, and
    carry no leading/trailing punctuation (`exercise::tile`), a tile's "¡" or "." would
    give away where it sits in the answer.
  - A sentence's `words` list has no duplicates, and a lesson never grades a word twice.
  - New words only ever enter a user's memory by being answered.
  - A verdict updates only its own mode's card; generated holes pass the all-words
    legality gate.
  - A legal mode with no card yet appears in `due()` at its derived first-attempt
    probability: never skip it there, or the mode starves (no card → never due →
    never served → no card).
