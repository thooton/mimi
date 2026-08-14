# mimi-auth

A small internal authentication service for mimi. It supports local registration and login backed by SQLite and Argon2id password hashing. It is the one place mimi keeps credentials: `mimi_backend` and `mimi_editor` both verify against it, and neither knows about the other. Browsers should use a consumer's own `/auth/*` API rather than reaching this service directly.

## Run

Requires Go 1.24 or newer.

```sh
go run ./cmd/mimi-auth
```

The service listens on `127.0.0.1:4770` and writes `mimi-auth.db` by default. In a multi-host deployment, set `MIMI_AUTH_ADDR` to a private interface and allow only the backend to reach it; do not publish this port through the public proxy.

## Usernames

A username is 3 to 64 ASCII letters, digits, or underscores (`[A-Za-z0-9_]`).
Its capitalisation is kept exactly as registered and returned that way anywhere
the name is displayed. Identity is case-insensitive, though: `AbCdE` and
`abcde` name the same account, so after either spelling is registered the other
cannot be registered separately. The same case-insensitive comparison applies
when signing in by username.

Names also cannot contain role or permission terms that could make an account
look more privileged than it is. This check is case-insensitive and applies to
substrings; the reserved terms are `administrator`, `bureaucrat`, `steward`,
`checkuser`, `oversight`, `admin`, `sysop`, and `moderator`.

## Passwords

A password must be 8 to 1024 characters and must not be one of the 100,000 most
used passwords. That is NIST SP 800-63B's shape for the rule: a modest length
floor paired with a blocklist, and no composition requirements, which mostly
buy predictable substitutions rather than better passwords.

The blocklist is SecLists' `10_million_password_list_top_100000`, the same one
MediaWiki screens against, embedded as `internal/auth/commonpasswords.txt.gz`.
It is checked here rather than only in the consumers because a rule the sign-up
form applies and this service does not is not a rule, `/v1/register` can be
called directly. Matching is exact, as MediaWiki's is; the list carries its own
capitalisations, so `password`, `Password` and `PASSWORD` are all refused.

Register:

```sh
curl -i http://localhost:4770/v1/register \
  -H 'Content-Type: application/json' \
  -d '{"username":"mimi","email":"mimi@example.com","password":"correct horse battery staple"}'
```

Login with either username or email:

```sh
curl -i http://localhost:4770/v1/login \
  -H 'Content-Type: application/json' \
  -d '{"login":"mimi@example.com","password":"correct horse battery staple"}'
```

Successful login validates credentials and returns the public user record. It deliberately does not issue a bearer token yet; token/session semantics should be chosen alongside the consumers of this service.

Change a password, by username or email:

```sh
curl -i http://localhost:4770/v1/password \
  -H 'Content-Type: application/json' \
  -d '{"login":"mimi@example.com","current_password":"correct horse battery staple","new_password":"a different long password"}'
```

The current password is what authorises the change, since there are no tokens
to present instead: a consumer calls this on behalf of somebody who has just
retyped it. A wrong current password and an unknown login both answer `401
invalid credentials`, so neither confirms that an account exists. Success
returns the same public user record as login, and the old password stops
working immediately.

It does **not** sign the account out anywhere. Sessions belong to the consumers,
`mimi_backend`'s cookie, the wiki's, and this service has no way to reach
them, so a password changed here leaves every existing session alive. Ending
them is each consumer's job: `mimi_backend` closes the account's other
sessions itself whenever it proxies a change (they are its own rows, so it
needs nothing from here), and the wiki does not.

Change an email, by username or email:

```sh
curl -i http://localhost:4770/v1/email \
  -H 'Content-Type: application/json' \
  -d '{"login":"mimi","password":"correct horse battery staple","new_email":"new@example.com"}'
```

The password authorises this one too. It is asked for even though the new
address is not itself a secret, because an address *is* a login: a session
somebody left open should not be enough to point an account somewhere else.
An address already registered to somebody answers `409 that email is already
registered`, and the username is untouched, that is the name, and this is a
contact detail.

Nothing is emailed on an address change. Confirming a new address, and telling
the old one that it changed, is still unbuilt: the outbox below exists for
password resets and is not yet wired to anything else.

## Forgotten passwords

Every other edit here is authorised by the current password, which is no help
to somebody who has lost it. A reset is authorised by reaching the address on
the account instead.

Ask for a link:

```sh
curl -i http://localhost:4770/v1/reset/request \
  -H 'Content-Type: application/json' \
  -d '{"login":"mimi@example.com"}'
```

It answers `202` with the same body every time, whether or not the login names
an account, and that is the point: an email address is a login here, so any
difference would turn this into a way to ask who has signed up. Only the inbox
knows. Nothing changes on the account either, so the old password keeps working
and an unrequested reset email is safe to ignore.

Spend the token from the link:

```sh
curl -i http://localhost:4770/v1/reset/confirm \
  -H 'Content-Type: application/json' \
  -d '{"token":"<from the emailed link>","new_password":"a different long password"}'
```

The token is 32 bytes from `crypto/rand`, works once, and expires an hour after
it was issued. Only its SHA-256 is stored, so a copy of `mimi-auth.db` is not a
set of working links; asking again invalidates the previous one, so an older
message in an inbox is not a second key. A token that is unknown, spent or
expired all answer `401` alike, since the difference is not something the
holder can act on. The new password faces the same length rule and the same
blocklist as every other; a rejected one does *not* spend the token.

As everywhere else here, this cannot end sessions: `mimi_backend` closes every
session on the account when it proxies a reset (`Store::delete_all_sessions`,
keeping none, unlike a password *change* which spares the browser that asked).

### Sending the mail

Delivery is [Resend](https://resend.com), one JSON POST, no SDK:

| variable               | what it does                                                        |
| ---------------------- | ------------------------------------------------------------------- |
| `MIMI_RESEND_API_KEY`  | Resend API key. Unset, links are logged instead of sent.             |
| `MIMI_RESEND_FROM`     | sender, default `Mimi <onboarding@resend.dev>`                       |
| `MIMI_RESET_URL`       | the page the link points at, default `http://localhost:4773/reset-password` |

```sh
MIMI_RESEND_API_KEY=re_… MIMI_RESET_URL=http://localhost:4773/reset-password \
  go run ./cmd/mimi-auth
```

With no key set the service starts anyway and writes each link to the log at
`WARN`. That is deliberate: a local stack can complete a reset by reading the
terminal, and a deployment that has lost its key fails visibly rather than
silently swallowing mail.

The default sender is Resend's sandbox address, which needs no verified domain
but will only deliver to the Resend account's own address. Any real deployment
must verify a domain and set `MIMI_RESEND_FROM` to an address at it.

`MIMI_RESET_URL` is a *consumer's* page, never one here: this service serves
JSON to the backend and the wiki, browsers are not supposed to reach this port
at all, and somebody following a link from their inbox has to land somewhere
that can render a form. The token is added as a `token` query parameter.

Other settings are `MIMI_AUTH_ADDR` and `MIMI_AUTH_DB`.

This service once carried a SAML 2.0 *service provider*, meant to let the other
mimi sites sign in through it. That could never have worked: a service provider
consumes assertions from an identity provider, so it can only delegate
authentication outwards to an IdP, it cannot answer anybody else's sign-in.
The sites share credentials by verifying against `/v1/login` here instead, which
is what the consumers always did in practice. Reintroducing SAML means writing
an *identity* provider, which is a much larger thing than the code that was
removed.

## Test

```sh
go test ./...
```
