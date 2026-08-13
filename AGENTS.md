# AGENTS.md: mimi

Notes for anyone, human or agent, picking up the whole of Mimi. This is the umbrella
file: it describes how the four pieces fit together and what is true *between* them. Each
piece has its own documentation, and where this file and those disagree, they are right.

## What Mimi is

A language learning application that anyone can edit, Duolingo's learner experience over
Wikipedia's authoring model. Courses are not shipped as code or data files; they are wiki
pages that anybody can edit, and the learner site rebuilds itself from them. AGPLv3, in
the `LICENSE` at this level, which covers all four components.

## Repository layout

This directory is a single repository. The four subdirectories are components of the
same clone and share its `github.com/thooton/mimi` remote:

| directory       | what it is                          | language                      |
| --------------- | ----------------------------------- | ----------------------------- |
| `mimi_auth`     | shared credential service           | Go 1.24, SQLite, Argon2id     |
| `mimi_backend`  | learner API and lesson builder      | Rust 2024, axum, SQLite       |
| `mimi_editor`   | the wiki authors write courses in   | MediaWiki 1.46 in Docker, PHP + Vue |
| `mimi_frontend` | the learner site                    | Astro + React + TypeScript    |

Run Git commands from this directory and keep a change spanning multiple components in a
coherent commit when that is the clearest unit of work. Runtime compatibility still
matters when components deploy separately: deploy the tolerant side first (a backend
that serves a new field before any client reads it, a wiki that accepts new content
before the backend requires it) and the demanding side second.

The root `MODULE.bazel` and each component's `BUILD.bazel` make Bazel a thin dependency
graph over those native toolchains. This is deliberately not a conventional Bazel setup:
the targets are local `genrule`s which run `cargo`, `go`, `npm`, Tailwind, and Docker
Compose in their own source directories. Do not replace them with language rules,
toolchains, sandboxes, or other parallel build machinery; the simple shell commands and
their declared invalidation inputs are the design.

## Where the real documentation is

- `mimi_backend/AGENTS.md`, the long one. The spaced-repetition ladder, the lesson
  builder, the API, and the invariants that quietly break if you ignore them. Read it
  before touching anything in `mimi_backend/src`.
- `mimi_editor/AGENTS.md`, namespaces, content models, schemas, the Vue editor.
- `mimi_editor/README.md`, `mimi_auth/README.md`, how to run each, and why they are
  arranged the way they are (`mimi_auth`'s is the design rationale for shared sign-in).
- `mimi_frontend` has **no** AGENTS.md. It is documented in comments instead; start at
  [api.ts](mimi_frontend/src/data/api.ts), which is a commented mirror of the backend's
  wire format, and [future/README.md](mimi_frontend/future/README.md), which explains why
  a pile of prototype pages sits outside the build.
  [astro.config.mjs](mimi_frontend/astro.config.mjs) is worth reading before deploying:
  public profiles are one prerendered page serving every name, so **the host must rewrite
  `/u/<name>` onto `/u/` with a 200**. Without that rule every profile link 404s. The dev
  server's copy of the rule lives in the same file, and `public/_redirects` carries the
  Netlify/Cloudflare form.

## How the four connect

```
             authors                                     learners
                |                                            |
        mimi_editor (wiki)  ──── polled for content ──▶  mimi_backend  ◀── HTTP ── mimi_frontend
       :4771 mimi.localhost                                 :4772                      :4773
                |                                            |
                └────────── verify credentials ──────────────┘
                                     ▼
                                 mimi_auth
                              127.0.0.1:4770
```

Three contracts, each owned by one side:

**Content (editor → backend).** The wiki is the single source of course content at
runtime. `mimi_backend`'s `wiki.rs`/`snapshot.rs`/`convert.rs` poll `Course:`, `Skill:`,
`Tips:` and `Glossary:` pages through `api.php` and project them into the loader's shape;
a bad edit is rejected at assembly and the previous generation keeps serving. So the
*schemas* live in `mimi_editor/extensions/MimiIncubator/schemas/` but the *validation that
matters* is `mimi_backend`'s `loader::assemble`. Changing what a course may contain means
touching both, and the wiki should be the permissive one.

**API (backend → frontend).** JSON over HTTP, no generated client: the types in
`mimi_frontend/src/data/api.ts` are hand-written against `mimi_backend/src/server.rs`.
Nothing checks that they agree, so if you change a response shape, change both. Note that
grading happens on the client, lessons are served with their answers, so a scoring
change may be a frontend change even though it sounds like a backend one.

**Credentials (both → auth).** `mimi_auth` is the only place passwords and addresses
live. The backend and the wiki each verify against it and each keep their own session;
neither knows about the other, and signing in on one does not sign you in on the other. It
issues no tokens, which is why editing a credential requires the current password and why
`mimi_auth` itself cannot log anybody out, ending sessions is each consumer's job, and
the backend does it (`Store::delete_other_sessions`) when a password changes. Browsers
talk to a consumer's own `/auth/*`, never to `mimi_auth` directly.

## Bringing the stack up

Dependency order, start the earlier services first. The backend waits for an offline
wiki, retrying once a second, while other missing dependencies may still fail:

```sh
cd mimi_auth     && MIMI_AUTH_ADDR=0.0.0.0:4770 go run ./cmd/mimi-auth
cd mimi_editor   && docker compose up -d      # first time: cp config/LocalSettings.example.php config/LocalSettings.php
cd mimi_backend  && cargo run
cd mimi_frontend && npm run dev
```

The equivalent Bazel entry points, run from this directory in the same order, are
`//mimi_auth:dev`, `//mimi_editor:dev`, `//mimi_backend:dev`, and
`//mimi_frontend:dev`. `bazel build //...` builds every non-interactive target; the dev
wrappers are tagged `manual`, so that command does not start services.

`MIMI_AUTH_ADDR=0.0.0.0:4770` is not optional if you want wiki sign-in: the default
`127.0.0.1` means *inside the container* when MediaWiki dials it. The backend, running on
the host, is happy with the default.

Defaults assume all four are local and occupy one consecutive block: auth is `4770`, the
editor is `4771`, the backend is `4772`, and the frontend is `4773`. Accordingly, the
backend reads `WIKI_API=http://mimi.localhost:4771/api.php` and
`MIMI_AUTH_URL=http://127.0.0.1:4770`, allows CORS from
`MIMI_FRONTEND_ORIGIN=http://localhost:4773`, and the frontend calls
`PUBLIC_MIMI_API=http://localhost:4772`. The wiki's learner link uses `4773` as well.
Every listener fails when its assigned port is unavailable rather than searching for a
free one. Choose an override explicitly with `MIMI_AUTH_ADDR`, the editor's paired
`MIMI_PORT`/`MIMI_SERVER`, or `PORT` for the backend and frontend; update dependent URLs
at the same time.

Databases are files and disposable: `mimi_auth/mimi-auth.db`, `mimi_backend/mimi.db`, and
the wiki's Docker volumes. There are no migrations anywhere in the project, when a shape
changes, delete and re-seed. The wiki seeds itself with the demo Spanish course, but
**the backend seeds no accounts**: a wipe costs every account you made, and a fresh
backend has no profile and no leaderboard entry to look at until somebody registers and
finishes a lesson. Nothing in the learner record is invented.

## Working across the components

- **Test with the native tool, per component:** `cargo test`, `go test ./...`,
  `npm test` (node's runner over `src/**/*.test.ts`). There is nothing that runs them all.
- **Check the whole path for a user-visible change.** A new field is not done when the
  backend serves it; it is done when a wiki page can express it, the backend validates it,
  and the frontend renders it. Say which of the four you actually exercised.
- **Match the house comment style.** All three documented components explain *intent* at
  length: why a constant has its value, why an approach was rejected. Terse code that
  merely restates itself reads as foreign here. The same applies to these documents: when
  the code and an AGENTS.md diverge, fix the document rather than leaving it.
- **Design references are in the tree.** `mimi_editor/Course_*.txt` and their `.webp`
  screenshots describe the original Duolingo authoring screens; read them before
  redesigning the course tree or sentence editor.
