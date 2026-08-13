// User-to-user messages: the inbox, and the socket it is read through.
//
// A **thread** is a pair of people, and that is all it is. There is no thread
// record to create before the first message and none to tidy up after the
// last — `Store::load_threads` derives the list from the messages themselves,
// so a conversation exists exactly while something has been said in it.
//
// **The socket is the whole API.** There is no REST shape for any of this,
// deliberately: an inbox is a thing two people change while both are looking
// at it, so every read here would need a poll behind it to stay true. One
// connection per open page, a frame in each direction, and the same frame
// that delivers a message to the person it was sent to delivers it to the
// sender's other tabs.
//
// The pieces:
//
// - **`Broker`** is the routing table — username → their live sockets. It
//   holds no messages and no history: the database is the record, and this is
//   only how a write reaches somebody who is looking. A learner with no page
//   open has no entry, which is the same thing as being offline.
// - **`serve`** is one connection: it writes the thread list, then relays
//   between the socket and that connection's channel until one of them ends.
// - **`handle`** is what a client frame *means*, and it is the seam the tests
//   use — a frame in, the frames it answers with out, and whatever it
//   published on the way past reachable through the broker.
//
// **Every send publishes to both ends, including the sender's own.** The tab
// that sent a message learns it landed the same way every other tab does,
// through the broker, so there is one path into the client's state instead of
// an optimistic local one and a real one that have to agree.
//
// Guests are not here at all — the route turns them away (see
// `server::inbox_socket`) for the reason they are not on the leaderboard and
// cannot be followed: there is nobody behind the record to write to, and it
// goes when its cookie does.

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use axum::extract::ws::{Message as Frame, WebSocket};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

use crate::server::now;
use crate::store::Store;

// How much can be said at once. Long enough for a paragraph of encouragement
// in a language you are still learning, short enough that a frame is a frame.
const MAX_BODY: usize = 2000;

// How much of a conversation is served when it is opened. There is no way to
// ask for what came before (see `Store::load_thread`), so this is also how
// far back a thread can be read — which is the trade being made until paging
// is worth its own protocol.
const THREAD_LIMIT: u32 = 200;

/// One message, exactly as it is stored. This is what goes on the wire: a
/// message the client has is a row the database has.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Message {
    /// its place in the thread, and what "read up to here" counts in
    pub id: i64,
    pub from: String,
    pub body: String,
    pub sent_at: u64,
}

/// One row of the inbox list: who the conversation is with, the last thing
/// said in it, and whether the reader has seen that.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Thread {
    /// the *other* person — a thread is named by whoever isn't reading it
    pub with: String,
    pub display: String,
    /// so the list can say "You: …" without guessing from the body
    pub last_sender: String,
    pub last: String,
    pub sent_at: u64,
    pub unread: bool,
}

// --- the broker ---

/// Every open inbox, by whose it is. One learner may have several — two tabs,
/// a phone — and a message reaches all of them or none.
pub struct Broker {
    connections: Mutex<HashMap<String, Vec<(u64, UnboundedSender<String>)>>>,
    // connections are told apart by a number rather than by their channel,
    // because closing one has to remove *that* one and a sender has no
    // identity worth comparing
    next: AtomicU64,
}

/// A registration, held for as long as the socket is: its number, and the end
/// of the channel `serve` reads.
pub struct Connection {
    id: u64,
    events: UnboundedReceiver<String>,
}

impl Broker {
    pub fn new() -> Broker {
        Broker {
            connections: Mutex::new(HashMap::new()),
            next: AtomicU64::new(0),
        }
    }

    /// Register an open inbox for `username`.
    pub fn connect(&self, username: &str) -> Connection {
        // Unbounded because the alternative is worse: a bounded channel makes
        // publishing await a reader, which would let one wedged socket hold
        // up the send that is trying to reach it. What accumulates is text
        // somebody typed, and a socket that has stopped reading is one the
        // runtime is about to drop anyway.
        let (events, receiver) = unbounded_channel();
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        self.connections
            .lock()
            .unwrap()
            .entry(username.to_string())
            .or_default()
            .push((id, events));
        Connection {
            id,
            events: receiver,
        }
    }

    /// The page has gone. The last connection takes the whole entry with it,
    /// so the table holds the people who are looking rather than everybody
    /// who ever has.
    pub fn disconnect(&self, username: &str, id: u64) {
        let mut connections = self.connections.lock().unwrap();
        let Some(open) = connections.get_mut(username) else {
            return;
        };
        open.retain(|(open_id, _)| *open_id != id);
        if open.is_empty() {
            connections.remove(username);
        }
    }

    /// Send one already-encoded frame to every inbox this user has open.
    /// Nobody looking is not a failure — it is the ordinary case, and the
    /// message is in the database either way.
    pub fn publish(&self, username: &str, frame: &str) {
        let mut connections = self.connections.lock().unwrap();
        let Some(open) = connections.get_mut(username) else {
            return;
        };
        // a closed receiver is a socket whose task has already ended; drop it
        // here rather than waiting for its own `disconnect`
        open.retain(|(_, events)| events.send(frame.to_string()).is_ok());
        if open.is_empty() {
            connections.remove(username);
        }
    }
}

// --- the protocol ---

/// What a client may say. Tagged by `type`, so the wire reads like the code:
/// `{"type":"send","to":"ana","body":"hola"}`.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ClientFrame {
    /// show me this conversation (and mark it read)
    Open {
        with: String,
    },
    /// I am looking at this conversation and something just arrived in it
    Read {
        with: String,
    },
    Send {
        to: String,
        body: String,
    },
}

/// What the server says. `threads` arrives unasked, the moment the socket
/// opens; the rest answer something.
#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ServerFrame<'a> {
    /// the inbox list, newest first — and who the reader is, so the client
    /// need not be told twice which messages are its own
    Threads { me: &'a str, threads: Vec<Thread> },
    Thread {
        with: &'a str,
        display: &'a str,
        messages: Vec<Message>,
    },
    /// One message, to one side of the conversation: `with` and `display`
    /// name the *other* person from that side's point of view, which is why
    /// the two ends get different frames for the same row.
    ///
    /// This is both of the updates a client needs. If the named thread is the
    /// one on screen, the message belongs at the bottom of it; either way the
    /// thread it belongs to has just become the most recent one.
    Message {
        with: &'a str,
        display: &'a str,
        message: &'a Message,
    },
    /// this conversation has no unread messages left in it
    Read { with: &'a str },
    /// something was refused. The socket stays open: a message too long to
    /// send is not a reason to lose the ones already on screen.
    Error { message: &'a str },
}

fn encode(frame: &ServerFrame) -> String {
    // every frame is plain data with string keys, so this cannot fail
    serde_json::to_string(frame).unwrap()
}

fn refuse(message: &str) -> Vec<String> {
    vec![encode(&ServerFrame::Error { message })]
}

// --- one connection ---

/// Relay between one socket and its owner's inbox until either end stops.
///
/// The loop writes what it owes, then waits for whichever comes first: a
/// frame from the client, or something the broker has for us. Handling a
/// client frame produces the replies *for this socket only* — anything the
/// other side of a conversation should hear is published, and reaches them
/// (and our own other tabs) through their own connection.
pub async fn serve(mut socket: WebSocket, me: String, store: &Store, broker: &Broker) {
    let Connection { id, mut events } = broker.connect(&me);

    // The list, unasked: a page that has just opened has nothing to show
    // until it has this, so waiting to be asked for it would only add a round
    // trip to every visit.
    let mut pending = match store.load_threads(&me) {
        Ok(threads) => vec![encode(&ServerFrame::Threads { me: &me, threads })],
        Err(error) => {
            eprintln!("database error: {error}");
            refuse("database error")
        }
    };

    'session: loop {
        for frame in pending.drain(..) {
            if socket.send(Frame::Text(frame.into())).await.is_err() {
                break 'session;
            }
        }

        // Neither branch touches the socket: the client frame is turned into
        // text here and written at the top of the next lap. That is not only
        // tidiness — a body that used `socket` while the other branch's read
        // future was still alive would be two mutable borrows of it.
        pending = tokio::select! {
            incoming = socket.recv() => match incoming {
                Some(Ok(Frame::Text(text))) => handle(&text, &me, store, broker),
                // pings and pongs are answered by axum, and nothing here
                // speaks binary
                Some(Ok(Frame::Binary(_) | Frame::Ping(_) | Frame::Pong(_))) => Vec::new(),
                // said goodbye, hung up, or stopped making sense
                Some(Ok(Frame::Close(_))) | Some(Err(_)) | None => break 'session,
            },
            event = events.recv() => match event {
                Some(frame) => vec![frame],
                // the broker dropped our sender, which it only does when it
                // has already forgotten us
                None => break 'session,
            },
        };
    }

    broker.disconnect(&me, id);
}

/// What one client frame means. Split out from the loop above because this is
/// the whole protocol and none of the plumbing: it takes text and returns the
/// text to answer with, which is exactly what a test wants to say.
fn handle(text: &str, me: &str, store: &Store, broker: &Broker) -> Vec<String> {
    let Ok(frame) = serde_json::from_str::<ClientFrame>(text) else {
        return refuse("unreadable frame");
    };
    let result = match frame {
        ClientFrame::Open { with } => open(me, &with, store, broker),
        ClientFrame::Read { with } => read(me, &with, store, broker),
        ClientFrame::Send { to, body } => send(me, &to, &body, store, broker),
    };
    match result {
        Ok(frames) => frames,
        Err(error) => {
            // the same reading as everywhere else: a database that won't
            // answer is this server's fault and not something the learner can
            // do anything about, so they are told that much and no more
            eprintln!("database error: {error}");
            refuse("database error")
        }
    }
}

/// Show a conversation. Opening one is also reading it, so the dot goes out
/// here — on every tab of the reader's, which is why it is published rather
/// than left for this socket's reply to imply.
fn open(me: &str, with: &str, store: &Store, broker: &Broker) -> rusqlite::Result<Vec<String>> {
    let display = match correspondent(me, with, store)? {
        Ok(display) => display,
        Err(refusal) => return Ok(refusal),
    };
    let messages = store.load_thread(me, with, THREAD_LIMIT)?;
    store.mark_read(me, with)?;
    broker.publish(me, &encode(&ServerFrame::Read { with }));
    Ok(vec![encode(&ServerFrame::Thread {
        with,
        display: &display,
        messages,
    })])
}

/// A message arrived in the conversation already on screen. Without this the
/// dot would come back on the next reload for something the reader watched
/// arrive — `open` is not repeated, because the client has the message
/// already and re-sending the thread would only make it flicker.
fn read(me: &str, with: &str, store: &Store, broker: &Broker) -> rusqlite::Result<Vec<String>> {
    if let Err(refusal) = correspondent(me, with, store)? {
        return Ok(refusal);
    }
    store.mark_read(me, with)?;
    broker.publish(me, &encode(&ServerFrame::Read { with }));
    Ok(Vec::new())
}

/// Say something. The message is stored first and delivered from what was
/// stored, so what the two ends have on screen is the row rather than two
/// hopeful copies of it.
fn send(
    me: &str,
    to: &str,
    body: &str,
    store: &Store,
    broker: &Broker,
) -> rusqlite::Result<Vec<String>> {
    let body = body.trim();
    if body.is_empty() {
        return Ok(refuse("a message needs something in it"));
    }
    // counted in characters, because that is what somebody typing is counting
    if body.chars().count() > MAX_BODY {
        return Ok(refuse(&format!(
            "a message can be at most {MAX_BODY} characters"
        )));
    }
    let theirs = match correspondent(me, to, store)? {
        Ok(display) => display,
        Err(refusal) => return Ok(refusal),
    };
    // what *they* will see this conversation named as
    let mine = match store.correspondent(me)? {
        Some((_, display)) => display,
        // the session names an account that has since been deleted
        None => return Ok(refuse("your account no longer exists")),
    };

    let message = store.send_message(me, to, body, now())?;
    // somebody has certainly seen the message they just wrote
    store.mark_read(me, to)?;
    broker.publish(
        to,
        &encode(&ServerFrame::Message {
            with: me,
            display: &mine,
            message: &message,
        }),
    );
    broker.publish(
        me,
        &encode(&ServerFrame::Message {
            with: to,
            display: &theirs,
            message: &message,
        }),
    );
    // nothing direct: this socket is one of `me`'s, so it has just been
    // published to like all the others
    Ok(Vec::new())
}

/// Who a frame is addressed to, if it is somebody who can be written to:
/// their display name on the way through. The outer `Result` is the
/// database's; the inner one is the answer, so a refusal reads as a value
/// rather than as an error.
#[allow(clippy::type_complexity)]
fn correspondent(
    me: &str,
    with: &str,
    store: &Store,
) -> rusqlite::Result<Result<String, Vec<String>>> {
    if with == me {
        // A note to yourself is a different feature wearing this one's
        // clothes, and it would put a thread in the list that nobody can
        // reply to.
        return Ok(Err(refuse("you cannot message yourself")));
    }
    Ok(match store.correspondent(with)? {
        None => Err(refuse(&format!("no user named '{with}'"))),
        // A guest is a record with a week to live and no name behind it, so
        // there is nobody there to write to. They cannot reach this end
        // either — the socket route turns them away.
        Some((true, _)) => Err(refuse("that account cannot be messaged")),
        Some((false, display)) => Ok(display),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    // Two accounts and an empty inbox each.
    fn store() -> Store {
        let store = Store::memory().unwrap();
        store.create_user("sam", 100).unwrap();
        store.create_user("ana", 100).unwrap();
        store
    }

    fn frames(raw: &[String]) -> Vec<Value> {
        raw.iter()
            .map(|text| serde_json::from_str(text).unwrap())
            .collect()
    }

    // What one of somebody's open pages has been handed, as parsed frames.
    fn delivered(connection: &mut Connection) -> Vec<Value> {
        let mut out = Vec::new();
        while let Ok(frame) = connection.events.try_recv() {
            out.push(serde_json::from_str(&frame).unwrap());
        }
        out
    }

    // The shape the whole feature turns on: one write, and both ends hear
    // about it through their own connection — the sender's included, so a
    // second tab of theirs is as up to date as the recipient is.
    #[test]
    fn a_sent_message_reaches_both_ends_and_every_page_they_have_open() {
        let store = store();
        let broker = Broker::new();
        let mut phone = broker.connect("sam");
        let mut laptop = broker.connect("sam");
        let mut hers = broker.connect("ana");

        let replies = handle(
            r#"{"type":"send","to":"ana","body":"hola"}"#,
            "sam",
            &store,
            &broker,
        );

        // nothing comes back down the socket that asked: its copy arrives
        // through the broker with everybody else's
        assert!(replies.is_empty());
        for page in [&mut phone, &mut laptop] {
            let sent = delivered(page);
            assert_eq!(sent.len(), 1);
            assert_eq!(sent[0]["type"], "message");
            // from sam's side the conversation is named after ana
            assert_eq!(sent[0]["with"], "ana");
            assert_eq!(sent[0]["message"]["from"], "sam");
            assert_eq!(sent[0]["message"]["body"], "hola");
        }
        // and from ana's side it is named after sam
        let received = delivered(&mut hers);
        assert_eq!(received[0]["with"], "sam");
        assert_eq!(received[0]["message"]["body"], "hola");
    }

    // The dot: a message is unread until the person it was sent to looks at
    // the conversation, and never unread to whoever wrote it.
    #[test]
    fn a_thread_is_unread_until_it_is_opened_and_then_it_is_not() {
        let store = store();
        let broker = Broker::new();
        handle(
            r#"{"type":"send","to":"ana","body":"hola"}"#,
            "sam",
            &store,
            &broker,
        );

        let threads = store.load_threads("ana").unwrap();
        assert_eq!(threads.len(), 1);
        assert_eq!(threads[0].with, "sam");
        assert_eq!(threads[0].last, "hola");
        assert!(threads[0].unread);
        // the sender's own copy of the same conversation never is
        assert!(!store.load_threads("sam").unwrap()[0].unread);

        let mut page = broker.connect("ana");
        let replies = frames(&handle(
            r#"{"type":"open","with":"sam"}"#,
            "ana",
            &store,
            &broker,
        ));
        assert_eq!(replies[0]["type"], "thread");
        assert_eq!(replies[0]["messages"][0]["body"], "hola");
        assert!(!store.load_threads("ana").unwrap()[0].unread);
        // every page ana has open loses the dot, not just the one that opened
        // the conversation
        assert_eq!(delivered(&mut page)[0]["type"], "read");
    }

    // A message that arrives while its thread is on screen has been seen, and
    // saying so must not cost the reader the thread they are reading.
    #[test]
    fn reading_an_open_thread_clears_it_without_re_serving_it() {
        let store = store();
        let broker = Broker::new();
        handle(
            r#"{"type":"send","to":"ana","body":"hola"}"#,
            "sam",
            &store,
            &broker,
        );
        let mut page = broker.connect("ana");

        let replies = handle(r#"{"type":"read","with":"sam"}"#, "ana", &store, &broker);

        assert!(replies.is_empty());
        assert!(!store.load_threads("ana").unwrap()[0].unread);
        assert_eq!(delivered(&mut page)[0]["type"], "read");
    }

    // Every refusal leaves the socket open and says why, because the page on
    // the other end of it is still a page somebody is using.
    #[test]
    fn a_refused_message_is_an_answer_rather_than_a_closed_socket() {
        let store = store();
        let broker = Broker::new();

        for (frame, reason) in [
            (
                r#"{"type":"send","to":"ana","body":"   "}"#,
                "a message needs something in it",
            ),
            (
                r#"{"type":"send","to":"sam","body":"hi"}"#,
                "you cannot message yourself",
            ),
            (
                r#"{"type":"send","to":"nobody","body":"hi"}"#,
                "no user named 'nobody'",
            ),
            (r#"{"type":"shout"}"#, "unreadable frame"),
            (r#"not json at all"#, "unreadable frame"),
        ] {
            let replies = frames(&handle(frame, "sam", &store, &broker));
            assert_eq!(replies.len(), 1, "{frame}");
            assert_eq!(replies[0]["type"], "error", "{frame}");
            assert_eq!(replies[0]["message"], reason, "{frame}");
        }
        // and none of them left anything behind
        assert!(store.load_threads("sam").unwrap().is_empty());
    }

    // A guest is a record with a week to live and no name behind it. The
    // route keeps them from having an inbox; this keeps them out of anyone
    // else's.
    #[test]
    fn a_guest_is_not_somebody_you_can_write_to() {
        let store = store();
        let broker = Broker::new();
        let (guest, _) = store.create_guest(1_000, 100).unwrap();

        let replies = frames(&handle(
            &format!(r#"{{"type":"send","to":"{guest}","body":"hi"}}"#),
            "sam",
            &store,
            &broker,
        ));
        assert_eq!(replies[0]["message"], "that account cannot be messaged");
    }

    // Long enough to say something, bounded so that a frame is a frame.
    #[test]
    fn a_message_is_capped_at_a_length_a_person_could_type() {
        let store = store();
        let broker = Broker::new();
        let body = "a".repeat(MAX_BODY + 1);
        let frame = serde_json::json!({"type": "send", "to": "ana", "body": body}).to_string();

        let replies = frames(&handle(&frame, "sam", &store, &broker));
        assert_eq!(replies[0]["type"], "error");
        assert!(store.load_threads("ana").unwrap().is_empty());
    }

    // The routing table holds the people who are looking, and forgets them
    // the moment they stop.
    #[test]
    fn a_closed_page_stops_being_published_to() {
        let broker = Broker::new();
        let open = broker.connect("sam");
        let mut still_open = broker.connect("sam");
        broker.disconnect("sam", open.id);

        broker.publish("sam", "frame");

        assert_eq!(still_open.events.try_recv().unwrap(), "frame");
        assert!(broker.connections.lock().unwrap().contains_key("sam"));
        broker.disconnect("sam", still_open.id);
        // nobody looking is an absent entry rather than an empty one
        assert!(!broker.connections.lock().unwrap().contains_key("sam"));
        // and publishing to somebody who has gone is not an error
        broker.publish("sam", "frame");
    }
}
