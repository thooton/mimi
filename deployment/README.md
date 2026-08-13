# Deploy Mimi on one Linux server

Build Mimi on a development or CI machine, then upload the resulting archive.
The production server does not need the source tree, Bazel, Go, Rust, Node, or
PHP build tools.

The archive installs this layout:

```text
/home/mimi/
  mimi_auth/       credential service and SQLite database
  mimi_backend/    learner API and SQLite database
  mimi_editor/     MediaWiki Compose application, MariaDB, and uploads
  mimi_frontend/   static learner site
  deployment/      systemd, NGINX, and environment templates
```

NGINX serves `learn.example.com`, proxies its `/api/*` requests to the backend,
and proxies `edit.example.com` to MediaWiki. Ports 4770–4772 stay private; only
80 and 443 should be publicly reachable.

## Prerequisites

The **build machine** needs Bazel 9.2.0 and the native build dependencies from
the root `AGENTS.md` (Go, Rust/Cargo, Node/npm, PHP, `zip`, `unzip`, and `tar`).

The **server** needs Linux with systemd, NGINX, Docker Engine with the
`docker compose` plugin, `openssl`, and `tar`. It also needs:

- Cloudflare-proxied DNS records for the learner and editor hostnames;
- Cloudflare SSL/TLS mode set to **Full (strict)**; and
- an origin certificate covering both names, plus its private key.

The certificate may be from Cloudflare Origin CA or a public CA. This setup
does not use Certbot.

## 1. Build and upload

On the build machine, from the Mimi source directory:

```sh
test -f mimi_frontend/.env || cp deployment/mimi-frontend.env.example mimi_frontend/.env
# Edit mimi_frontend/.env and replace edit.example.com with the real hostname.
bazel build //deployment:deploy
scp bazel-bin/deployment/mimi-deployment.tar.gz ADMIN@SERVER:/tmp/
```

That one archive contains all four applications and the deployment files.
The frontend is static, so Astro reads `mimi_frontend/.env` at build time and
embeds the editor URL in the site. Keep this untracked file on the build
machine for subsequent releases; copying an env file onto the server after the
build cannot reconfigure the generated HTML and JavaScript.

### Optional: bring the local wiki pages

To start production with the pages from the local editor, export one XML file
while the local editor is running. From the Mimi source directory on the build
machine:

```sh
cd mimi_editor
docker compose exec -T mediawiki \
  php /var/www/html/maintenance/run.php dumpBackup --current --quiet \
  > mimi-pages.xml
scp mimi-pages.xml ADMIN@SERVER:/tmp/
cd ..
```

`--current` exports only the current version of every page, not its revision
history. The dump also excludes accounts, passwords, sessions, deleted pages,
and uploaded files. Revision attribution names remain attached to the exported
page versions, but they do not create accounts on the server. Skip this section
for an empty production wiki.

## 2. Unpack the server

Run the server commands as root. Create the service account, unpack the upload,
then switch to that account once:

```sh
useradd --create-home --shell /bin/bash mimi
usermod --append --groups docker mimi
tar --no-same-owner -xzf /tmp/mimi-deployment.tar.gz -C /home/mimi
chown -R mimi:mimi /home/mimi
chmod 0755 /home/mimi
su - mimi
```

Docker-group membership is effectively root access, but it is required for the
editor's systemd service to manage its containers. `su - mimi` starts a fresh
login shell with that new membership.

As `mimi`, create the live configuration and persistent editor directories:

```sh
cp deployment/mimi-auth.env.example mimi_auth/mimi-auth.env
cp deployment/mimi-backend.env.example mimi_backend/mimi-backend.env
cp deployment/mimi-editor.env.example mimi_editor/mimi-editor.env
cp mimi_editor/config/LocalSettings.example.php mimi_editor/config/LocalSettings.php
mkdir -p mimi_editor/data/database mimi_editor/data/uploads
chmod 0600 mimi_auth/mimi-auth.env mimi_backend/mimi-backend.env mimi_editor/mimi-editor.env
```

Edit the three server `.env` files:

- replace `learn.example.com` and `edit.example.com` with the real hostnames;
- replace the four `REPLACE_WITH_...` values in `mimi-editor.env` with four
  independent values from these commands:

```sh
openssl rand -hex 32
openssl rand -hex 24
openssl rand -hex 32
openssl rand -hex 8
```

Use them in order for `MIMI_DB_PASSWORD`, `MIMI_ADMIN_PASSWORD`,
`MIMI_SECRET_KEY`, and `MIMI_UPGRADE_KEY`. The admin password is used only to
seed an empty wiki; change it through MediaWiki after the first sign-in.

Auth listens on Docker's host bridge so both the host backend and editor
container can reach it. Check the address:

```sh
ip -4 addr show docker0
```

The templates use Docker's usual `172.17.0.1`. If the address differs, replace
it in both `mimi-auth.env` and `mimi-backend.env`. Then leave the `mimi` shell:

```sh
exit
```

## 3. Install the services and NGINX

Back in the root shell:

```sh
install -m 0644 /home/mimi/deployment/mimi-auth.service /etc/systemd/system/
install -m 0644 /home/mimi/deployment/mimi-backend.service /etc/systemd/system/
install -m 0644 /home/mimi/deployment/mimi-editor.service /etc/systemd/system/
install -m 0644 /home/mimi/deployment/mimi.target /etc/systemd/system/
systemctl daemon-reload
```

Install the previously issued origin certificate and key:

```sh
install -d -o root -g root -m 0700 /etc/nginx/certs
install -o root -g root -m 0644 YOUR_CERTIFICATE.pem /etc/nginx/certs/mimi.crt
install -o root -g root -m 0600 YOUR_PRIVATE_KEY.pem /etc/nginx/certs/mimi.key
```

Copy the NGINX configuration, replace both example hostnames in the installed
copy, and enable it:

```sh
install -m 0644 /home/mimi/deployment/nginx.conf /etc/nginx/sites-available/mimi
ln -s /etc/nginx/sites-available/mimi /etc/nginx/sites-enabled/mimi
nginx -t
systemctl reload nginx
```

If the distribution uses `/etc/nginx/conf.d/*.conf`, install the file as
`/etc/nginx/conf.d/mimi.conf` instead; do not install it in both places.

Keep the exact `/api/me/inbox` location from the supplied configuration when
customizing NGINX. It disables response buffering and caching for the
Server-Sent Events feed while leaving ordinary API responses buffered; the
long read timeout keeps an otherwise quiet inbox connected between events,
and compression stays off so it cannot introduce another output buffer.

MediaWiki deliberately keeps `MIMI_FORCE_HTTPS=false`. NGINX handles public
HTTPS while the backend polls MediaWiki through its private HTTP listener.

## 4. Start and verify

```sh
systemctl enable --now docker.service nginx.service
systemctl enable --now mimi.target
systemctl --no-pager --full status mimi.target mimi-auth mimi-editor mimi-backend nginx
```

If `/tmp/mimi-pages.xml` was uploaded in step 1, import it now. The shell is
still running as root, so switch to `mimi`, copy the dump into the MediaWiki
container, and give the maintenance script its container path:

```sh
su - mimi
docker compose --env-file mimi_editor/mimi-editor.env \
  -f mimi_editor/compose.yaml cp \
  /tmp/mimi-pages.xml mediawiki:/tmp/mimi-pages.xml
docker compose --env-file mimi_editor/mimi-editor.env \
  -f mimi_editor/compose.yaml exec -T mediawiki \
  php /var/www/html/maintenance/run.php importDump /tmp/mimi-pages.xml
exit
```

Import into the new wiki before editing it: MediaWiki merges imports with pages
that already exist and does not replace a newer destination revision with an
older imported one. The uploaded XML can be deleted after verifying the pages.

Without an import, a fresh wiki has no course. Create one at
`https://edit.example.com/Special:NewCourse`. The backend retries until at
least one course contains valid content, then publishes every usable course
through its catalog.

Check the public routes:

```sh
curl --fail --silent --show-error 'https://edit.example.com/api.php?action=query&format=json'
curl --fail --silent --show-error https://learn.example.com/api/leaderboard
curl --fail --silent --show-error https://learn.example.com/u/a-future-name >/dev/null
```

The last request must return the shared profile page, not 404. Finally,
register a learner, sign into the editor with the same credentials, and submit
a lesson. The two sites intentionally keep separate sessions.

Useful logs:

```sh
journalctl -u mimi-auth -f
journalctl -u mimi-backend -f
```

For editor logs, run `su - mimi` and then:

```sh
docker compose --env-file mimi_editor/mimi-editor.env -f mimi_editor/compose.yaml logs -f
```

## Updating

```sh
systemctl stop mimi.target
find /home/mimi/mimi_frontend -mindepth 1 -delete
tar --no-same-owner -xzf ./mimi-deployment.tar.gz -C /home/mimi
chown -R mimi:mimi /home/mimi
systemctl daemon-reload
systemctl start mimi.target
```

The archive does not contain live `.env` files, `LocalSettings.php`, databases,
or uploads, so updating preserves them. The service files are deliberately
reinstalled on every update so lifecycle fixes take effect in the same release.
When the packaged NGINX file changes, reinstall it separately, run `nginx -t`,
and reload NGINX.

## Backups

Back up these application-owned files together with the matching release:

- `/home/mimi/mimi_auth/mimi-auth.db` and `mimi-auth.env`;
- `/home/mimi/mimi_backend/mimi.db` and `mimi-backend.env`; and
- `/home/mimi/mimi_editor/data`, `mimi-editor.env`, and
  `config/LocalSettings.php`.

Use SQLite's online `.backup` command or stop its service while copying. Use a
logical MariaDB dump rather than copying a live database directory. The
frontend has no persistent state.

Stopping Mimi is nondestructive:

```sh
systemctl stop mimi.target
```

It stops the editor containers but keeps `/home/mimi/mimi_editor/data`. Never
use `docker compose down --volumes` as an operational command.
