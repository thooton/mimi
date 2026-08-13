// User-to-user messages: the inbox, its event stream, and its commands.
//
// A thread is a pair of people, and that is all it is. There is no thread
// record to create before the first message and none to tidy up after the
// last: `Store::load_threads` derives the list from the messages themselves,
// so a conversation exists exactly while something has been said in it.
//
// Events flow down; commands go up. One Server-Sent Events connection per
// open page carries the initial thread list and every live change after it.
// Ordinary authenticated HTTP requests open a thread, mark it read, or send a
// message. Keeping the directions separate fits HTTP while retaining one
// authoritative path into client state: even the sender sees a stored message
// through the event stream rather than inventing an optimistic copy.
//
// The pieces:
//
// - `Broker` is the routing table, username to their live event feeds. It
//   holds no messages and no history: the database is the record, and this is
//   only how a write reaches somebody who is looking. A learner with no page
//   open has no entry, which is the same thing as being offline.
// - `events` is one connection: it writes the thread list, then turns that
//   connection's broker channel into an SSE response until the page leaves.
// - `open`/`read`/`send` are the command side and the seam the tests use.
//
// Every send publishes to both ends, including the sender's own. The tab
// that sent a message learns it landed the same way every other tab does,
// through the broker, so there is one path into the client's state instead of
// an optimistic local one and a real one that have to agree.
//
// Guests are not here at all; the route turns them away (see
// `server::inbox_events`) for the reason they are not on the leaderboard and
// cannot be followed: there is nobody behind the record to write to, and it
// goes when its cookie does.

use std::collections::HashMap;
use std::convert::Infallible;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};

use axum::response::sse::{Event, KeepAlive, Sse};
use futures_util::{Stream, StreamExt, stream};
use serde::Serialize;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

use crate::server::now;
use crate::store::Store;

// How much can be said at once. Long enough for a paragraph of encouragement
// in a language you are still learning, short enough for one chat message.
const MAX_BODY: usize = 2000;

// How much of a conversation is served when it is opened. There is no way to
// ask for what came before (see `Store::load_thread`), so this is also how
// far back a thread can be read, which is the trade being made until paging
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
    /// the other person: a thread is named by whoever isn't reading it
    pub with: String,
    pub display: String,
    /// so the list can say "You: …" without guessing from the body
    pub last_sender: String,
    pub last: String,
    pub sent_at: u64,
    pub unread: bool,
}

// --- the broker ---

/// Every open inbox, by whose it is. One learner may have several (two tabs, a
/// phone) and a message reaches all of them or none.
#[derive(Clone)]
pub struct Broker {
    inner: Arc<BrokerInner>,
}

struct BrokerInner {
    connections: Mutex<HashMap<String, Vec<(u64, UnboundedSender<ServerEvent>)>>>,
    // connections are told apart by a number rather than by their channel,
    // because closing one has to remove that one and a sender has no
    // identity worth comparing
    next: AtomicU64,
}

/// A registration, held for as long as the event feed is: its number, owner,
/// and the receiving end of the channel the SSE response reads.
pub struct Connection {
    id: u64,
    username: String,
    broker: Broker,
    events: UnboundedReceiver<ServerEvent>,
}

impl Broker {
    pub fn new() -> Broker {
        Broker {
            inner: Arc::new(BrokerInner {
                connections: Mutex::new(HashMap::new()),
                next: AtomicU64::new(0),
            }),
        }
    }

    /// Register an open inbox for `username`.
    pub fn connect(&self, username: &str) -> Connection {
        // Unbounded because the alternative is worse: a bounded channel makes
        // publishing await a reader, which would let one wedged feed hold
        // up the send that is trying to reach it. What accumulates is text
        // somebody typed, and a feed that has stopped reading is one the
        // runtime is about to drop anyway.
        let (events, receiver) = unbounded_channel();
        let id = self.inner.next.fetch_add(1, Ordering::Relaxed);
        self.inner
            .connections
            .lock()
            .unwrap()
            .entry(username.to_string())
            .or_default()
            .push((id, events));
        Connection {
            id,
            username: username.to_string(),
            broker: self.clone(),
            events: receiver,
        }
    }

    /// The page has gone. The last connection takes the whole entry with it,
    /// so the table holds the people who are looking rather than everybody
    /// who ever has.
    fn disconnect(&self, username: &str, id: u64) {
        let mut connections = self.inner.connections.lock().unwrap();
        let Some(open) = connections.get_mut(username) else {
            return;
        };
        open.retain(|(open_id, _)| *open_id != id);
        if open.is_empty() {
            connections.remove(username);
        }
    }

    /// Send one event to every inbox this user has open.
    /// Nobody looking is not a failure but the ordinary case, and the message
    /// is in the database either way.
    fn publish(&self, username: &str, event: ServerEvent) {
        let mut connections = self.inner.connections.lock().unwrap();
        let Some(open) = connections.get_mut(username) else {
            return;
        };
        // a closed receiver is a feed whose task has already ended; drop it
        // here rather than waiting for its own `disconnect`
        open.retain(|(_, events)| events.send(event.clone()).is_ok());
        if open.is_empty() {
            connections.remove(username);
        }
    }
}

impl Stream for Connection {
    type Item = ServerEvent;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.events.poll_recv(cx)
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        self.broker.disconnect(&self.username, self.id);
    }
}

// --- the event stream ---

/// What the server says. `threads` arrives unasked when an event stream opens;
/// `thread` is the response to an open command; the live changes are
/// `message` and `read`. The tag keeps the SSE data self-describing and lets
/// reconnects use exactly the same client dispatch path as first connects.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerEvent {
    /// the inbox list, newest first, and who the reader is, so the client need
    /// not be told twice which messages are its own
    Threads { me: String, threads: Vec<Thread> },
    Thread {
        with: String,
        display: String,
        messages: Vec<Message>,
    },
    /// One message, to one side of the conversation: `with` and `display`
    /// name the other person from that side's point of view, which is why
    /// the two ends get different events for the same row.
    ///
    /// This is both of the updates a client needs. If the named thread is the
    /// one on screen, the message belongs at the bottom of it; either way the
    /// thread it belongs to has just become the most recent one.
    Message {
        with: String,
        display: String,
        message: Message,
    },
    /// this conversation has no unread messages left in it
    Read { with: String },
}

fn sse_event(event: ServerEvent) -> Result<Event, Infallible> {
    // every event is plain serializable data, so this cannot fail
    Ok(Event::default().json_data(event).unwrap())
}

/// Open a page's event feed. Registration happens before the initial database
/// read, so a message concurrent with connecting is either present in that
/// snapshot or queued behind it (and harmlessly idempotent in the client).
pub fn events(
    me: String,
    store: &Store,
    broker: &Broker,
) -> rusqlite::Result<Sse<impl Stream<Item = Result<Event, Infallible>> + Send + 'static + use<>>> {
    let connection = broker.connect(&me);
    let initial = ServerEvent::Threads {
        threads: store.load_threads(&me)?,
        me,
    };
    let stream = stream::once(async move { initial })
        .chain(connection)
        .map(sse_event);
    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

#[derive(Debug)]
pub enum CommandError {
    Rejected(String),
    Database(rusqlite::Error),
}

impl From<rusqlite::Error> for CommandError {
    fn from(value: rusqlite::Error) -> Self {
        Self::Database(value)
    }
}

/// Show a conversation. Opening one is also reading it, so the dot goes out
/// here, on every tab of the reader's, which is why it is published rather
/// than left for this command's reply to imply.
pub fn open(
    me: &str,
    with: &str,
    store: &Store,
    broker: &Broker,
) -> Result<ServerEvent, CommandError> {
    let display = correspondent(me, with, store)?;
    let messages = store.load_thread(me, with, THREAD_LIMIT)?;
    store.mark_read(me, with)?;
    broker.publish(
        me,
        ServerEvent::Read {
            with: with.to_string(),
        },
    );
    Ok(ServerEvent::Thread {
        with: with.to_string(),
        display,
        messages,
    })
}

/// A message arrived in the conversation already on screen. Without this the
/// dot would come back on the next reload for something the reader watched
/// arrive. `open` is not repeated, because the client has the message already
/// and re-sending the thread would only make it flicker.
pub fn read(me: &str, with: &str, store: &Store, broker: &Broker) -> Result<(), CommandError> {
    correspondent(me, with, store)?;
    store.mark_read(me, with)?;
    broker.publish(
        me,
        ServerEvent::Read {
            with: with.to_string(),
        },
    );
    Ok(())
}

/// Say something. The message is stored first and delivered from what was
/// stored, so what the two ends have on screen is the row rather than two
/// hopeful copies of it.
pub fn send(
    me: &str,
    to: &str,
    body: &str,
    store: &Store,
    broker: &Broker,
) -> Result<(), CommandError> {
    let body = body.trim();
    if body.is_empty() {
        return Err(CommandError::Rejected(
            "a message needs something in it".into(),
        ));
    }
    // counted in characters, because that is what somebody typing is counting
    if body.chars().count() > MAX_BODY {
        return Err(CommandError::Rejected(format!(
            "a message can be at most {MAX_BODY} characters"
        )));
    }
    let theirs = correspondent(me, to, store)?;
    // what they will see this conversation named as
    let mine = match store.correspondent(me)? {
        Some((_, display)) => display,
        // the session names an account that has since been deleted
        None => {
            return Err(CommandError::Rejected(
                "your account no longer exists".into(),
            ));
        }
    };

    let message = store.send_message(me, to, body, now())?;
    // somebody has certainly seen the message they just wrote
    store.mark_read(me, to)?;
    broker.publish(
        to,
        ServerEvent::Message {
            with: me.to_string(),
            display: mine,
            message: message.clone(),
        },
    );
    broker.publish(
        me,
        ServerEvent::Message {
            with: to.to_string(),
            display: theirs,
            message,
        },
    );
    // no command response body: this page's feed is one of `me`'s, so it has
    // just been published to like all the others
    Ok(())
}

/// Who a command is addressed to, if it is somebody who can be written to:
/// their display name on the way through. Rejections are client errors while
/// a failed lookup remains a database error for the server to hide.
fn correspondent(me: &str, with: &str, store: &Store) -> Result<String, CommandError> {
    if with == me {
        // A note to yourself is a different feature wearing this one's
        // clothes, and it would put a thread in the list that nobody can
        // reply to.
        return Err(CommandError::Rejected("you cannot message yourself".into()));
    }
    match store.correspondent(with)? {
        None => Err(CommandError::Rejected(format!("no user named '{with}'"))),
        // A guest is a record with a week to live and no name behind it, so
        // there is nobody there to write to. They cannot reach this end
        // either, since the event route turns them away.
        Some((true, _)) => Err(CommandError::Rejected(
            "that account cannot be messaged".into(),
        )),
        Some((false, display)) => Ok(display),
    }
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

    // What one of somebody's open pages has been handed, as JSON values.
    fn delivered(connection: &mut Connection) -> Vec<Value> {
        let mut out = Vec::new();
        while let Ok(event) = connection.events.try_recv() {
            out.push(serde_json::to_value(event).unwrap());
        }
        out
    }

    // The shape the whole feature turns on: one write, and both ends hear
    // about it through their own connection, the sender's included, so a
    // second tab of theirs is as up to date as the recipient is.
    #[test]
    fn a_sent_message_reaches_both_ends_and_every_page_they_have_open() {
        let store = store();
        let broker = Broker::new();
        let mut phone = broker.connect("sam");
        let mut laptop = broker.connect("sam");
        let mut hers = broker.connect("ana");

        send("sam", "ana", "hola", &store, &broker).unwrap();

        // The command has no representation of its own: its stored result
        // arrives through the broker with everybody else's.
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
        send("sam", "ana", "hola", &store, &broker).unwrap();

        let threads = store.load_threads("ana").unwrap();
        assert_eq!(threads.len(), 1);
        assert_eq!(threads[0].with, "sam");
        assert_eq!(threads[0].last, "hola");
        assert!(threads[0].unread);
        // the sender's own copy of the same conversation never is
        assert!(!store.load_threads("sam").unwrap()[0].unread);

        let mut page = broker.connect("ana");
        let reply = serde_json::to_value(open("ana", "sam", &store, &broker).unwrap()).unwrap();
        assert_eq!(reply["type"], "thread");
        assert_eq!(reply["messages"][0]["body"], "hola");
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
        send("sam", "ana", "hola", &store, &broker).unwrap();
        let mut page = broker.connect("ana");

        read("ana", "sam", &store, &broker).unwrap();

        assert!(!store.load_threads("ana").unwrap()[0].unread);
        assert_eq!(delivered(&mut page)[0]["type"], "read");
    }

    // Every refusal is a command error rather than an event-stream failure,
    // because the page is still a page somebody is using.
    #[test]
    fn a_refused_message_is_a_command_error_rather_than_a_closed_stream() {
        let store = store();
        let broker = Broker::new();

        for (to, body, reason) in [
            ("ana", "   ", "a message needs something in it"),
            ("sam", "hi", "you cannot message yourself"),
            ("nobody", "hi", "no user named 'nobody'"),
        ] {
            let CommandError::Rejected(message) =
                send("sam", to, body, &store, &broker).unwrap_err()
            else {
                panic!("expected a rejected command");
            };
            assert_eq!(message, reason);
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

        let CommandError::Rejected(message) =
            send("sam", &guest, "hi", &store, &broker).unwrap_err()
        else {
            panic!("expected a rejected command");
        };
        assert_eq!(message, "that account cannot be messaged");
    }

    // Long enough to say something, bounded to one human-scale message.
    #[test]
    fn a_message_is_capped_at_a_length_a_person_could_type() {
        let store = store();
        let broker = Broker::new();
        let body = "a".repeat(MAX_BODY + 1);
        assert!(matches!(
            send("sam", "ana", &body, &store, &broker),
            Err(CommandError::Rejected(_))
        ));
        assert!(store.load_threads("ana").unwrap().is_empty());
    }

    // The routing table holds the people who are looking, and forgets them
    // the moment they stop.
    #[test]
    fn a_closed_page_stops_being_published_to() {
        let broker = Broker::new();
        let open = broker.connect("sam");
        let mut still_open = broker.connect("sam");
        drop(open);

        broker.publish("sam", ServerEvent::Read { with: "ana".into() });

        assert!(matches!(
            still_open.events.try_recv().unwrap(),
            ServerEvent::Read { .. }
        ));
        assert!(broker.inner.connections.lock().unwrap().contains_key("sam"));
        drop(still_open);
        // nobody looking is an absent entry rather than an empty one
        assert!(!broker.inner.connections.lock().unwrap().contains_key("sam"));
        // and publishing to somebody who has gone is not an error
        broker.publish("sam", ServerEvent::Read { with: "ana".into() });
    }
}
