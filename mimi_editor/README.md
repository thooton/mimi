# mimi editor

A stock MediaWiki 1.46.0 installation for local development.

## Start

```sh
cp config/LocalSettings.example.php config/LocalSettings.php
docker compose up -d
```

Open <http://mimi.localhost:4771>.

The `MimiIncubator` extension adds structured `Skill:`, `Course:` and
`Glossary:` pages.
Run `./build-tailwind.sh` after changing utility classes in its templates, or
`build-tailwind.bat` on Windows.
A fresh wiki starts empty; use **Special:NewCourse** to create the first course,
then add skills and a glossary beneath it.
Use each page's **Edit** tab to open its Vue/Codex editor. Course skills can be
dragged within a row or between rows, and a skill's words, a word's sentences
and a glossary form's translations are reordered by dragging the grip at the
left of each row — or by focusing it and pressing the up and down arrows.

The main page is replaced with Mimi's front page, laid out after Wikipedia's:
a welcome banner over the wiki's totals, then boxes for the published courses,
a sample of the sentences they teach, what was edited most recently, and how to
help. It is written only while the main page still holds the text MediaWiki's
installer left there, so an edited front page is never overwritten.

The initial administrator account is `Admin` with password
`mimi-editor-admin`. Change it after signing in. `MIMI_ADMIN_PASSWORD`
overrides that initial password when installing an empty database; it has no
effect once the database exists.

## Signing in

Accounts belong to `mimi_auth`, the small service the other Mimi sites also
verify against, so one username and password work on all of them. Signing in
here does not sign anyone in on the learner site — each site keeps its own
session — but there is only ever one account and one password to remember.
Both the sign-in and the create-account forms are MediaWiki's own; they just
ask `mimi_auth` instead of the wiki's user table.

`Admin` is the exception. It lives only in the wiki's own table, and the
provider is arranged so local accounts still work — including while `mimi_auth`
is down, which is exactly when somebody needs to sign in as `Admin`.

Start `mimi_auth` so the wiki can reach it:

```sh
cd ../mimi/mimi_auth && MIMI_AUTH_ADDR=0.0.0.0:4770 go run ./cmd/mimi-auth
```

**The address matters.** `mimi_auth` defaults to `127.0.0.1:4770`, and inside
the MediaWiki container that means the container itself, so the default refuses
every sign-in. Compose maps `host.docker.internal` to the host, and
`$wgMimiAuthUrl` (or `MIMI_AUTH_URL`) points there; the service still has to be
listening on an address the container can dial. Bind it to the Docker bridge
address instead of `0.0.0.0` if you would rather not expose the port.

With `mimi_auth` stopped, a Mimi account's sign-in fails as "wrong password",
because the wiki cannot tell a rejection from an unreachable service. The real
reason is in `docker compose logs mediawiki`, which is what
`$wgDebugLogGroups['authentication']` is set for.

Changing a password works here and changes it everywhere, because it is changed
in `mimi_auth`. **Change password** asks for the current one as well as the new
one — `mimi_auth` has no tokens, so retyping the old password is what authorises
replacing it — and the new password works on every Mimi site immediately, while
the old one stops working on all of them.

It does not sign anyone out, though. Each site holds its own session and
`mimi_auth` cannot reach them, so sessions opened before the change stay open.

MediaWiki's own password form is refused for these accounts, since it would
write a second password into the wiki's user table that kept working after the
Mimi one changed. `Admin` still uses it, having no Mimi account to change.

`config/LocalSettings.php` is **not** checked in: its keys belong to one
installation rather than to the project. `config/LocalSettings.example.php` is,
and copying it is the first command above — everything except those keys is the
shared configuration, so editing the example is how the wiki changes for
everybody. Copy it before the first start: Compose mounts the file read-only,
and Docker answers a missing mount source with a root-owned directory in its
place rather than an error, which then takes `sudo` to clear away.

The installer only runs when the database is still empty; it builds the schema
and the administrator account, and the settings file it insists on generating
is discarded, because the mounted copy is the wiki's configuration. The keys in
the example are placeholders, fine for a wiki nothing but localhost can reach
and worth regenerating before it is reachable by anything else. Production may
instead supply `MIMI_SECRET_KEY`, `MIMI_UPGRADE_KEY`, and `MIMI_DB_PASSWORD`
through Compose, which keeps installation-specific values out of the mounted
file. `MIMI_FORCE_HTTPS=true` enables MediaWiki's HTTPS-only mode when a reverse
proxy sends `X-Forwarded-Proto: https`; leave it false for the ordinary HTTP
development server.

## Windows

Docker Desktop with the WSL 2 backend is all that is required — the commands
above are unchanged.

Two things are worth knowing:

- Windows does not resolve `*.localhost` the way Linux does. Chrome, Edge and
  Firefox map it to 127.0.0.1 themselves, so browsing works, but `curl` and
  `Invoke-WebRequest` will not resolve the name. Add
  `127.0.0.1 mimi.localhost` to `C:\Windows\System32\drivers\etc\hosts` (as
  administrator) if you need those.
- The editor uses port 4771 rather than port 80, so it avoids the IIS and
  `http.sys` conflict common on Windows. If 4771 is already occupied, override
  both `MIMI_PORT` and `MIMI_SERVER` in a `.env` file; their ports must agree or
  MediaWiki will generate links pointing at the wrong port.

Do not disable the repository's `.gitattributes`. The container entrypoint is a
shell script mounted straight into Linux, and a CRLF checkout stops it booting.

## Stop

```sh
docker compose down
```

The database and uploaded files are retained in Docker volumes. MariaDB gets a
two-minute shutdown grace period: Compose's ten-second default can kill it
while it is flushing `tc.log`, leaving the next start unable to recover that
transaction-coordinator log. To remove the installation data as well, run
`docker compose down --volumes`.
