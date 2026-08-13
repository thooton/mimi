# AGENTS.md

Notes for anyone — human or agent — picking this repository up.

## What this is

A local MediaWiki 1.46 installation in Docker, plus one bundled extension,
`MimiIncubator`, which is the actual work. The extension rebuilds Duolingo's
old course-authoring tool ("the Incubator") as ordinary wiki pages: a course
tree, its skills, and its glossary are structured JSON content models with
their own read views, diff views and Vue editors.

The two `Course_*.txt` / `Course_*.webp` pairs in the root are descriptions and
screenshots of the original Duolingo screens. They are the design reference for
the course tree and the sentence editor; read them before redesigning either.

Nothing here is a stock MediaWiki fork — `mediawiki:1.46.0` is pulled as an
image and only `config/LocalSettings.php` and `extensions/MimiIncubator` are
mounted into it. Do not go looking for MediaWiki core in the repository.

## Layout

```
compose.yaml                 mariadb + mediawiki, with the extension bind-mounted
assets/                      wiki favicon and logo, mounted at the web root
config/LocalSettings.example.php   the whole wiki configuration; copy it to
                             config/LocalSettings.php, which is untracked
docker/entrypoint.sh         installs on an empty database, then writes the front page
build-tailwind.sh / .bat     downloads the pinned Tailwind CLI and builds the CSS
extensions/MimiIncubator/
  extension.json             namespaces, content handlers, special page, RL modules
  schemas/*.schema.json      the contract every save is checked against
  includes/Content/          one handler per model + a shared base class
  includes/Action/          the edit action, which boots the Vue editor
  includes/Auth/            sign-in against mimi_auth, the shared credential service
  includes/Diff/             field-by-field diff renderer for all four models,
                             and the DiffSection it reduces a revision to
  includes/Special/          Special:NewCourse
  includes/Markdown.php      the Markdown subset tips are written in
  includes/*.php             front page tags, course catalogue, name/icon/flag helpers
  resources/editor.js        the whole Vue 3 + Codex editor, all four kinds
  resources/view.js          progressive enhancement for the read views
  resources/tailwind.css     generated — never hand-edit
  resources/frontpage.css    hand-written — the one stylesheet Tailwind does not own
  maintenance/SeedMainPage.php   the front page
```

## Running it

```sh
cp config/LocalSettings.example.php config/LocalSettings.php   # once, per clone
docker compose up -d          # http://mimi.localhost:4771, Admin / mimi-editor-admin
docker compose restart mediawiki   # after any PHP or extension.json change
docker compose down           # volumes keep the database; --volumes wipes it
```

`config/LocalSettings.php` is untracked and holds one installation's keys; the
example beside it is the shared configuration, so a change every clone should
get belongs in the example (and in your own copy). Compose mounts the copy
read-only, and Docker meets a missing mount source by creating a root-owned
directory at that path instead of failing, which then needs `sudo` to remove —
so the copy has to exist before the first `up`.

The entrypoint installs only when the `page` table is missing, then runs
`SeedMainPage.php` on every start. It is guarded: the front page is only written
while the main page still holds the installer's placeholder, so it will not
clobber an edit you made in the browser. The wiki otherwise starts empty —
create courses through `Special:NewCourse`.

`MIMI_PORT` / `MIMI_SERVER` in a `.env` file move the wiki off port 4771; both
must agree or MediaWiki generates links to the wrong port. Production can also
set `MIMI_DB_PASSWORD`, `MIMI_ADMIN_PASSWORD`, `MIMI_SECRET_KEY`,
`MIMI_UPGRADE_KEY`, and `MIMI_FORCE_HTTPS`; the checked-in defaults preserve
the simple local install. `MIMI_DATABASE_STORAGE` and `MIMI_UPLOADS_STORAGE`
may replace the default named volumes with application-owned host paths.

Signing in needs `mimi_auth` running *and listening somewhere the container can
dial*: its own default is `127.0.0.1:4770`, which inside the container means the
container, so start it as `MIMI_AUTH_ADDR=0.0.0.0:4770 go run ./cmd/mimi-auth`.
`MIMI_AUTH_URL` overrides where the wiki looks. `Admin` still signs in without
it.

## Architecture, in the order it matters

**Namespaces and models.** `extension.json` declares `Skill:` (3000),
`Course:` (3002), `Glossary:` (3004) and `Tips:` (3006) with `mimi-skill`,
`mimi-course-layout`, `mimi-glossary` and `mimi-tips` as their default content
models, so a page in the right namespace gets the right model without anyone
choosing one.

**Names carry data; content does not.** A course page is named
`Course:Spanish for English speakers`, and `CourseName` derives the course and
the language pair from the title. Skills are subpages,
`Skill:<course>/<skill>`; the glossary is `Glossary:<course>`; a skill's tips
are the same name again in `Tips:`, which is the whole of the link between the
two — neither page stores a pointer to the other. Language names
and page names are deliberately *not* stored in the JSON — successive schema
versions removed them, precisely because two sources of truth can disagree. Do
not reintroduce them.

**A glossary may be spread over segments, and they are subpages too.** A
language has more words than `$wgMaxArticleSize` has bytes — five thousand
Spanish lemmas with their forms are some fourteen megabytes — so a large
glossary is written to `Glossary:<course>/<letter>` pages, and the whole
glossary is `Glossary:<course>` *together with* everything beneath it. Nothing
lists the segments: they are found by sitting under the page the course names,
which is why the read view, `CourseCatalogue::glossaryTerms()` and
`convert::glossary_entries` in mimi_backend each query for subpages rather than
following a pointer. A small glossary stays on one page and shows no index; the
split is what a glossary grows into, not a shape it is born in. Lemmas are
unique *per page* — the check cannot span pages it has not read — so the same
word filed under two letters is possible, and the backend resolves it by
visiting pages in title order.

**A glossary read view ships data and renders rows on approach.** It is the one
read view that does not write its content out in full: a segment is fifteen
thousand rows, so `renderStructuredView()` writes the first fifty entries and
attaches all of them as JSON, and `view.js` builds a block of rows as it is
scrolled near and takes it down once it is past. Two consequences are easy to
trip over. The row markup exists twice — in PHP for the reader without
JavaScript and in `group()` for everyone else — and they have to agree. And the
blocks are measured, not watched: a block runs to thousands of pixels, so an
`IntersectionObserver` on any one node inside it reports a block as gone while
it still fills the screen. That was tried; it left blank pages.

**A diff is drawn in the vocabulary of the editor, not of the JSON.**
`StructuredSlotDiffRenderer` reduces each revision to `DiffSection`s — a word, a
glossary entry, a tip, the course tree — and draws a card per section that
differs, headed by what became of it and holding one line per changed field.
Four things about it are decisions rather than details:

- **Sections are keyed by what the author wrote**, not by position, and the two
  key lists are aligned with core's `Diff`. Anything that alignment does not
  call a copy, while existing on both sides, is reported as *moved*. Keying by
  index instead would call five thousand lemmas changed because one was
  inserted above them; asking whether an index differs has the same fault.
  The price is that renaming a word is a removal and an addition, which is
  roughly what it is.
- **Inside a section, groups are keyed by position**, because a sentence and a
  glossary form have no name of their own — so inserting a sentence renumbers
  the ones after it. The section around it stops that spreading any further,
  and a group that is empty on one side is labelled added or removed rather
  than shown as four fields that each became "not set".
- **A field whose value is the empty string is absent, not blank.** That is what
  lets a sentence with no notes say nothing about notes, and it is why the
  disabled flag contributes `'Disabled'` or `''` and never `'Active'` — a field
  that always has a value would make every group look present on both sides and
  defeat the check above.
- **The highlighting is core's `WordLevelDiff`**, and the cells keep core's
  `diff-addedline` / `diff-deletedline` classes, so an edited sentence marks the
  words that moved in the colours the rest of the wiki uses. Only the typeface
  is overridden: the editfont preference is about wikitext, and these cells hold
  prose. Derived numbers — word counts, entry counts — are deliberately *not*
  diffed; they became the tally in the bar at the top, because a count changing
  says nothing the added and removed cards below do not already say.

The four columns a `SlotDiffRenderer` is given cannot hold a label column, so
every row it emits spans all four and lays itself out inside. That is also what
lets the whole thing stack into one column on a phone.

**Validation.** `StructuredContentHandler::validateSave()` runs every save
through the JSON Schema in `schemas/` (via the small hand-rolled
`SchemaValidator`, which supports only the keywords those schemas use) and then
through a per-model `validateSemantics()` for the rules a schema cannot state:
unique words in a skill, every listed skill placed in exactly one row, castle
boundaries increasing, skills namespaced under their course.

**There are no migrations, and nothing reads an older shape.** Skill is at
schemaVersion 5, course layout at 5, glossary at 3, tips at 1, and every stored
page is at those versions: the wiki was iterated on quickly, then rewritten once
in place, and the upgrade chain that used to run when the editor opened was
deleted with it. Readers address their fields directly.

The cost is paid in **page history**: revisions written before those versions
are still stored, and nothing understands them any more. A diff or a permalink
reaching one renders the fields it cannot read as absent rather than failing —
a v4 sentence shows its text and "not set" where its translation would be. That
is accepted, not a bug to fix by reintroducing tolerance.

`normalize()` in `editor.js` survives, but it only fills in what the schema
leaves *optional* so the Vue templates have something to bind to. It is not an
upgrade step.

**Tips are stored as Markdown, and there are two renderers for it.** The editor
is a formatting canvas rather than a syntax box, so `resources/editor.js` has to
render the subset to show what is being written and serialise it back on every
keystroke, while `includes/Markdown.php` renders the same grammar for the read
view. The two must agree; render a battery of inputs through both and diff the
output after touching either. Markdown is what makes a tip diff as text in page
history and keeps the read view safe — the canvas's own HTML would do neither.

**Below 1024px every editor becomes an app, not a narrow page.** None of the
four layouts survives being squeezed, so each one turns its columns into a
sequence of screens — words → sentences → sentence, entries → entry → form,
tips → tip, and the course tree's single screen — and shows one at a time.
`SCREENS` in `editor.js` names that sequence per kind and `screenClass()` decides which way
the rest slide out; the `mimi-editor-screen*` rules in `tailwind.input.css` are
the only part that moves them, and they are inert above the breakpoint, so the
desktop layouts are still plain grids. Two things are easy to undo by accident:
`editor.js` reparents `#mimi-editor-root` to `<body>`, because Vector gives its
page container a `z-index: 0` that no descendant can escape; and the one
breakpoint lives in three places that have to agree — the media queries in the
stylesheet, the `lg:` variants in the templates, and the `matchMedia` call that
reparents. Touch is not a small mouse, either: hover-only affordances are shown
outright below `lg:`, and the course tree's cards may only be dragged by their
icon, since every other pixel of them has to stay available for scrolling.

**Six lists are ordered by hand, and five of them share one implementation.**
A skill's words, a word's sentences, and a glossary's entries, forms and
translations are all dragged by `createRowDrag()` in `editor.js`: the row picked
up follows the pointer as a ghost, the list holds a gap where it would land, and
the array is only rewritten once the ghost has flown into that gap, because
committing in the tick the ghost vanishes makes the rows jump. Each list gives it
the array, the text the ghost carries, and — where a selection is remembered by
index — an `onMove` that walks that index along with the row. A glossary entry's
first form is the lemma itself, so that structural row gives the factory a lower
bound and cannot be moved. Rows are keyed from a side band the factory keeps
rather than by position, since a translation is a plain string with no identity
of its own; that is why `addWord`, `removeSentence` and their siblings tell it
about an insertion or a deletion *before* they touch the array. The course tree
is the sixth and stays separate: it aims at a grid of seats across rows rather
than at one column, which is a different problem.

Each non-virtualized list's `transition-group` is keyed by the list it is
showing, not left unkeyed. Selecting another word hands the group an entirely
new set of row keys, so without that every row leaves as its replacement enters
— and Vue keeps a leaving row in the flow until its leave transition resolves,
which takes a frame or two even when there is none, long enough to see both
words' sentences stacked on top of each other. Keying the group swaps it in a
single patch instead. Taking leaving rows out of the flow, the usual advice, is
the wrong fix here: they would be laid over the list that is arriving. The
virtualized entries list keeps the stable row keys but not the transition group:
scrolling exchanges its rendered window continuously, and those rows must leave
immediately.

The ghost is drawn once, at the end of the app template rather than beside any
list. A `.mimi-editor-screen` carries `will-change: transform` below the
breakpoint, which makes it the containing block for anything `position: fixed`
inside it, and a ghost placed within one hangs below the finger by however far
down the screen begins. It cannot go outside `#mimi-editor-root` either: the
Tailwind border reset in `tailwind.input.css` is scoped to that subtree, and a
ghost teleported to `<body>` would lose its border.

**Sign-in is delegated to `mimi_auth`, and the ordering is the design.**
`includes/Auth/MimiAuthPrimaryAuthenticationProvider.php` is a
`PrimaryAuthenticationProvider` that verifies against `mimi_auth`'s `/v1/login`
and registers through `/v1/register`. Every Mimi site verifies against that one
service and keeps its own session, so this is *not* single sign-on — the sites
do not know about each other — but one account works on all of them.

Two properties hold the whole thing up, and undoing either breaks something
that no test will catch:

- It sorts at 50, ahead of core's `LocalPasswordPrimaryAuthenticationProvider`
  at 100, so `mimi_auth` is asked first and stays the source of truth.
- It is **not authoritative**, so a rejection ABSTAINs rather than fails and
  core's local check still gets its turn. That is the only reason `Admin` — a
  local account `mimi_auth` has never heard of — can still sign in, and it keeps
  working while `mimi_auth` is down. Core's provider is authoritative and runs
  last, so a genuinely wrong password still ends in "wrong password".

The cost is that an unreachable `mimi_auth` is indistinguishable from a wrong
password at the form. `$wgDebugLogGroups['authentication']` sends the real
reason to the container's stderr; without it the outage is silent.

**Changing a password needs its own credential type.** `MimiPasswordChangeRequest`
exists because mimi_auth wants the *current* password — it has no tokens, so
retyping it is the authorisation — and core's change request carries only the
new one. It deliberately does not extend `PasswordAuthenticationRequest`:
`LocalPasswordPrimaryAuthenticationProvider` claims a request with
`get_class( $req ) === PasswordAuthenticationRequest::class`, an exact match a
subclass would not satisfy either, and being a separate class is what stops core
writing a local password beside the one mimi_auth holds.

The provider then *refuses* core's password change for any account with no local
password, which is how autocreated Mimi accounts are told apart from `Admin`.
That refusal removes the credential Special:ChangePassword shortcuts to, so
`ChangePasswordRedirect` sends those users to the Mimi form instead of core's
"not a valid credential type"; it asks the provider rather than re-deciding, so
there is one rule about who is local.

Two traps in that flow. `providerChangeAuthenticationData()` returns **void** and
cannot report failure, so everything that can be rejected — the retype, the
policy, the current password — has to be settled in
`providerAllowsAuthenticationDataChange()` while a status can still be returned.
And password-policy failures arrive as *warnings*: `MinimalPasswordLength`
carries `suggestChangeOnLogin`, so `checkPasswordValidity()` leaves `isOK()`
true and only `isGood()` false. Testing `isOK()` lets a six-character password
through to a silent 400 from mimi_auth after the user has been told it worked —
merge the status and let AuthManager judge it, as core does.

Three things about MediaWiki's auth are easy to get wrong here, and each cost
an hour: `$wgGroupPermissions['*']['autocreateaccount']` must be granted or the
*first* sign-in of every Mimi account fails, because there is no local row yet;
the account-creation email arrives on the `User` object via `populateUser()`,
not in the `$reqs` handed to the provider; and `mimi_auth` compares usernames
`COLLATE NOCASE`, which is the only reason MediaWiki capitalising `mimi` into
`Mimi` does not create a second account.

`mimi_auth` once carried a SAML *service provider* meant to let the sites sign
in through it. A service provider consumes assertions from an identity
provider, so it can only delegate outwards and can never answer anybody else's
sign-in; it was deleted. Making SAML work would mean writing an identity
provider, which is a much larger thing.

**The front page is wikitext plus six parser tags.** `FrontPage` registers
`<mimilearn>`, `<mimistats>`, `<mimicourses>`, `<miminewcourse>`,
`<mimisentences>` and `<mimiactivity>`; the prose around them stays editable
on-wiki. The course data tags read through `CourseCatalogue`, which caches page
content for the request because summarising a course opens every skill it
lists, and the tags set a ten-minute parser cache expiry rather than depending
on an edit to the main page.

## Changing a schema

A version bump touches five places. Miss one and the failure is silent:

1. `schemas/<model>.schema.json` — the `const` schemaVersion and the fields.
2. `<Model>ContentHandler::makeEmptyContent()` — the blank page.
3. `resources/editor.js` `normalize()` — defaults for whatever the new shape
   leaves optional.
4. `StructuredSlotDiffRenderer`'s `*Sections()` method for the model — so
   history stays readable.
5. Both READMEs.

Then **rewrite every stored page in the same change**, because no reader here
tolerates an older shape and one left behind is a page that silently reads as
blank. A one-off maintenance script that walks the four namespaces, applies the
transform and saves through the ordinary page updater — so the schema checks it
on the way in — is how the last bump was done; write one, run it, delete it.

## Conventions

- PHP follows MediaWiki style: tabs, spaces inside parentheses, `Html::element`
  rather than string concatenation, services from `MediaWikiServices`.
- Comments explain *why*, in full sentences, and are load-bearing — most of the
  non-obvious decisions in this codebase are recorded only there. Match that
  register when you add code; do not strip the existing ones.
- `resources/editor.js` is Prettier-formatted (4 spaces, double quotes);
  `view.js` is MediaWiki JS style (tabs, single quotes). Keep each as it is.
- Read views are styled with Tailwind utilities written inline in the PHP.
  Rebuild `resources/tailwind.css` with `./build-tailwind.sh` after changing any
  class, and commit the result. Tailwind's content globs only cover
  `includes/**/*.php` and `resources/**/*.js`, so a class that only ever appears
  in wikitext will not be generated — that is why `frontpage.css` is hand-written.
- Preflight is off, so Tailwind's `border-*` utilities need the reset that
  `tailwind.input.css` reproduces for `#mimi-editor-root`.
- Icons are Codex icons: server-rendered as inline SVG via `Icon::codex()` in
  read views, and delivered through ResourceLoader's `CodexModule` in the
  editor. New icons must be added to the `callbackParam` list in
  `extension.json` before the editor can use them.
- Missing pages are linked red on purpose — the course tree uses red links as
  the invitation to write what is missing.

## Verifying a change

**Do not use the `preview_*` tools on this project.** They fail about half the
time here and the wiki runs on privileged port 80, which `preview_start`
refuses. Use instead:

- `curl http://mimi.localhost:4771/index.php/<Page>` for rendered output.
- `docker compose exec -T mediawiki php /var/www/html/maintenance/run.php eval`
  with statements on stdin — the fastest way to exercise a content handler,
  `validateSave()` or the diff renderer directly.
- `api.php` with `action=login` as `Admin` / `mimi-editor-admin` for a real save
  round trip (`action=edit&contentmodel=mimi-glossary`).
- For the Vue editor, drive Playwright's bundled **headless shell**
  (`chromium_headless_shell-*/chrome-headless-shell`); the full Chromium binary
  hangs. `--no-sandbox --disable-gpu --user-data-dir=$(mktemp -d)
  --virtual-time-budget=8000 --screenshot=out.png <url>`.

There is no test suite, no linter config, no composer or npm project, and no
CI. Verification is manual, and the list above is all of it.

## Gotchas

- **PHP changes need `docker compose restart mediawiki`.** Opcache keeps the old
  bytecode, so purging the parser cache first re-renders with the *previous*
  code and the change looks like it did nothing. Restart, then purge, then curl.
  JS and CSS are live immediately — they are bind-mounted.
- **Edits to `config/LocalSettings.php` do not reach anybody else.** It is
  gitignored, so a configuration change that everyone should get has to go into
  `config/LocalSettings.example.php` as well — and the two drift silently,
  because nothing compares them.
- **`.gitattributes` forces LF.** `docker/entrypoint.sh` is mounted straight into
  Linux; a CRLF checkout kills the container on a `#!/bin/sh\r` shebang. Do not
  weaken those rules.
- The front page caches for ten minutes, so front-page changes may not appear on
  the next request; `?action=purge` on the main page.
- Front page listings only count content a course actually links. An orphan
  skill page is invisible to `<mimistats>` and `<mimicourses>`.
