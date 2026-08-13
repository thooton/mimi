/* The inbox's connection to the backend (see mimi_backend/src/messages.rs).

   One EventSource per open page carries the initial thread list and live
   changes down. Opening, reading and sending are ordinary HTTP commands in
   the other direction. The same event that delivers a stored message to its
   recipient delivers it to the sender's other tabs and the sending tab.

   This module is the wire and nothing else — it parses events, hands them to
   the handlers it was given, issues commands, and lets EventSource reconnect
   when the feed drops. What any of it *means* to the page belongs to InboxApp,
   which is the only thing that knows which conversation is on screen.

   Event types live here rather than in api.ts because they form one small
   streaming protocol. `connect` is handed the inbox's HTTP address rather
   than working it out, which also leaves everything below testable off a
   browser. */

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

   Snake case, and a `type` tag on every event. Kept apart from the camelCase
   the rest of the file uses so that exactly one place converts between them:
   a feed that quietly handed `sent_at` to the UI would work until the day
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

type ServerEvent =
  | { type: 'threads'; me: string; threads: WireThread[] }
  | { type: 'thread'; with: string; display: string; messages: WireMessage[] }
  | { type: 'message'; with: string; display: string; message: WireMessage }
  | { type: 'read'; with: string };

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
  /** the list, which arrives unasked the moment the feed opens — and again
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
  /** whether there is a live event feed, so the page can say when there isn't */
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

function reason(value: unknown): string {
  return value instanceof Error ? value.message : String(value);
}

async function command<T>(url: string, init?: RequestInit): Promise<T> {
  const response = await fetch(url, {
    ...init,
    credentials: 'include',
    headers: init?.body ? { 'content-type': 'application/json' } : {},
  });
  if (!response.ok) {
    const body = await response.json().catch(() => null) as { error?: string } | null;
    throw new Error(body?.error ?? `request failed: ${response.status}`);
  }
  return response.status === 204 ? undefined as T : response.json() as Promise<T>;
}

/** Open an inbox event feed and let the browser keep it open. EventSource
    owns reconnection; every reconnect receives a fresh thread list, and the
    page re-opens its current conversation to recover anything missed. */
export function connect(url: string, handlers: InboxHandlers): Inbox {
  const events = new EventSource(url, { withCredentials: true });

  const dispatch = (event: ServerEvent) => {
    switch (event.type) {
      case 'threads':
        handlers.onThreads(event.me, event.threads.map(thread));
        break;
      case 'thread':
        handlers.onThread(event.with, event.display, event.messages.map(message));
        break;
      case 'message':
        handlers.onMessage(event.with, event.display, message(event.message));
        break;
      case 'read':
        handlers.onRead(event.with);
        break;
    }
  };

  events.onopen = () => handlers.onLive(true);
  events.onerror = () => handlers.onLive(false);
  events.onmessage = (incoming) => {
    try {
      dispatch(JSON.parse(incoming.data) as ServerEvent);
    } catch {
      /* an event we can't read is the backend's problem, not the page's */
    }
  };

  const report = (error: unknown) => handlers.onError(reason(error));
  const correspondent = (username: string) =>
    `${url}/with/${encodeURIComponent(username)}`;

  return {
    open: (username) => {
      command<ServerEvent>(correspondent(username)).then(dispatch).catch(report);
    },
    read: (username) => {
      command<void>(`${correspondent(username)}/read`, { method: 'PUT' }).catch(report);
    },
    send: (to, body) => {
      command<void>(correspondent(to), {
        method: 'POST',
        body: JSON.stringify({ body }),
      }).catch(report);
    },
    close: () => {
      events.close();
      handlers.onLive(false);
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
