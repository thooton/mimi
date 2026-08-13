import { useEffect, useRef, useState } from "react";
import type { FormEvent, KeyboardEvent } from "react";
import { Button, Icon, Spinner } from "@blueprintjs/core";
import { apiUrl, searchUsers } from "../../data/api";
import type { ApiFoundUser } from "../../data/api";
import { useAuth } from "../../data/auth";
import { ago, clockTime, connect, withMessage } from "../../data/inbox";
import type { Inbox, Message, Thread } from "../../data/inbox";

/* /inbox — every conversation this learner is in, and one of them open.

   Two columns: the threads on the left, the conversation on the right. Below
   the phone breakpoint they are the same column and only one of them is on
   screen at a time (see inbox.css) — which is why "which thread is open" is
   state here rather than a route: on a phone it is also *which page* you are
   looking at, and going back to the list must not be a page load.

   Live changes arrive over one event feed (see data/inbox.ts). The page holds no
   opinion the backend hasn't given it: a message it sends appears when the
   backend sends it back, so what is on screen is what was stored rather than
   an optimistic copy that has to be reconciled if the feed drops between
   the two.

   The address bar carries the open thread (`/inbox?with=ana`), so a profile's
   "Message" button lands on the right conversation and a reload keeps it. */

/** Somebody's initial, the same mark the account button in the bar wears. */
function Avatar({ display }: { display: string }) {
    return (
        <span className="avatar" aria-hidden="true">
            {display.charAt(0)}
        </span>
    );
}

function ThreadRow({
    thread,
    active,
    now,
    me,
    onOpen,
}: {
    thread: Thread;
    active: boolean;
    now: number;
    me: string;
    onOpen: () => void;
}) {
    const classes = ["thread"];
    if (active) classes.push("is-active");
    if (thread.unread) classes.push("is-unread");
    return (
        <li>
            <button
                type="button"
                className={classes.join(" ")}
                onClick={onOpen}
                aria-current={active ? "true" : undefined}
            >
                {/* present only while there is something here they haven't
                    read — once it disappears, the text takes its place */}
                <span
                    className="thread-dot"
                    aria-label={thread.unread ? "Unread" : undefined}
                />
                <span className="thread-text">
                    <span className="thread-head">
                        <span className="thread-name">{thread.display}</span>
                        <span className="thread-when">
                            {ago(thread.sentAt, now)}
                        </span>
                    </span>
                    <span className="thread-last">
                        {thread.lastSender === me && (
                            <span className="thread-mine">You: </span>
                        )}
                        {thread.last}
                    </span>
                </span>
            </button>
        </li>
    );
}

export default function InboxApp() {
    const { user, ready } = useAuth();
    /* A guest has no inbox — the event route turns them away, for the same
       reason they are not on the leaderboard and cannot be followed. The
       offer below is the answer, not the refusal. */
    const guest = ready && user?.guest === true;
    const signedIn = ready && user !== null && !guest;

    const [me, setMe] = useState("");
    const [threads, setThreads] = useState<Thread[]>([]);
    const [open, setOpen] = useState<string | null>(null);
    const [display, setDisplay] = useState("");
    /* null while a conversation is being fetched, so the pane can say it is
       loading rather than claim the thread is empty */
    const [messages, setMessages] = useState<Message[] | null>(null);
    const [draft, setDraft] = useState("");
    const [error, setError] = useState<string | null>(null);
    const [live, setLive] = useState(false);
    const [now, setNow] = useState(() => Math.floor(Date.now() / 1000));

    const [query, setQuery] = useState("");
    const [found, setFound] = useState<ApiFoundUser[] | null>(null);

    const inbox = useRef<Inbox | null>(null);
    /* The feed's handlers are installed once and live as long as the page,
       so what they read has to be a box rather than a closed-over value: `me`
       and `open` are both used to decide what an arriving message means. */
    const openRef = useRef<string | null>(null);
    const meRef = useRef("");
    const scroller = useRef<HTMLDivElement>(null);
    const composer = useRef<HTMLTextAreaElement>(null);

    /* The thread list says how long ago, so it has to be told that time has
       passed. A minute is the resolution the shortest label is written at. */
    useEffect(() => {
        const tick = setInterval(() => setNow(Math.floor(Date.now() / 1000)), 60_000);
        return () => clearInterval(tick);
    }, []);

    const show = (username: string, name: string) => {
        openRef.current = username;
        setOpen(username);
        setDisplay(name);
        setMessages(null);
        setError(null);
        /* the dot goes now rather than when the backend's `read` lands: the
           conversation is on screen, and a row that stayed lit under the
           reader's cursor would look broken */
        setThreads((rows) =>
            rows.map((row) =>
                row.with === username ? { ...row, unread: false } : row,
            ),
        );
        inbox.current?.open(username);
        window.history.replaceState(
            null,
            "",
            `/inbox?with=${encodeURIComponent(username)}`,
        );
    };

    useEffect(() => {
        if (!signedIn) return;
        /* Where the page was asked to start. Read once, and used when the
           thread list arrives — the feed is what establishes the inbox,
           and it isn't connected yet. */
        let wanted = new URLSearchParams(window.location.search).get("with");

        const connection = connect(apiUrl("/me/inbox"), {
            onThreads: (who, rows) => {
                meRef.current = who;
                setMe(who);
                setThreads(rows);
                /* The list also arrives after a reconnection, and then the
                   conversation on screen is re-opened rather than dropped —
                   whatever was said while the feed was away is in it. */
                const resume = wanted ?? openRef.current;
                wanted = null;
                if (resume) {
                    const known = rows.find((row) => row.with === resume);
                    show(resume, known?.display ?? resume);
                }
            },
            onThread: (username, name, list) => {
                if (username !== openRef.current) return;
                setDisplay(name);
                setMessages(list);
            },
            onMessage: (username, name, value) => {
                const mine = value.from === meRef.current;
                const looking = username === openRef.current;
                setThreads((rows) =>
                    withMessage(rows, username, name, value, !mine && !looking),
                );
                if (!looking) return;
                setMessages((list) =>
                    list?.some((item) => item.id === value.id)
                        ? list
                        : list
                          ? [...list, value]
                          : [value],
                );
                /* Watching a message arrive is reading it. Without this the
                   dot would be back on the next reload for something they
                   saw land. */
                if (!mine) connection.read(username);
            },
            onRead: (username) =>
                setThreads((rows) =>
                    rows.map((row) =>
                        row.with === username ? { ...row, unread: false } : row,
                    ),
                ),
            onError: setError,
            onLive: setLive,
        });
        inbox.current = connection;
        return () => {
            connection.close();
            inbox.current = null;
        };
        /* One event feed per signed-in page, and the session is the only thing
           that could change which. `show` is deliberately not a dependency:
           everything it touches is a ref or a setter, and re-dialling on
           every render is the bug leaving it out avoids. */
    }, [signedIn]);

    /* Opening a conversation is about to type in it. */
    useEffect(() => {
        if (open) composer.current?.focus();
    }, [open]);

    /* A conversation is read from the bottom, so it opens there and stays
       there as messages arrive. */
    useEffect(() => {
        const box = scroller.current;
        if (box) box.scrollTop = box.scrollHeight;
    }, [messages]);

    /* The search box, one request behind what has been typed. The pause is
       what keeps a name from being a request per keystroke. */
    useEffect(() => {
        const text = query.trim();
        if (!text) {
            setFound(null);
            return;
        }
        let cancelled = false;
        const wait = setTimeout(() => {
            searchUsers(text)
                .then((answer) => {
                    if (!cancelled) setFound(answer.users);
                })
                .catch(() => {
                    /* the box is a shortcut, not the page: a failed lookup
                       shows nothing rather than an error over the inbox */
                    if (!cancelled) setFound([]);
                });
        }, 200);
        return () => {
            cancelled = true;
            clearTimeout(wait);
        };
    }, [query]);

    function send(event: FormEvent | KeyboardEvent<HTMLTextAreaElement>) {
        event.preventDefault();
        const body = draft.trim();
        if (!body || !open) return;
        inbox.current?.send(open, body);
        /* cleared on sending rather than on the message coming back: the box
           is the learner's, and holding their typing hostage to a round trip
           is how a second copy gets sent */
        setDraft("");
        setError(null);
    }

    function key(event: KeyboardEvent<HTMLTextAreaElement>) {
        /* Enter sends, Shift+Enter is a new line — a message is usually one
           line, and reaching for a button to send it is not. */
        if (event.key === "Enter" && !event.shiftKey) send(event);
    }

    function pick(user: ApiFoundUser) {
        setQuery("");
        setFound(null);
        show(user.username, user.display);
    }

    function back() {
        openRef.current = null;
        setOpen(null);
        setMessages(null);
        window.history.replaceState(null, "", "/inbox");
    }

    if (!ready) return <div className="shell inbox-page" />;

    /* Signed out, or a guest: the same offer either way, because a guest is
       somebody without an account and messages are between accounts. */
    if (!signedIn) {
        return (
            <div className="shell inbox-page">
                <section className="panel inbox-offer">
                    <h1>Messages</h1>
                    <p>
                        Messages are between accounts, so there is somebody to
                        write back. Create one — everything you have done so
                        far comes with you.
                    </p>
                    <div className="inbox-offer-actions">
                        <a
                            className="bp6-button bp6-intent-primary"
                            href="/signup?next=%2Finbox"
                        >
                            Create account
                        </a>
                        <a className="bp6-button" href="/login?next=%2Finbox">
                            Sign in
                        </a>
                    </div>
                </section>
            </div>
        );
    }

    return (
        <div className="shell inbox-page">
            <section className={open ? "panel inbox is-open" : "panel inbox"}>
                <div className="inbox-list">
                    <div className="inbox-search">
                        <input
                            className="inbox-search-input"
                            type="search"
                            value={query}
                            placeholder="Search or start new conversation"
                            aria-label="Search or start new conversation"
                            onChange={(event) => setQuery(event.target.value)}
                        />
                        {found !== null && (
                            <ul className="inbox-results">
                                {found.length === 0 ? (
                                    <li className="inbox-result-empty">
                                        Nobody by that name
                                    </li>
                                ) : (
                                    found.map((person) => (
                                        <li key={person.username}>
                                            <button
                                                type="button"
                                                className="inbox-result"
                                                onClick={() => pick(person)}
                                            >
                                                <Avatar display={person.display} />
                                                <span className="inbox-result-name">
                                                    {person.display}
                                                </span>
                                                <span className="inbox-result-handle">
                                                    {person.username}
                                                </span>
                                            </button>
                                        </li>
                                    ))
                                )}
                            </ul>
                        )}
                    </div>

                    {threads.length === 0 ? (
                        <p className="inbox-note">
                            No messages yet. Search for somebody above to start
                            a conversation.
                        </p>
                    ) : (
                        <ul className="thread-list">
                            {threads.map((thread) => (
                                <ThreadRow
                                    key={thread.with}
                                    thread={thread}
                                    active={thread.with === open}
                                    now={now}
                                    me={me}
                                    onOpen={() => show(thread.with, thread.display)}
                                />
                            ))}
                        </ul>
                    )}
                </div>

                <div className="inbox-pane">
                    {open === null ? (
                        <p className="inbox-note pane-empty">
                            Pick a conversation to read it.
                        </p>
                    ) : (
                        <>
                            <header className="pane-head">
                                {/* only on a phone, where the list is the
                                    page this one replaced */}
                                <button
                                    type="button"
                                    className="pane-back"
                                    onClick={back}
                                    aria-label="Back to conversations"
                                >
                                    <Icon icon="chevron-left" size={16} />
                                </button>
                                <Avatar display={display} />
                                <a
                                    className="pane-name"
                                    href={`/u/${encodeURIComponent(open)}`}
                                >
                                    {display}
                                </a>
                            </header>

                            <div className="pane-messages" ref={scroller}>
                                {messages === null ? (
                                    <div className="pane-loading">
                                        <Spinner size={20} />
                                    </div>
                                ) : messages.length === 0 ? (
                                    <p className="inbox-note">
                                        Nothing here yet. Say hello.
                                    </p>
                                ) : (
                                    messages.map((value) => (
                                        <div
                                            key={value.id}
                                            className={
                                                value.from === me
                                                    ? "bubble is-mine"
                                                    : "bubble"
                                            }
                                        >
                                            <span className="bubble-body">
                                                {value.body}
                                            </span>
                                            <time className="bubble-time">
                                                {clockTime(value.sentAt)}
                                            </time>
                                        </div>
                                    ))
                                )}
                            </div>

                            {error && (
                                <p className="pane-error" role="alert">
                                    {error}
                                </p>
                            )}

                            <form className="pane-compose" onSubmit={send}>
                                <textarea
                                    ref={composer}
                                    className="pane-input"
                                    rows={1}
                                    value={draft}
                                    placeholder={
                                        live
                                            ? `Message ${display}`
                                            : "Reconnecting…"
                                    }
                                    aria-label={`Message ${display}`}
                                    disabled={!live}
                                    onChange={(event) =>
                                        setDraft(event.target.value)
                                    }
                                    onKeyDown={key}
                                />
                                <Button
                                    type="submit"
                                    intent="primary"
                                    icon="send-message"
                                    aria-label="Send"
                                    disabled={!live || draft.trim() === ""}
                                />
                            </form>
                        </>
                    )}
                </div>
            </section>
        </div>
    );
}
