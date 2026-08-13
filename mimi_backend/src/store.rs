// Persistence. Everything the server needs to survive a restart lives in a
// SQLite database: the users (their position and their FSRS memory state),
// their profiles and the day-by-day record of what they have done.
//
// A course's memory state is a map of word -> `WordState` (see word.rs), and
// the way the server uses it is all-or-nothing: building a lesson walks
// *every* card to find what is most due. Each account therefore stores a map
// of course id -> `User`, and each `User` is the whole state of that course.
// This outer partition is load-bearing: two courses may both contain a skill
// named `basics` without either one inheriting the other's progress.
//
// The **activity** table follows the same principle one level up: one row per
// user, course and day, holding everything they did there as a blob (see
// profile.rs). Nothing ever asks about a single lesson after the fact, and
// everything the profile asks — how long the streak is, how the score moved,
// what was learnt in June — is a scan over days, so days are what is stored.
// `(username, course_id, day)` is the primary key, which makes "add this
// lesson to this course today" a single-row read-modify-write. Profiles fold
// those rows together for overall totals and keep them apart for each
// language's score.
//
// Profiles are a separate table rather than more columns on `users` because
// they are separate things: `users` is the account the learning algorithm
// reads and writes, `profiles` is what a person typed about themselves, and
// an account with no profile row is perfectly meaningful (it just hasn't been
// filled in).
//
// **Follows** are the one table that is two things at once, and the schema
// below says why: it holds the live edge (who follows whom) *and* the log of
// every follow that ever happened, because following somebody is dated
// activity that belongs in the follower's feed, and unfollowing them later
// does not unhappen it.
//
// A **guest** is an account with no credentials behind it, so that somebody
// can start the course before deciding whether they want one. It is a row in
// all three tables like anybody else's — that is the point, since a lesson is
// generated from a learner's own FSRS state and there is nowhere else for
// that state to live — and the only thing `users.guest` changes is what may
// happen to it: it can be claimed by a registration (`claim_guest`), and it
// is swept away once its session is gone (`create_guest`). Everything that
// reads a learner reads a guest without knowing it is one.
//
// All access goes through a single connection behind a mutex. That serializes
// the database the way the old in-memory maps were serialized by their
// mutexes, and it lets a load-mutate-store (see `update_user`) hold the lock
// for the whole read-modify-write, so a submission can't lose an update to a
// concurrent one.

use std::collections::HashMap;
use std::sync::Mutex;

use rand::RngExt;
use ring::digest::{SHA256, digest};
use rusqlite::{Connection, OptionalExtension, params};

use crate::messages::{Message, Thread};
use crate::profile::{Activity, Follow, Profile, ProfileEdit};
use crate::user::User;

pub struct Store {
    conn: Mutex<Connection>,
}

// whether a create_user call actually created the user, or found the name taken
pub enum Created {
    Ok,
    Taken,
}

#[derive(Clone)]
pub struct SessionIdentity {
    pub username: String,
    // None for a guest, who has no credentials and so no address either
    pub email: Option<String>,
    pub guest: bool,
}

impl Store {
    // open (creating if necessary) the database at `path` and make sure its
    // schema exists
    pub fn open(path: &str) -> rusqlite::Result<Store> {
        Self::init(Connection::open(path)?)
    }

    // an in-memory database, for tests. It lives exactly as long as its one
    // connection, which is fine: the connection lives as long as the Store.
    #[cfg(test)]
    pub fn memory() -> rusqlite::Result<Store> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> rusqlite::Result<Store> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS users (
                 username TEXT PRIMARY KEY,
                 courses  TEXT    NOT NULL,
                 guest    INTEGER NOT NULL DEFAULT 0
             );
             CREATE TABLE IF NOT EXISTS profiles (
                 username    TEXT PRIMARY KEY,
                 display     TEXT    NOT NULL,
                 title       TEXT,
                 bio         TEXT    NOT NULL,
                 cefr        TEXT    NOT NULL,
                 avatar      TEXT,
                 course_id   TEXT,
                 joined      INTEGER NOT NULL
             );
             -- Who follows whom, and **every follow that has ever happened**:
             -- a row is written when somebody is first followed and is never
             -- deleted, `following` going to 0 on an unfollow instead. The
             -- table is therefore two things at once, on purpose — the live
             -- edge (`following = 1`, which is what the counts and the button
             -- read) and the log (every row, which is what the follower's
             -- activity feed reads). Unfollowing somebody is not a claim that
             -- you never followed them, so the feed entry stays; and because
             -- `day` is only set on the first insert, a follow/unfollow/
             -- re-follow leaves the one entry it should, on the day it
             -- actually happened.
             CREATE TABLE IF NOT EXISTS follows (
                 follower  TEXT    NOT NULL,
                 followee  TEXT    NOT NULL,
                 day       INTEGER NOT NULL,
                 following INTEGER NOT NULL,
                 PRIMARY KEY (follower, followee)
             );
             -- the primary key answers 'who does this user follow'; a
             -- follower count asks the transposed question
             CREATE INDEX IF NOT EXISTS follows_by_followee ON follows (followee);
             CREATE TABLE IF NOT EXISTS activity (
                 username  TEXT    NOT NULL,
                 course_id TEXT    NOT NULL,
                 day       INTEGER NOT NULL,
                 data      TEXT    NOT NULL,
                 PRIMARY KEY (username, course_id, day)
             );
             -- the primary key answers 'this user's history', which is what a
             -- profile asks; the leaderboard asks the transposed question
             -- ('everyone's last seven days') and would otherwise scan the
             -- whole table for it
             CREATE INDEX IF NOT EXISTS activity_by_day ON activity (day);
             CREATE TABLE IF NOT EXISTS sessions (
                 token_hash TEXT PRIMARY KEY,
                 username   TEXT    NOT NULL,
                 email      TEXT,
                 expires_at INTEGER NOT NULL
             );
             -- Private messages, one row each. `thread` is the pair of
             -- usernames sorted and joined (see `thread_key`), which is what
             -- makes a conversation a thing that can be looked up: the two
             -- people in it are the whole of its identity, so there is no
             -- thread record to create before the first message and none to
             -- clean up after the last.
             --
             -- `id` is the only ordering. A row's `sent_at` is for display,
             -- and two messages in the same second still have an order —
             -- which is also what 'how far have I read' is counted in.
             CREATE TABLE IF NOT EXISTS messages (
                 id        INTEGER PRIMARY KEY AUTOINCREMENT,
                 thread    TEXT    NOT NULL,
                 sender    TEXT    NOT NULL,
                 recipient TEXT    NOT NULL,
                 body      TEXT    NOT NULL,
                 sent_at   INTEGER NOT NULL
             );
             -- opening one conversation reads (thread, id); the inbox list
             -- asks the other question, 'every thread this person is in',
             -- which is either end of a row
             CREATE INDEX IF NOT EXISTS messages_by_thread ON messages (thread, id);
             CREATE INDEX IF NOT EXISTS messages_by_sender ON messages (sender, id);
             CREATE INDEX IF NOT EXISTS messages_by_recipient ON messages (recipient, id);
             -- How far each person has read each of their threads: the id of
             -- the newest message they have had on screen. One row per side
             -- of a conversation, not one per message — 'unread' is a
             -- comparison against the newest id, so a thread with a thousand
             -- messages in it still costs one number per reader.
             CREATE TABLE IF NOT EXISTS reads (
                 reader    TEXT    NOT NULL,
                 thread    TEXT    NOT NULL,
                 last_read INTEGER NOT NULL,
                 PRIMARY KEY (reader, thread)
             );",
        )?;
        Ok(Store {
            conn: Mutex::new(conn),
        })
    }

    // Create a fresh user and the blank profile that goes with them, unless
    // the name is already taken.
    pub fn create_user(&self, username: &str, joined: u64) -> rusqlite::Result<Created> {
        let conn = self.conn.lock().unwrap();
        if !insert_account(&conn, username, false, &Profile::new(username, joined))? {
            return Ok(Created::Taken);
        }
        Ok(Created::Ok)
    }

    // Open an account for somebody who hasn't got one yet, and hand back the
    // name it was given along with a session for it. The name is prefixed
    // with `guest~`, and the `~` is load-bearing: mimi_auth's `validUsername`
    // allows only letters, digits and `._-`, so no credentialed account can
    // ever be given a name in this shape — which is what makes `claim_guest`
    // below safe to write as a rename.
    //
    // The sweep, the account and the session are one method because they are
    // one step: purging between another request's create and its session
    // insert would delete a guest that is about to be handed out.
    pub fn create_guest(
        &self,
        expires_at: u64,
        timestamp: u64,
    ) -> rusqlite::Result<(String, String)> {
        let conn = self.conn.lock().unwrap();
        expire_sessions(&conn, timestamp)?;
        // A guest is only ever reachable through the cookie they were handed,
        // so once no session names one the record can never be read again.
        // Sweeping here rather than on a timer keeps the cost on the path
        // that creates the mess, and needs no background task.
        let abandoned: Vec<String> = conn
            .prepare(
                "SELECT username FROM users
                 WHERE guest = 1
                   AND username NOT IN (SELECT username FROM sessions)",
            )?
            .query_map([], |row| row.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        for username in &abandoned {
            delete_account(&conn, username)?;
        }

        // 64 bits of name: a collision would be handed somebody else's
        // progress, and the loop costs nothing against a chance that small.
        let username = loop {
            let candidate = format!("guest~{:016x}", rand::rng().random::<u64>());
            let mut profile = Profile::new(&candidate, timestamp);
            // the `guest~…` name is plumbing; "Guest" is what a person reads
            "Guest".clone_into(&mut profile.display);
            if insert_account(&conn, &candidate, true, &profile)? {
                break candidate;
            }
        };
        let token = insert_session(&conn, &username, None, expires_at)?;
        Ok((username, token))
    }

    // Move a guest's whole record onto the name they have just registered.
    // Claiming is a rename rather than a copy because it is the same learner:
    // their words, their place in the tree and every day of their record
    // carry over, which is the whole promise of "save your progress".
    //
    // Anything already under `username` is deleted first. mimi_auth has just
    // minted the name, so a learning record under it can only be a leftover
    // from a credential database that has since been thrown away — and the
    // live guest in front of us is certainly not that.
    pub fn claim_guest(&self, guest: &str, username: &str, joined: u64) -> rusqlite::Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let transaction = conn.transaction()?;
        delete_account(&transaction, username)?;
        transaction.execute(
            "UPDATE users SET username = ?1, guest = 0 WHERE username = ?2",
            params![username, guest],
        )?;
        transaction.execute(
            "UPDATE profiles SET username = ?1 WHERE username = ?2",
            params![username, guest],
        )?;
        // the display name was "Guest"; the account has a name of its own now
        transaction.execute(
            "UPDATE profiles SET display = ?1 WHERE username = ?1",
            [username],
        )?;
        transaction.execute(
            "UPDATE activity SET username = ?1 WHERE username = ?2",
            params![username, guest],
        )?;
        // the caller issues a fresh session for the claimed account; the
        // guest's own cookies stop working the moment this commits
        transaction.execute("DELETE FROM sessions WHERE username = ?1", [guest])?;
        // an account that predates profile rows leaves nothing to rename
        if !transaction.query_row(
            "SELECT EXISTS (SELECT 1 FROM profiles WHERE username = ?1)",
            [username],
            |row| row.get::<_, bool>(0),
        )? {
            save_profile(&transaction, username, &Profile::new(username, joined))?;
        }
        transaction.commit()
    }

    // Throw an account away: the learning record, the profile, the activity
    // and any live session. Only guests are ever discarded this way — they
    // are the accounts a learner can walk away from (see the callers in
    // server.rs) — but nothing here depends on that.
    pub fn delete_account(&self, username: &str) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        delete_account(&conn, username)
    }

    // This learner's state in one course, or None if there is no such
    // account. A course they have never opened is a real, empty state.
    pub fn load_user(&self, username: &str, course_id: &str) -> rusqlite::Result<Option<User>> {
        let conn = self.conn.lock().unwrap();
        load_user(&conn, username, course_id)
    }

    // The cookie contains 256 random bits; only its SHA-256 digest is kept in
    // SQLite, so a read-only database leak does not immediately become a set
    // of live browser sessions.
    pub fn create_session(
        &self,
        username: &str,
        email: &str,
        expires_at: u64,
        timestamp: u64,
    ) -> rusqlite::Result<String> {
        let conn = self.conn.lock().unwrap();
        expire_sessions(&conn, timestamp)?;
        insert_session(&conn, username, Some(email), expires_at)
    }

    // Whether the session names a guest is read from the account rather than
    // copied into the session row: an account is claimed while its owner is
    // signed in, and a flag written here would go on saying "guest" after
    // they had stopped being one.
    pub fn load_session(
        &self,
        token: &str,
        timestamp: u64,
    ) -> rusqlite::Result<Option<SessionIdentity>> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT sessions.username, sessions.email, COALESCE(users.guest, 0)
             FROM sessions LEFT JOIN users ON users.username = sessions.username
             WHERE sessions.token_hash = ?1 AND sessions.expires_at > ?2",
            params![token_hash(token), timestamp as i64],
            |row| {
                Ok(SessionIdentity {
                    username: row.get(0)?,
                    email: row.get(1)?,
                    guest: row.get(2)?,
                })
            },
        )
        .optional()
    }

    pub fn delete_session(&self, token: &str) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM sessions WHERE token_hash = ?1",
            [token_hash(token)],
        )?;
        Ok(())
    }

    // An address lives in two places: mimi_auth's row, which is the record,
    // and the session rows this server answers `/auth/me` from. When the
    // first changes the second has to follow, or a signed-in browser goes on
    // showing the old address until its cookie expires. Every live session
    // for the account is rewritten, not just the one that asked — the same
    // person's other devices never see the reply that carried the new one.
    pub fn update_session_email(&self, username: &str, email: &str) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE sessions SET email = ?2 WHERE username = ?1",
            params![username, email],
        )?;
        Ok(())
    }

    // Everything signed in as this account except the caller's own session.
    // A password is changed either to tidy up or because somebody else may
    // know the old one, and the second reading is the one worth designing
    // for: mimi_auth can retire a password but has no way to reach the
    // cookies its consumers hold, so ending them is this server's job. The
    // browser that made the change keeps its session, because signing
    // somebody out of the page they are on to tell them it worked is a
    // strange way to answer.
    pub fn delete_other_sessions(&self, username: &str, keep: &str) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM sessions WHERE username = ?1 AND token_hash != ?2",
            params![username, token_hash(keep)],
        )?;
        Ok(())
    }

    // Load one course's user state, hand it to `f` to mutate, store the
    // result, and fold the activity `f` reports into that course's row for
    // `day` — all under one
    // lock, so the whole read-modify-write is atomic. Returns None (touching
    // nothing) if there's no such user, otherwise whatever `f` returned.
    //
    // The activity travels with the mutation rather than being recorded
    // afterwards for the same reason the load and the store are one step: a
    // lesson that moved the user's memory but left no trace in the record
    // would break their streak and flatten their graph. The delta has already
    // been submitted by then, so there is no second chance to reconstruct it.
    pub fn update_user<T>(
        &self,
        username: &str,
        course_id: &str,
        day: u32,
        f: impl FnOnce(&mut User) -> (T, Activity),
    ) -> rusqlite::Result<Option<T>> {
        let conn = self.conn.lock().unwrap();
        let Some(mut user) = load_user(&conn, username, course_id)? else {
            return Ok(None);
        };
        let (result, activity) = f(&mut user);
        save_user(&conn, username, course_id, &user)?;
        if !activity.is_empty() {
            // a course-day the user has already been active on has a row to
            // grow; otherwise this is their first lesson there today
            let mut today = load_activity_on(&conn, username, course_id, day)?.unwrap_or_default();
            today.absorb(activity);
            save_activity(&conn, username, course_id, day, &today)?;
        }
        Ok(Some(result))
    }

    // --- profiles ---

    // what the user has written about themselves, or None if they have never
    // been given a profile row (an account that predates profiles, or one
    // that was seeded straight into the users table)
    pub fn load_profile(&self, username: &str) -> rusqlite::Result<Option<Profile>> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT display, title, bio, cefr, avatar, course_id, joined
             FROM profiles WHERE username = ?1",
            [username],
            |row| {
                Ok(Profile {
                    display: row.get(0)?,
                    title: row.get(1)?,
                    bio: row.get(2)?,
                    cefr: row.get(3)?,
                    avatar: row.get(4)?,
                    course_id: row.get(5)?,
                    joined: row.get::<_, i64>(6)? as u64,
                })
            },
        )
        .optional()
    }

    // Store what the owner has written about themselves. `edit` has already
    // been checked (that is what having the type means — see
    // `ProfileEdit::of`), so nothing is validated here.
    //
    // The columns are named rather than the row replaced, because two of the
    // profile's fields are not the editor's to write: `title` is granted, and
    // `course_id` belongs to the writer below.
    pub fn save_profile_edit(
        &self,
        username: &str,
        edit: ProfileEdit,
        joined: u64,
    ) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        let rows = conn.execute(
            "UPDATE profiles SET display = ?1, bio = ?2, cefr = ?3, avatar = ?4
             WHERE username = ?5",
            params![edit.display, edit.bio, edit.cefr, edit.avatar, username],
        )?;
        if rows == 0 {
            // an account that predates profile rows: open one with the edit
            // already applied (the handler has checked the user exists)
            let mut profile = Profile::new(username, joined);
            profile.apply(edit);
            save_profile(&conn, username, &profile)?;
        }
        Ok(())
    }

    // The active course gets a writer of its own rather than going through
    // the edit above: the app can't function without the user setting it (it
    // *is* the account's course choice, not a public claim like a bio), it is
    // set from the course chooser rather than the profile editor, and the
    // worst a vandal can do with it is change what that user sees next visit.
    pub fn save_course(
        &self,
        username: &str,
        course_id: &str,
        joined: u64,
    ) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        let rows = conn.execute(
            "UPDATE profiles SET course_id = ?1 WHERE username = ?2",
            params![course_id, username],
        )?;
        if rows == 0 {
            // an account that predates profile rows: open one with the
            // choice already made (the handler has checked the user exists)
            let mut profile = Profile::new(username, joined);
            profile.course_id = Some(course_id.to_string());
            save_profile(&conn, username, &profile)?;
        }
        Ok(())
    }

    // --- following ---

    // Whether this name belongs to an account, and whether that account is a
    // guest. Both answers come from one row because the callers need both:
    // following somebody requires them to exist *and* to be somebody, and a
    // guest is a record with a week to live and no name behind it.
    pub fn account_is_guest(&self, username: &str) -> rusqlite::Result<Option<bool>> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT guest FROM users WHERE username = ?1",
            [username],
            |row| row.get(0),
        )
        .optional()
    }

    // Follow somebody, dated `day`. A first follow writes the row that the
    // follower's activity feed will quote forever; a re-follow only turns the
    // live edge back on, deliberately leaving `day` where it was — the entry
    // in the feed belongs to the day it happened, and toggling a button is
    // not a new event to report.
    pub fn follow(&self, follower: &str, followee: &str, day: u32) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO follows (follower, followee, day, following) VALUES (?1, ?2, ?3, 1)
             ON CONFLICT (follower, followee) DO UPDATE SET following = 1",
            params![follower, followee, day],
        )?;
        Ok(())
    }

    // Stop following somebody. The row stays: see the schema — this table is
    // the log as well as the edge, and the feed reads the log.
    pub fn unfollow(&self, follower: &str, followee: &str) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE follows SET following = 0 WHERE follower = ?1 AND followee = ?2",
            params![follower, followee],
        )?;
        Ok(())
    }

    pub fn follows(&self, follower: &str, followee: &str) -> rusqlite::Result<bool> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT EXISTS (SELECT 1 FROM follows
                            WHERE follower = ?1 AND followee = ?2 AND following = 1)",
            params![follower, followee],
            |row| row.get(0),
        )
    }

    // how many people follow this user, and how many they follow — the live
    // edge only, which is the one thing about this table that an unfollow
    // does change
    pub fn follow_counts(&self, username: &str) -> rusqlite::Result<(u32, u32)> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT (SELECT COUNT(*) FROM follows WHERE followee = ?1 AND following = 1),
                    (SELECT COUNT(*) FROM follows WHERE follower = ?1 AND following = 1)",
            [username],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
    }

    // Every follow this user has ever made, for their activity feed —
    // including the ones they have since undone, because the feed is a record
    // of what they did.
    //
    // The followee's display name is read *now* rather than remembered from
    // the day of the follow, so a person who renames themselves is not quoted
    // under a name they have stopped using. The join is a LEFT one for the
    // same reason as the leaderboard's: an account with no profile row is
    // still a person, and their username is the name they have.
    pub fn follow_log(&self, follower: &str) -> rusqlite::Result<Vec<Follow>> {
        let conn = self.conn.lock().unwrap();
        let mut statement = conn.prepare(
            "SELECT follows.day,
                    follows.followee,
                    COALESCE(profiles.display, follows.followee)
             FROM follows
             LEFT JOIN profiles ON profiles.username = follows.followee
             WHERE follows.follower = ?1
             ORDER BY follows.day",
        )?;
        let rows = statement.query_map([follower], |row| {
            Ok(Follow {
                day: row.get::<_, i64>(0)? as u32,
                username: row.get(1)?,
                display: row.get(2)?,
            })
        })?;
        rows.collect()
    }

    // --- messages ---

    // Who this name belongs to, as far as a conversation is concerned:
    // whether the account is a guest, and what they call themselves. Both
    // answers come from one row because every message needs both — a guest
    // cannot be written to, and the person who is written to has a name that
    // goes at the top of the thread.
    pub fn correspondent(&self, username: &str) -> rusqlite::Result<Option<(bool, String)>> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT users.guest, COALESCE(profiles.display, users.username)
             FROM users LEFT JOIN profiles ON profiles.username = users.username
             WHERE users.username = ?1",
            [username],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
    }

    // Everybody this user is in a conversation with, newest first: one entry
    // per thread, carrying the last thing said in it and whether they have
    // seen it. This is the inbox list, and it is the whole of it — the same
    // reasoning as the leaderboard's, that a cut-off nobody has measured is a
    // number invented rather than chosen.
    //
    // The joins are what make a row readable on its own: `other` is whichever
    // end of the message isn't the reader, the display name is read *now*
    // rather than remembered (so somebody who renames themselves is not
    // quoted under a name they have dropped), and `unread` compares the
    // newest id against how far they have read. A message the reader sent is
    // never unread to them, which is why the sender is checked as well.
    pub fn load_threads(&self, username: &str) -> rusqlite::Result<Vec<Thread>> {
        let conn = self.conn.lock().unwrap();
        let mut statement = conn.prepare(
            "SELECT CASE WHEN messages.sender = ?1 THEN messages.recipient ELSE messages.sender END,
                    COALESCE(profiles.display,
                             CASE WHEN messages.sender = ?1
                                  THEN messages.recipient ELSE messages.sender END),
                    messages.sender,
                    messages.body,
                    messages.sent_at,
                    (messages.id > COALESCE(reads.last_read, 0) AND messages.sender <> ?1)
             FROM messages
             JOIN (SELECT thread, MAX(id) AS id FROM messages
                   WHERE sender = ?1 OR recipient = ?1
                   GROUP BY thread) newest ON newest.id = messages.id
             LEFT JOIN reads ON reads.reader = ?1 AND reads.thread = messages.thread
             LEFT JOIN profiles ON profiles.username =
                 CASE WHEN messages.sender = ?1 THEN messages.recipient ELSE messages.sender END
             ORDER BY messages.id DESC",
        )?;
        let rows = statement.query_map([username], |row| {
            Ok(Thread {
                with: row.get(0)?,
                display: row.get(1)?,
                last_sender: row.get(2)?,
                last: row.get(3)?,
                sent_at: row.get::<_, i64>(4)? as u64,
                unread: row.get(5)?,
            })
        })?;
        rows.collect()
    }

    // One conversation, oldest first, capped at its `limit` most recent
    // messages. The cap is why the query reads backwards and the result is
    // turned round afterwards: what a thread is opened at is its end.
    //
    // There is no way to ask for the messages before them. That is a real
    // limit rather than an oversight — paging is a second protocol (a cursor,
    // a request for it, a client that knows when it is at the top) and this
    // one is worth having first.
    pub fn load_thread(
        &self,
        username: &str,
        other: &str,
        limit: u32,
    ) -> rusqlite::Result<Vec<Message>> {
        let conn = self.conn.lock().unwrap();
        let mut statement = conn.prepare(
            "SELECT id, sender, body, sent_at FROM messages
             WHERE thread = ?1 ORDER BY id DESC LIMIT ?2",
        )?;
        let rows = statement.query_map(params![thread_key(username, other), limit], |row| {
            Ok(Message {
                id: row.get(0)?,
                from: row.get(1)?,
                body: row.get(2)?,
                sent_at: row.get::<_, i64>(3)? as u64,
            })
        })?;
        let mut messages: Vec<Message> = rows.collect::<rusqlite::Result<_>>()?;
        messages.reverse();
        Ok(messages)
    }

    // Say something, and hand back the stored row. The id it was given is the
    // message's place in its thread and the thing 'read up to here' is
    // counted in, so the caller gets the whole record rather than the pieces
    // it passed in — what goes out over the event feeds is what went into the
    // database.
    pub fn send_message(
        &self,
        from: &str,
        to: &str,
        body: &str,
        timestamp: u64,
    ) -> rusqlite::Result<Message> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO messages (thread, sender, recipient, body, sent_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![thread_key(from, to), from, to, body, timestamp as i64],
        )?;
        Ok(Message {
            id: conn.last_insert_rowid(),
            from: from.to_string(),
            body: body.to_string(),
            sent_at: timestamp,
        })
    }

    // Mark this thread read as far as its newest message. `MAX` rather than
    // an assignment because two of somebody's tabs can report this in either
    // order, and a marker that could move backwards would put the unread dot
    // back on a conversation they are looking at.
    pub fn mark_read(&self, reader: &str, other: &str) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        let thread = thread_key(reader, other);
        conn.execute(
            "INSERT INTO reads (reader, thread, last_read)
             SELECT ?1, ?2, COALESCE(MAX(id), 0) FROM messages WHERE thread = ?2
             ON CONFLICT (reader, thread) DO UPDATE SET
                 last_read = MAX(reads.last_read, excluded.last_read)",
            params![reader, thread],
        )?;
        Ok(())
    }

    // Accounts whose username or display name begins with `prefix`, for the
    // inbox's 'start a new conversation' box. Guests are left out for the
    // same reason they are left off the board and cannot be followed: there
    // is nobody there to write to, and the record goes when its cookie does.
    pub fn search_accounts(
        &self,
        prefix: &str,
        limit: u32,
    ) -> rusqlite::Result<Vec<(String, String)>> {
        let conn = self.conn.lock().unwrap();
        let mut statement = conn.prepare(
            "SELECT users.username, COALESCE(profiles.display, users.username)
             FROM users LEFT JOIN profiles ON profiles.username = users.username
             WHERE users.guest = 0
               AND (users.username LIKE ?1 ESCAPE '\\' OR profiles.display LIKE ?1 ESCAPE '\\')
             ORDER BY users.username LIMIT ?2",
        )?;
        let rows = statement
            .query_map(params![format!("{}%", like_prefix(prefix)), limit], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })?;
        rows.collect()
    }

    // --- activity ---

    // Everything the user has ever done, one entry per course-day, oldest
    // first. The whole record, because the whole record is what a
    // profile is: the streak, the totals and the score graph are each a scan
    // over all of it, and a user who studies daily for five years still has
    // fewer rows than a week of per-lesson logging would give.
    pub fn load_activity(&self, username: &str) -> rusqlite::Result<Vec<(String, u32, Activity)>> {
        let conn = self.conn.lock().unwrap();
        let mut statement = conn.prepare(
            "SELECT course_id, day, data FROM activity WHERE username = ?1 ORDER BY day",
        )?;
        let rows = statement.query_map([username], |row| {
            let course_id: String = row.get(0)?;
            let day: i64 = row.get(1)?;
            let data: String = row.get(2)?;
            Ok((
                course_id,
                day as u32,
                serde_json::from_str(&data).map_err(from_json(2))?,
            ))
        })?;
        rows.collect()
    }

    // One user's one course on one UTC day, for features whose window is today.
    // Daily quests should not scan a five-year history to find the row that
    // is already keyed exactly this way, and no row is the ordinary meaning
    // of "nothing done yet", not an error.
    pub fn load_activity_on(
        &self,
        username: &str,
        course_id: &str,
        day: u32,
    ) -> rusqlite::Result<Option<Activity>> {
        let conn = self.conn.lock().unwrap();
        load_activity_on(&conn, username, course_id, day)
    }

    // Everyone's activity from `from_day` onwards, as `(username, display,
    // activity)` — the raw material of the weekly board (see leaderboard.rs),
    // which is a sum over one week of these rather than anything stored.
    //
    // The rows arrive unaggregated, several per user, because the XP a day is
    // worth lives inside the blob: `Activity::xp` knows that a perfect lesson
    // pays double, and SQL cannot see into the JSON to apply that. Summing in
    // Rust keeps one XP schedule in the codebase instead of two.
    //
    // **The join against `users` is what keeps guests off the board.** It also
    // drops activity belonging to no account at all, which is the right
    // reading of an orphaned row: `delete_account` clears both, so anything
    // left behind is wreckage rather than a learner.
    //
    // The profile join is a LEFT one because a profile row is optional (an
    // account seeded straight into `users` has none), and a learner with
    // nothing written about them is still ranked — under their username,
    // which is the only name they have.
    pub fn load_activity_since(
        &self,
        from_day: u32,
    ) -> rusqlite::Result<Vec<(String, String, Activity)>> {
        let conn = self.conn.lock().unwrap();
        let mut statement = conn.prepare(
            "SELECT activity.username,
                    COALESCE(profiles.display, activity.username),
                    activity.data
             FROM activity
             JOIN users ON users.username = activity.username
             LEFT JOIN profiles ON profiles.username = activity.username
             WHERE activity.day >= ?1 AND users.guest = 0",
        )?;
        let rows = statement.query_map([from_day], |row| {
            let username: String = row.get(0)?;
            let display: String = row.get(1)?;
            let data: String = row.get(2)?;
            Ok((
                username,
                display,
                serde_json::from_str(&data).map_err(from_json(2))?,
            ))
        })?;
        rows.collect()
    }
}

// Open a fresh account and the profile that goes with it, unless the name is
// already taken; true if it was created. The unique primary key does the
// checking, so a concurrent create can't slip through between a look-up and
// an insert — and the profile only goes in if the account did, so losing the
// race can't overwrite the winner's profile.
fn insert_account(
    conn: &Connection,
    username: &str,
    guest: bool,
    profile: &Profile,
) -> rusqlite::Result<bool> {
    let rows = conn.execute(
        "INSERT OR IGNORE INTO users (username, courses, guest)
         VALUES (?1, ?2, ?3)",
        params![username, json(&HashMap::<String, User>::new()), guest],
    )?;
    if rows != 1 {
        return Ok(false);
    }
    save_profile(conn, username, profile)?;
    Ok(true)
}

fn delete_account(conn: &Connection, username: &str) -> rusqlite::Result<()> {
    for table in ["sessions", "activity", "profiles", "users"] {
        conn.execute(
            &format!("DELETE FROM {table} WHERE username = ?1"),
            [username],
        )?;
    }
    // Follows name a user at both ends, so this one is not keyed by
    // `username` like the rest. A record that is gone follows nobody and is
    // followed by nobody — and leaving the log behind would have somebody
    // else's feed quoting an account that no longer exists.
    conn.execute(
        "DELETE FROM follows WHERE follower = ?1 OR followee = ?1",
        [username],
    )?;
    // Messages name a user at both ends too, and the same reasoning applies:
    // a conversation with an account that is gone is half a conversation.
    // Only guests are ever deleted and a guest can neither send nor receive,
    // so this is defensive rather than load-bearing — but it is the rule for
    // the table, and a table that is cleaned up by accident is one that will
    // stop being.
    conn.execute(
        "DELETE FROM messages WHERE sender = ?1 OR recipient = ?1",
        [username],
    )?;
    conn.execute("DELETE FROM reads WHERE reader = ?1", [username])?;
    Ok(())
}

// A conversation's identity is the pair of people in it, so its key is their
// two names in a fixed order: whoever writes first, both of them address the
// same thread. `\n` is the separator because no username can contain one —
// mimi_auth allows letters, digits and `._-`, and a guest is `guest~<hex>`.
fn thread_key(one: &str, other: &str) -> String {
    if one <= other {
        format!("{one}\n{other}")
    } else {
        format!("{other}\n{one}")
    }
}

// What somebody typed into the search box, as a LIKE pattern that matches it
// literally: `%` and `_` are wildcards in a pattern and ordinary characters
// in a name, and a search for `_` should not return everybody.
fn like_prefix(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

// The cookie contains 256 random bits; only its SHA-256 digest is kept in
// SQLite, so a read-only database leak does not immediately become a set of
// live browser sessions.
fn insert_session(
    conn: &Connection,
    username: &str,
    email: Option<&str>,
    expires_at: u64,
) -> rusqlite::Result<String> {
    let mut bytes = [0u8; 32];
    rand::rng().fill(&mut bytes);
    let token = hex(&bytes);
    conn.execute(
        "INSERT INTO sessions (token_hash, username, email, expires_at)
         VALUES (?1, ?2, ?3, ?4)",
        params![token_hash(&token), username, email, expires_at as i64],
    )?;
    Ok(token)
}

fn expire_sessions(conn: &Connection, timestamp: u64) -> rusqlite::Result<()> {
    conn.execute(
        "DELETE FROM sessions WHERE expires_at <= ?1",
        [timestamp as i64],
    )?;
    Ok(())
}

// Read one course out of an account's course map. Course state is nested
// under its stable id so identically named skills and words in two courses
// can never see or mutate one another.
fn load_user(conn: &Connection, username: &str, course_id: &str) -> rusqlite::Result<Option<User>> {
    conn.query_row(
        "SELECT courses FROM users WHERE username = ?1",
        [username],
        |row| {
            let stored: String = row.get(0)?;
            let courses: HashMap<String, User> =
                serde_json::from_str(&stored).map_err(from_json(0))?;
            Ok(courses.get(course_id).cloned().unwrap_or_default())
        },
    )
    .optional()
}

// Write a user's whole row.
//
// Progress is a blob rather than columns because it *is* a map: in a
// branching tree a learner is not at a coordinate, they are some way into
// each of several skills, and there is no query that wants one of those on
// its own. An upsert so this both updates an existing user and creates a
// seeded one — and it names the columns it is updating rather than replacing
// the row, because `guest` is not the learning state's to write: a lesson
// submitted by a guest must not quietly turn them into somebody else.
fn save_user(
    conn: &Connection,
    username: &str,
    course_id: &str,
    user: &User,
) -> rusqlite::Result<()> {
    let stored: Option<String> = conn
        .query_row(
            "SELECT courses FROM users WHERE username = ?1",
            [username],
            |row| row.get(0),
        )
        .optional()?;
    let mut courses: HashMap<String, User> = match stored {
        Some(stored) => serde_json::from_str(&stored).map_err(from_json(0))?,
        None => HashMap::new(),
    };
    courses.insert(course_id.to_string(), user.clone());
    conn.execute(
        "INSERT INTO users (username, courses) VALUES (?1, ?2)
         ON CONFLICT (username) DO UPDATE SET courses = excluded.courses",
        params![username, json(&courses)],
    )?;
    Ok(())
}

fn save_profile(conn: &Connection, username: &str, profile: &Profile) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO profiles
             (username, display, title, bio, cefr, avatar, course_id, joined)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            username,
            profile.display,
            profile.title,
            profile.bio,
            profile.cefr,
            profile.avatar,
            profile.course_id,
            profile.joined as i64,
        ],
    )?;
    Ok(())
}

// one day's row, or None if the user did nothing that day (there is no such
// thing as an empty row: see `Activity::is_empty`)
fn load_activity_on(
    conn: &Connection,
    username: &str,
    course_id: &str,
    day: u32,
) -> rusqlite::Result<Option<Activity>> {
    conn.query_row(
        "SELECT data FROM activity WHERE username = ?1 AND course_id = ?2 AND day = ?3",
        params![username, course_id, day],
        |row| {
            let data: String = row.get(0)?;
            serde_json::from_str(&data).map_err(from_json(0))
        },
    )
    .optional()
}

fn save_activity(
    conn: &Connection,
    username: &str,
    course_id: &str,
    day: u32,
    activity: &Activity,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO activity (username, course_id, day, data)
         VALUES (?1, ?2, ?3, ?4)",
        params![
            username,
            course_id,
            day,
            serde_json::to_string(activity).unwrap()
        ],
    )?;
    Ok(())
}

// both of a user's blobs are plain data keyed by a string, so this never fails
fn json<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_string(value).unwrap()
}

fn token_hash(token: &str) -> String {
    hex(digest(&SHA256, token.as_bytes()).as_ref())
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut encoded, "{byte:02x}").unwrap();
    }
    encoded
}

// a stored blob that won't parse is a corrupt database, not a normal outcome;
// surface it as a conversion failure on the column it came from
fn from_json(column: usize) -> impl Fn(serde_json::Error) -> rusqlite::Error {
    move |e| {
        rusqlite::Error::FromSqlConversionFailure(column, rusqlite::types::Type::Text, Box::new(e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::word::{Stage, WordState};

    const TEST_COURSE: &str = "spanish_for_english";

    #[test]
    fn a_users_tree_progress_and_word_state_round_trip() {
        let store = Store::memory().unwrap();
        assert!(matches!(store.create_user("sam", 1).unwrap(), Created::Ok));
        store
            .update_user("sam", TEST_COURSE, 0, |user| {
                user.progress.insert("greetings".into(), 2);
                user.castles = 1;
                user.words
                    .insert("hola".into(), WordState::new(Stage::Scaffolding));
                ((), Activity::default())
            })
            .unwrap();
        let user = store.load_user("sam", TEST_COURSE).unwrap().unwrap();
        assert_eq!(user.progress["greetings"], 2);
        assert_eq!(user.castles, 1);
        assert_eq!(user.words["hola"].stage, Stage::Scaffolding);
    }

    #[test]
    fn courses_with_the_same_local_ids_keep_separate_learning_state() {
        let store = Store::memory().unwrap();
        store.create_user("sam", 1).unwrap();
        for (course, lessons) in [(TEST_COURSE, 3), ("french_for_english", 9)] {
            store
                .update_user("sam", course, 1, |user| {
                    user.progress.insert("basics".into(), lessons);
                    (
                        (),
                        Activity {
                            lessons: lessons as u32,
                            ..Activity::default()
                        },
                    )
                })
                .unwrap();
        }

        assert_eq!(
            store
                .load_user("sam", TEST_COURSE)
                .unwrap()
                .unwrap()
                .progress["basics"],
            3
        );
        assert_eq!(
            store
                .load_user("sam", "french_for_english")
                .unwrap()
                .unwrap()
                .progress["basics"],
            9
        );
        assert_eq!(
            store
                .load_activity_on("sam", TEST_COURSE, 1)
                .unwrap()
                .unwrap()
                .lessons,
            3
        );
        assert_eq!(
            store
                .load_activity_on("sam", "french_for_english", 1)
                .unwrap()
                .unwrap()
                .lessons,
            9
        );
    }

    #[test]
    fn the_active_course_round_trips_without_touching_the_rest() {
        let store = Store::memory().unwrap();
        assert!(matches!(store.create_user("sam", 1).unwrap(), Created::Ok));
        // a fresh account hasn't picked one yet
        assert_eq!(store.load_profile("sam").unwrap().unwrap().course_id, None);
        store.save_course("sam", TEST_COURSE, 2).unwrap();
        let profile = store.load_profile("sam").unwrap().unwrap();
        assert_eq!(profile.course_id.as_deref(), Some(TEST_COURSE));
        // the rest of the authored profile is what create_user opened
        assert_eq!(profile.display, "sam");
        assert_eq!(profile.joined, 1);
        // and a change of mind overwrites rather than stacking
        store.save_course("sam", "french_for_english", 3).unwrap();
        assert_eq!(
            store
                .load_profile("sam")
                .unwrap()
                .unwrap()
                .course_id
                .as_deref(),
            Some("french_for_english")
        );
    }

    #[test]
    fn an_edit_writes_the_authored_fields_and_leaves_the_rest_alone() {
        let store = Store::memory().unwrap();
        store.create_user("sam", 1).unwrap();
        store.save_course("sam", TEST_COURSE, 1).unwrap();

        store
            .save_profile_edit(
                "sam",
                ProfileEdit::of(
                    "Sam",
                    "learning Spanish",
                    "B1",
                    Some("https://x.example/a.png"),
                )
                .unwrap(),
                1,
            )
            .unwrap();

        let profile = store.load_profile("sam").unwrap().unwrap();
        assert_eq!(profile.display, "Sam");
        assert_eq!(profile.bio, "learning Spanish");
        assert_eq!(profile.cefr, "B1");
        assert_eq!(profile.avatar.as_deref(), Some("https://x.example/a.png"));
        // the course choice and the join date are not the editor's to write
        assert_eq!(profile.course_id.as_deref(), Some(TEST_COURSE));
        assert_eq!(profile.joined, 1);

        // and clearing the picture is an ordinary edit, not a missing one
        store
            .save_profile_edit("sam", ProfileEdit::of("Sam", "", "", None).unwrap(), 1)
            .unwrap();
        assert_eq!(store.load_profile("sam").unwrap().unwrap().avatar, None);
    }

    // The table is the live edge and the log at once: unfollowing changes who
    // you follow, and changes nothing about what you did.
    #[test]
    fn unfollowing_ends_the_follow_but_not_the_record_of_it() {
        let store = Store::memory().unwrap();
        store.create_user("sam", 100).unwrap();
        store.create_user("ren", 100).unwrap();

        store.follow("sam", "ren", 20).unwrap();
        assert!(store.follows("sam", "ren").unwrap());
        assert_eq!(store.follow_counts("ren").unwrap(), (1, 0));
        assert_eq!(store.follow_counts("sam").unwrap(), (0, 1));

        store.unfollow("sam", "ren").unwrap();
        assert!(!store.follows("sam", "ren").unwrap());
        assert_eq!(store.follow_counts("ren").unwrap(), (0, 0));
        assert_eq!(
            store.follow_log("sam").unwrap(),
            [Follow {
                day: 20,
                username: "ren".into(),
                display: "ren".into(),
            }]
        );

        // following again is the same follow resumed, not a second event on a
        // later day — the feed already says when it happened
        store.follow("sam", "ren", 40).unwrap();
        assert!(store.follows("sam", "ren").unwrap());
        assert_eq!(store.follow_log("sam").unwrap().len(), 1);
        assert_eq!(store.follow_log("sam").unwrap()[0].day, 20);
    }

    // The feed reads a follow's name when it serves it, so somebody who
    // renames themselves is not quoted under a name they have dropped.
    #[test]
    fn a_logged_follow_carries_the_name_its_target_uses_now() {
        let store = Store::memory().unwrap();
        store.create_user("sam", 100).unwrap();
        store.create_user("ren", 100).unwrap();
        store.follow("sam", "ren", 20).unwrap();
        store
            .save_profile_edit("ren", ProfileEdit::of("Ren", "", "", None).unwrap(), 100)
            .unwrap();

        assert_eq!(store.follow_log("sam").unwrap()[0].display, "Ren");
    }

    // A discarded guest leaves nobody's feed quoting an account that is gone.
    #[test]
    fn deleting_an_account_takes_its_follows_at_both_ends() {
        let store = Store::memory().unwrap();
        store.create_user("sam", 100).unwrap();
        store.create_user("ren", 100).unwrap();
        store.follow("sam", "ren", 20).unwrap();
        store.follow("ren", "sam", 20).unwrap();

        store.delete_account("ren").unwrap();

        assert!(store.follow_log("sam").unwrap().is_empty());
        assert_eq!(store.follow_counts("sam").unwrap(), (0, 0));
    }

    // The inbox list, which is derived from the messages rather than stored:
    // a conversation exists because something was said in it, it is named
    // after whoever isn't reading, and the most recent one is at the top.
    #[test]
    fn the_thread_list_is_one_row_per_conversation_newest_first() {
        let store = Store::memory().unwrap();
        for name in ["sam", "ana", "ren"] {
            store.create_user(name, 100).unwrap();
        }
        store
            .save_profile_edit("ana", ProfileEdit::of("Ana", "", "", None).unwrap(), 100)
            .unwrap();

        store.send_message("sam", "ana", "hola", 10).unwrap();
        store.send_message("ren", "sam", "hey", 20).unwrap();
        store.send_message("ana", "sam", "hola!", 30).unwrap();

        let threads = store.load_threads("sam").unwrap();
        // two conversations from three messages: the pair is the thread, so
        // a reply lands in the one that was already there
        assert_eq!(
            threads.iter().map(|t| t.with.as_str()).collect::<Vec<_>>(),
            ["ana", "ren"]
        );
        // named after the other person, under the name they use now
        assert_eq!(threads[0].display, "Ana");
        assert_eq!(threads[0].last, "hola!");
        assert_eq!(threads[0].last_sender, "ana");
        assert_eq!(threads[0].sent_at, 30);
        // ren's is older, and is still ren's own name
        assert_eq!(threads[1].display, "ren");
        assert_eq!(threads[1].last, "hey");

        // and the same messages read from the other end are one thread
        assert_eq!(store.load_threads("ana").unwrap().len(), 1);
    }

    // A thread is opened at its end, and that is also as far back as it can
    // be read: `limit` takes the newest, and they arrive oldest first because
    // that is the order they are shown in.
    #[test]
    fn a_thread_is_served_oldest_first_from_its_most_recent_messages() {
        let store = Store::memory().unwrap();
        store.create_user("sam", 100).unwrap();
        store.create_user("ana", 100).unwrap();
        for i in 1..=5 {
            store
                .send_message("sam", "ana", &format!("{i}"), 10 * i as u64)
                .unwrap();
        }

        let recent = store.load_thread("ana", "sam", 3).unwrap();
        assert_eq!(
            recent.iter().map(|m| m.body.as_str()).collect::<Vec<_>>(),
            ["3", "4", "5"]
        );
        // either way round names the same conversation
        assert_eq!(store.load_thread("sam", "ana", 100).unwrap().len(), 5);
    }

    // The search box behind "start a new conversation": a prefix of either
    // name, nobody who cannot be written to, and no wildcards smuggled in.
    #[test]
    fn account_search_matches_a_prefix_of_either_name_and_skips_guests() {
        let store = Store::memory().unwrap();
        for name in ["ana", "andres", "sam"] {
            store.create_user(name, 100).unwrap();
        }
        store
            .save_profile_edit("sam", ProfileEdit::of("Anaïs", "", "", None).unwrap(), 100)
            .unwrap();
        store.create_guest(1_000, 100).unwrap();

        let names = |prefix: &str| {
            store
                .search_accounts(prefix, 8)
                .unwrap()
                .into_iter()
                .map(|(username, _)| username)
                .collect::<Vec<_>>()
        };
        // the username, and the display name of somebody whose username
        // starts with something else entirely
        assert_eq!(names("an"), ["ana", "andres", "sam"]);
        assert_eq!(names("andr"), ["andres"]);
        // a guest is nobody to write to, so `guest~…` finds nothing
        assert!(names("guest").is_empty());
        // and LIKE's own wildcards are ordinary characters in a name
        assert!(names("%").is_empty());
        assert!(names("_").is_empty());
    }

    #[test]
    fn one_activity_day_can_be_loaded_without_scanning_the_history() {
        let store = Store::memory().unwrap();
        store.create_user("sam", 1).unwrap();
        study(&store, "sam", 20, 2);
        study(&store, "sam", 22, 3);

        assert_eq!(
            store
                .load_activity_on("sam", TEST_COURSE, 20)
                .unwrap()
                .unwrap()
                .lessons,
            2
        );
        assert!(
            store
                .load_activity_on("sam", TEST_COURSE, 21)
                .unwrap()
                .is_none()
        );
        assert_eq!(
            store
                .load_activity_on("sam", TEST_COURSE, 22)
                .unwrap()
                .unwrap()
                .lessons,
            3
        );
    }

    // A guest is an ordinary learner to everything except the flag, so the
    // things a lesson does to one must work: they load, they submit, and
    // submitting must not write over what they are.
    #[test]
    fn a_guest_is_an_ordinary_account_that_knows_it_is_one() {
        let store = Store::memory().unwrap();
        let (username, token) = store.create_guest(200, 100).unwrap();
        assert!(username.starts_with("guest~"));
        // the `~` is what keeps the name out of mimi_auth's reach
        assert!(!username.contains(|c: char| c.is_ascii_uppercase()));

        let identity = store.load_session(&token, 150).unwrap().unwrap();
        assert_eq!(identity.username, username);
        assert_eq!(identity.email, None);
        assert!(identity.guest);
        // and they read as a person, not as their plumbing
        assert_eq!(
            store.load_profile(&username).unwrap().unwrap().display,
            "Guest"
        );

        store
            .update_user(&username, TEST_COURSE, 0, |user| {
                user.words
                    .insert("hola".into(), WordState::new(Stage::Scaffolding));
                ((), Activity::default())
            })
            .unwrap();
        assert!(store.load_session(&token, 150).unwrap().unwrap().guest);
    }

    // The whole promise of "save your progress": what they did as a guest is
    // still theirs afterwards, under the name they chose.
    #[test]
    fn claiming_a_guest_carries_their_whole_record_onto_the_new_name() {
        let store = Store::memory().unwrap();
        let (guest, token) = store.create_guest(200, 100).unwrap();
        store
            .update_user(&guest, TEST_COURSE, 7, |user| {
                user.progress.insert("greetings".into(), 3);
                user.words
                    .insert("hola".into(), WordState::new(Stage::Recognition));
                (
                    (),
                    Activity {
                        lessons: 1,
                        ..Activity::default()
                    },
                )
            })
            .unwrap();

        store.claim_guest(&guest, "sam", 100).unwrap();

        let user = store.load_user("sam", TEST_COURSE).unwrap().unwrap();
        assert_eq!(user.progress["greetings"], 3);
        assert_eq!(user.words["hola"].stage, Stage::Recognition);
        assert_eq!(store.load_activity("sam").unwrap()[0].2.lessons, 1);
        // they are not a guest any more, and the guest is gone
        let profile = store.load_profile("sam").unwrap().unwrap();
        assert_eq!(profile.display, "sam");
        assert!(store.load_user(&guest, TEST_COURSE).unwrap().is_none());
        assert!(store.load_session(&token, 150).unwrap().is_none());
        let fresh = store
            .create_session("sam", "sam@example.com", 300, 200)
            .unwrap();
        assert!(!store.load_session(&fresh, 250).unwrap().unwrap().guest);
    }

    // A guest is only ever reachable through their cookie, so once no session
    // names one there is nothing left to protect — but a live guest and a
    // real account both have to survive the sweep.
    #[test]
    fn creating_a_guest_sweeps_up_the_guests_whose_sessions_have_gone() {
        let store = Store::memory().unwrap();
        let (abandoned, _) = store.create_guest(200, 100).unwrap();
        let (live, _) = store.create_guest(1_000, 100).unwrap();
        store.create_user("sam", 100).unwrap();
        store
            .create_session("sam", "sam@example.com", 150, 100)
            .unwrap();

        // past the abandoned guest's expiry, and past sam's session too
        store.create_guest(1_000, 300).unwrap();

        assert!(store.load_user(&abandoned, TEST_COURSE).unwrap().is_none());
        assert!(store.load_user(&live, TEST_COURSE).unwrap().is_some());
        // sam's session has expired, but an account with credentials behind
        // it can always be signed back into
        assert!(store.load_user("sam", TEST_COURSE).unwrap().is_some());
    }

    // record one day of study for `username`, which is all the leaderboard
    // reads them through
    fn study(store: &Store, username: &str, day: u32, lessons: u32) {
        store
            .update_user(username, TEST_COURSE, day, |_| {
                (
                    (),
                    Activity {
                        lessons,
                        ..Activity::default()
                    },
                )
            })
            .unwrap();
    }

    // The board ranks people, and a guest is nobody yet: an unnamed record
    // that vanishes with its cookie has no business holding a public placing.
    // Registering is what turns them into somebody — and because claiming is a
    // rename, the week they had already put in arrives with them.
    #[test]
    fn the_week_skips_guests_but_not_the_learners_they_turn_into() {
        let store = Store::memory().unwrap();
        store.create_user("sam", 100).unwrap();
        let (guest, _) = store.create_guest(1_000, 100).unwrap();
        study(&store, "sam", 20, 2);
        study(&store, &guest, 20, 3);

        let named: Vec<String> = store
            .load_activity_since(20)
            .unwrap()
            .into_iter()
            .map(|(username, _, _)| username)
            .collect();
        assert_eq!(named, ["sam"]);

        store.claim_guest(&guest, "ren", 100).unwrap();
        let mut named: Vec<(String, u32)> = store
            .load_activity_since(20)
            .unwrap()
            .into_iter()
            .map(|(username, _, activity)| (username, activity.lessons))
            .collect();
        named.sort();
        // the three lessons they did as a guest are on the board under `ren`
        assert_eq!(named, [("ren".to_string(), 3), ("sam".to_string(), 2)]);
    }

    // The board asks for a week, not a history: days before the Monday it
    // starts at are somebody else's business.
    #[test]
    fn the_week_starts_where_it_is_asked_to() {
        let store = Store::memory().unwrap();
        store.create_user("sam", 100).unwrap();
        study(&store, "sam", 17, 5); // last week
        study(&store, "sam", 20, 1);
        study(&store, "sam", 21, 2);

        let lessons: u32 = store
            .load_activity_since(18)
            .unwrap()
            .iter()
            .map(|(_, _, activity)| activity.lessons)
            .sum();
        assert_eq!(lessons, 3);
    }

    // A board shows what people call themselves, and falls back to the name
    // that identifies them when there is nothing else — an account seeded
    // straight into `users` has no profile row to read a display name from.
    #[test]
    fn a_ranked_row_carries_a_display_name_or_the_username_behind_it() {
        let store = Store::memory().unwrap();
        store.create_user("sam", 100).unwrap();
        save_profile(
            &store.conn.lock().unwrap(),
            "sam",
            &Profile {
                display: "Sam".into(),
                ..Profile::new("sam", 100)
            },
        )
        .unwrap();
        save_user(
            &store.conn.lock().unwrap(),
            "nameless",
            TEST_COURSE,
            &User::new(),
        )
        .unwrap();
        study(&store, "sam", 20, 1);
        study(&store, "nameless", 20, 1);

        let mut names: Vec<(String, String)> = store
            .load_activity_since(20)
            .unwrap()
            .into_iter()
            .map(|(username, display, _)| (username, display))
            .collect();
        names.sort();
        assert_eq!(
            names,
            [
                ("nameless".to_string(), "nameless".to_string()),
                ("sam".to_string(), "Sam".to_string()),
            ]
        );
    }

    #[test]
    fn sessions_round_trip_expire_and_can_be_revoked() {
        let store = Store::memory().unwrap();
        let token = store
            .create_session("sam", "sam@example.com", 200, 100)
            .unwrap();
        assert_eq!(token.len(), 64);
        let identity = store.load_session(&token, 150).unwrap().unwrap();
        assert_eq!(identity.username, "sam");
        assert_eq!(identity.email.as_deref(), Some("sam@example.com"));
        assert!(store.load_session(&token, 200).unwrap().is_none());

        let token = store
            .create_session("sam", "sam@example.com", 300, 200)
            .unwrap();
        store.delete_session(&token).unwrap();
        assert!(store.load_session(&token, 250).unwrap().is_none());
    }

    // The settings pages' two writes, from the sessions' side: a new address
    // reaches every browser the account is signed in on, and a new password
    // closes all of them but the one that changed it.
    #[test]
    fn changing_the_email_updates_every_session_and_a_password_ends_the_others() {
        let store = Store::memory().unwrap();
        let phone = store
            .create_session("sam", "sam@example.com", 300, 100)
            .unwrap();
        let laptop = store
            .create_session("sam", "sam@example.com", 300, 100)
            .unwrap();
        let someone_else = store
            .create_session("ana", "ana@example.com", 300, 100)
            .unwrap();

        store
            .update_session_email("sam", "new@example.com")
            .unwrap();
        for token in [&phone, &laptop] {
            let identity = store.load_session(token, 150).unwrap().unwrap();
            assert_eq!(identity.email.as_deref(), Some("new@example.com"));
        }

        store.delete_other_sessions("sam", &laptop).unwrap();
        assert!(store.load_session(&laptop, 150).unwrap().is_some());
        assert!(store.load_session(&phone, 150).unwrap().is_none());
        // and neither write reached past the account that made it
        let other = store.load_session(&someone_else, 150).unwrap().unwrap();
        assert_eq!(other.email.as_deref(), Some("ana@example.com"));
    }
}
