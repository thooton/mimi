/* The inbox's connection to the backend (see mimi_backend/src/messages.rs).

   Messaging has no REST endpoints, deliberately: an inbox is a thing two
   people change while both are looking at it, so every read would need a poll
   behind it to stay true. One socket per open page instead, a frame in each
   direction, and the same frame that delivers a message to the person it was
   sent to delivers it to the sender's other tabs.

   This module is the wire and nothing else — it parses frames, hands them to
   the handlers it was given, and puts the connection back when it drops. What
   any of it *means* to the page belongs to InboxApp, which is the only thing
   that knows which conversation is on screen.

   Frame types live here rather than in api.ts because they are not the HTTP
   API: they are a protocol with a session behind it, and the only piece of
   messaging that api.ts owns is the user search the new-conversation box
   uses. Where the socket *goes* is still api.ts's business, though — it is
   the same backend — so `connect` is handed an address rather than working
   one out, which also leaves everything below testable off a browser. */

/** one message, exactly as the backend stores it */
export interface Message {
  /** its place in the thread — the order messages are in, and what "read up
      to here" is counted in */
  id: number;
  from: string;
  body: string;
  sentAt: number;
}

/** one row of the thread list: a conversation, seen from the reader's side */
export interface Thread {
  /** the *other* person: a thread is named by whoever isn't reading it */
  with: string;
  display: string;
  /** so the row can say "You: …" rather than guess from the body */
  lastSender: string;
  last: string;
  sentAt: number;
  unread: boolean;
}

/* --- the wire ---

   Snake case, and a `type` tag on every frame. Kept apart from the camelCase
   the rest of the file uses so that exactly one place converts between them:
   a socket that quietly handed `sent_at` to the UI would work until the day
   somebody read it. */

interface WireMessage {
  id: number;
  from: string;
  body: string;
  sent_at: number;
}

interface WireThread {
  with: string;
  display: string;
  last_sender: string;
  last: string;
  sent_at: number;
  unread: boolean;
}

type ServerFrame =
  | { type: 'threads'; me: string; threads: WireThread[] }
  | { type: 'thread'; with: string; display: string; messages: WireMessage[] }
  | { type: 'message'; with: string; display: string; message: WireMessage }
  | { type: 'read'; with: string }
  | { type: 'error'; message: string };

function message(wire: WireMessage): Message {
  return { id: wire.id, from: wire.from, body: wire.body, sentAt: wire.sent_at };
}

function thread(wire: WireThread): Thread {
  return {
    with: wire.with,
    display: wire.display,
    lastSender: wire.last_sender,
    last: wire.last,
    sentAt: wire.sent_at,
    unread: wire.unread,
  };
}

export interface InboxHandlers {
  /** the list, which arrives unasked the moment the socket opens — and again
      after a reconnection, which is why it replaces rather than merges */
  onThreads(me: string, threads: Thread[]): void;
  onThread(username: string, display: string, messages: Message[]): void;
  /** One message, to one side of a conversation. This is both of the updates
      a page needs: if `username` is the conversation on screen the message
      belongs at the bottom of it, and either way that conversation has just
      become the most recent one. */
  onMessage(username: string, display: string, value: Message): void;
  /** this conversation has no unread messages left in it */
  onRead(username: string): void;
  onError(reason: string): void;
  /** whether there is a live socket, so the page can say when there isn't */
  onLive(live: boolean): void;
}

export interface Inbox {
  /** show me this conversation (and mark it read) */
  open(username: string): void;
  /** I am looking at this conversation and something just arrived in it */
  read(username: string): void;
  send(to: string, body: string): void;
  /** the page has gone; stop, and stop reconnecting */
  close(): void;
}

/* How long to wait before dialling again. One delay rather than a backoff
   curve: a dropped socket here is a laptop waking up or a wifi handover, and
   the honest response to that is to try again shortly. */
const RETRY_MS = 2000;

/** Open an inbox socket, and keep it open. A drop is not an error the page
    has to handle — the connection comes back, the backend sends the thread
    list again, and `onLive` is what the page shows in the meantime. */
export function connect(url: string, handlers: InboxHandlers): Inbox {
  let socket: WebSocket | null = null;
  let retry: ReturnType<typeof setTimeout> | null = null;
  let closed = false;

  const dial = () => {
    if (closed) return;
    const live = new WebSocket(url);
    socket = live;
    live.onopen = () => handlers.onLive(true);
    live.onmessage = (event) => {
      let frame: ServerFrame;
      try {
        frame = JSON.parse(event.data as string) as ServerFrame;
      } catch {
        /* a frame we can't read is the backend's problem, not the page's */
        return;
      }
      switch (frame.type) {
        case 'threads':
          handlers.onThreads(frame.me, frame.threads.map(thread));
          break;
        case 'thread':
          handlers.onThread(frame.with, frame.display, frame.messages.map(message));
          break;
        case 'message':
          handlers.onMessage(frame.with, frame.display, message(frame.message));
          break;
        case 'read':
          handlers.onRead(frame.with);
          break;
        case 'error':
          handlers.onError(frame.message);
          break;
      }
    };
    /* onclose covers both endings: a socket that fails to open closes too, so
       there is one path back to dialling rather than two that must agree. */
    live.onclose = () => {
      if (closed) return;
      handlers.onLive(false);
      retry = setTimeout(dial, RETRY_MS);
    };
  };

  const say = (frame: object) => {
    if (socket?.readyState === WebSocket.OPEN) socket.send(JSON.stringify(frame));
  };

  dial();

  return {
    open: (username) => say({ type: 'open', with: username }),
    read: (username) => say({ type: 'read', with: username }),
    send: (to, body) => say({ type: 'send', to, body }),
    close: () => {
      closed = true;
      if (retry) clearTimeout(retry);
      socket?.close();
    },
  };
}

/* --- reading a timestamp ---

   Messages are dated in local time, unlike everything on a profile. That is
   not an inconsistency: the activity record is keyed by UTC day so that two
   people reading the same streak agree about it, while a message happened at
   a moment, and the moment somebody wants is the one their own clock shows. */

/** the time of day a message was sent, 24-hour: "9:05", "21:24" */
export function clockTime(sentAt: number): string {
  const at = new Date(sentAt * 1000);
  return `${at.getHours()}:${String(at.getMinutes()).padStart(2, '0')}`;
}

/** How long ago, for the thread list: coarse on purpose, because what a row
    answers is "is this conversation still warm" rather than exactly when. */
export function ago(sentAt: number, now: number): string {
  const seconds = Math.max(0, now - sentAt);
  const minutes = Math.floor(seconds / 60);
  if (minutes < 1) return 'just now';
  if (minutes < 60) return `${minutes} minute${minutes === 1 ? '' : 's'} ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 48) return `${hours} hour${hours === 1 ? '' : 's'} ago`;
  const days = Math.floor(hours / 24);
  if (days < 31) return `${days} days ago`;
  const months = Math.floor(days / 30);
  if (months < 12) return `${months} month${months === 1 ? '' : 's'} ago`;
  const years = Math.floor(days / 365);
  return `${years} year${years === 1 ? '' : 's'} ago`;
}

/** Where the thread list puts a conversation that has just been written in:
    at the top, and only once — the same pair is the same thread, so an
    arriving message moves the row it belongs to rather than adding one. */
export function withMessage(
  threads: Thread[],
  username: string,
  display: string,
  value: Message,
  unread: boolean,
): Thread[] {
  const rest = threads.filter((row) => row.with !== username);
  return [
    {
      with: username,
      display,
      lastSender: value.from,
      last: value.body,
      sentAt: value.sentAt,
      unread,
    },
    ...rest,
  ];
}
