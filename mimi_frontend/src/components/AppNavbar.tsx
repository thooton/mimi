import { useEffect, useRef, useState } from "react";
import type { Ref } from "react";
import {
    Icon,
    Menu,
    MenuDivider,
    MenuItem,
    Popover,
    Tooltip,
} from "@blueprintjs/core";
import { ShareIcon } from "@blueprintjs/icons";
import DiscordMark from "./DiscordMark";
import Flame from "./Flame";
import LanguagePicker from "./LanguagePicker";
import MimiDog from "./MimiDog";
import { fetchProfile, keepAlive } from "../data/api";
import { useAuth } from "../data/auth";
import { onProfileEdit, safeAvatar } from "../data/profile";
import { DISCORD_URL, EDITOR_URL } from "../data/site";

/** Compact labels keep the navigation legible beside Mimi's photo mark. */
const NAV = [
    { id: "learn", href: "/learn", label: "Learn" },
    { id: "practice", label: "Practice" },
    { id: "leaderboard", href: "/leaderboard", label: "Leaderboard" },
    { id: "editor", href: EDITOR_URL, label: "Editor", external: true },
] as const;

/* Practice is a menu rather than a page, with one working mode for now. */
const MENUS: Record<string, { href: string; label: string }[]> = {
    practice: [{ href: "/practice/flashcards", label: "Flashcards" }],
};

interface Props {
    active?: string;
}

// Half the server's 30-second presence window. This timer belongs to the
// navbar because it is the one client island mounted exactly once on every
// page: every tab pings, without every component using auth starting one.
const KEEP_ALIVE_MS = 15_000;

/** an auth page that comes back to whatever the guest was looking at */
function guestNext(path: string): string {
    const here =
        typeof window === "undefined" ? "/learn" : window.location.pathname;
    return `${path}?next=${encodeURIComponent(here)}`;
}

/* The signed-in account, from the same endpoint the profile page reads, so
   the bar and the page can't disagree about the streak. It arrives after the
   first paint — the bar is on every page and must not wait for it — so until
   it lands the right side of the bar renders nothing: no flame beside a
   guessed 0, no name that might be wrong.

   `settled` flips when the request finishes either way, and it's what the
   whole right side waits on: the flag's own fetch (the same profile, via
   courseSelection.ts) lands at about the same time, and showing the bar piecemeal
   made it reveal in two pops — flag first, then the counters flashing in
   around it. Holding everything for the one signal makes the three appear
   in a single paint. A profile that won't load still settles, so the flag
   appears without the counters rather than not at all. */
function useViewer(username?: string) {
    const [state, setState] = useState<{
        me: { display: string; streak: number; avatar?: string } | null;
        settled: boolean;
    }>({ me: null, settled: false });
    useEffect(() => {
        let cancelled = false;
        if (!username) {
            setState({ me: null, settled: true });
            return;
        }
        /* Only the first read blanks the bar. A re-read after an edit is a
       redraw of a name and a picture that are already there, and blanking
       for it would make the bar flicker every time somebody saves. */
        let first = true;
        const load = () => {
            if (first) setState({ me: null, settled: false });
            fetchProfile(username)
                .then((profile) => {
                    if (cancelled) return;
                    first = false;
                    setState({
                        me: {
                            display: profile.display,
                            streak: profile.streak,
                            /* checked here as everywhere else it is rendered: the
                 backend stored it, but this is still a URL one person
                 typed and every other person's browser fetches */
                            avatar: safeAvatar(profile.avatar),
                        },
                        settled: true,
                    });
                })
                .catch(() => {
                    /* the bar is chrome; a missing profile isn't worth an error here */
                    if (!cancelled && first)
                        setState({ me: null, settled: true });
                });
        };
        load();
        /* the picture in the corner is the same picture the profile editor
       changes, so it re-reads when that happens rather than staying wrong
       until the next navigation */
        const stop = onProfileEdit(load);
        return () => {
            cancelled = true;
            stop();
        };
    }, [username]);
    return state;
}

export default function AppNavbar({ active = "" }: Props) {
    const { user, ready, signOut } = useAuth();
    const { me, settled } = useViewer(user?.username);

    useEffect(() => {
        if (!ready || !user) return;
        let pending = false;
        const ping = () => {
            if (pending) return;
            pending = true;
            keepAlive()
                // Presence is best-effort chrome. The page's real request will show
                // its own error if the server or session is unavailable.
                .catch(() => undefined)
                .finally(() => {
                    pending = false;
                });
        };
        ping();
        const interval = window.setInterval(ping, KEEP_ALIVE_MS);
        return () => window.clearInterval(interval);
    }, [ready, user?.username]);
    /* A guest is a learner in every way that shows here — they have a streak
     and a language — so the bar carries all of it, and only swaps the
     account menu for the offer to keep it. There is nothing behind /profile
     for `guest~…`, and no account to sign out of. */
    const guest = ready && user?.guest === true;

    /* Under the hamburger breakpoint the horizontal nav collapses into this
     button's panel (see the bottom of chrome.css). The swap is done purely
     in CSS, and the panel's styles only take effect inside the same media
     query that reveals the button — so widening the window while it's open
     can't strand it, visible and unstyled, on the desktop bar. */
    const [menuOpen, setMenuOpen] = useState(false);
    const barRef = useRef<HTMLElement>(null);
    const menuBtnRef = useRef<HTMLButtonElement>(null);

    /* While the panel is open it answers to the world outside it the way a
     disclosure should: Escape closes it and returns focus to the button,
     and a tap anywhere off the bar dismisses it. The links inside need no
     close handler of their own — internal ones navigate away, and the one
     that doesn't (the editor, in a new tab) closes it explicitly. */
    useEffect(() => {
        if (!menuOpen) return;
        const onKey = (event: KeyboardEvent) => {
            if (event.key === "Escape") {
                setMenuOpen(false);
                menuBtnRef.current?.focus();
            }
        };
        const onPointer = (event: PointerEvent) => {
            if (
                barRef.current &&
                !barRef.current.contains(event.target as Node)
            )
                setMenuOpen(false);
        };
        document.addEventListener("keydown", onKey);
        document.addEventListener("pointerdown", onPointer);
        return () => {
            document.removeEventListener("keydown", onKey);
            document.removeEventListener("pointerdown", onPointer);
        };
    }, [menuOpen]);

    return (
        <header className="topbar" ref={barRef}>
            <div className="shell topbar-inner">
                <a className="brand" href="/">
                    <MimiDog width={34} alt="" />
                    <span className="brand-name">mimi</span>
                </a>

                <nav className="topnav" aria-label="Main">
                    {NAV.map((item) => {
                        const isActive = active === item.id;
                        const cls = isActive
                            ? "topnav-link is-active"
                            : "topnav-link";

                        /* Practice has no page of its own, so the tab is a button that
               opens its subsections in a menu below — on hover,
               on focus, or on click (a tap on a touch screen counts as a
               hover). It's a button because it navigates nowhere itself. */
                        if (item.id === "practice") {
                            const menu = MENUS[item.id];
                            return (
                                <Popover
                                    key={item.id}
                                    interactionKind="hover"
                                    placement="bottom-start"
                                    minimal
                                    /* as wide as the tab, dropping from the bar's own bottom
                       edge and overlapping its hairline — an extension of
                       the bar, not a card floating beneath it (square
                       corners come from .nav-menu in styles/chrome.css) */
                                    matchTargetWidth
                                    /* The tab is a 36px pill centred in the 52px bar, so it
                     floats 8px clear of the bar's bottom edge. The menu
                     hangs off that edge, not off the pill, so it clears the
                     gap and then pulls back 1px onto the hairline:
                     (52 − 36) / 2 − 1.

                     `enabled` is not redundant. Blueprint ties the offset
                     modifier to the arrow — `enabled: isArrowEnabled()`,
                     which is false whenever `minimal` is set — so on a
                     minimal popover the offset is switched off and its
                     options are read into a modifier that never runs. This
                     prop carried [0, -1] for that reason without ever
                     moving the menu a pixel. */
                                    modifiers={{
                                        offset: {
                                            enabled: true,
                                            options: { offset: [0, 7] },
                                        },
                                    }}
                                    /* Blueprint's defaults feel like they hang around: no hover
                     delays, and the fade matches the css's own 100ms */
                                    hoverOpenDelay={0}
                                    hoverCloseDelay={0}
                                    transitionDuration={100}
                                    popoverClassName="nav-menu nav-menu--wide"
                                    /* `isOpen` drives the lit state rather than the
                     aria-expanded the stylesheet used to key off: Blueprint
                     sets that attribute to undefined for hover popovers
                     (a hover target isn't a disclosure), so the rule could
                     never match and the tab stayed dark under its own menu. */
                                    renderTarget={({
                                        isOpen,
                                        ref,
                                        ...targetProps
                                    }) => (
                                        <button
                                            {...targetProps}
                                            ref={ref as Ref<HTMLButtonElement>}
                                            type="button"
                                            className={
                                                isOpen ? `${cls} is-open` : cls
                                            }
                                            aria-current={
                                                isActive ? "page" : undefined
                                            }
                                        >
                                            {item.label}
                                        </button>
                                    )}
                                    content={
                                        <Menu
                                            role="menu"
                                            aria-label={item.label}
                                        >
                                            {menu.map((sub) => (
                                                <MenuItem
                                                    key={sub.label}
                                                    href={sub.href}
                                                    text={sub.label}
                                                />
                                            ))}
                                        </Menu>
                                    }
                                />
                            );
                        }

                        return (
                            <a
                                key={item.id}
                                className={cls}
                                href={item.href}
                                target={
                                    "external" in item && item.external
                                        ? "_blank"
                                        : undefined
                                }
                                rel={
                                    "external" in item && item.external
                                        ? "noopener noreferrer"
                                        : undefined
                                }
                                aria-current={isActive ? "page" : undefined}
                            >
                                {item.label}
                                {"external" in item && item.external && (
                                    /* The generic Icon renders its font fallback during SSR,
                     then replaces it with an SVG after its paths load in the
                     browser. At this non-standard 11px size the fallback
                     inherits the nav's 14px type, so it visibly shrinks on
                     hydration. The static component has its paths up front
                     and renders the same 11px SVG on both sides. */
                                    <ShareIcon
                                        className="topnav-external"
                                        size={11}
                                        aria-hidden
                                    />
                                )}
                            </a>
                        );
                    })}
                </nav>

                <div className="topbar-right">
                    {/* one reveal, not three: the streak, the flag and the account
              button all wait on the profile request (the picker's own data
              is ready far earlier) and appear in the same paint.

              A guest has no streak. Days in a row is a thing an account
              keeps; offering one to a record that lives in a cookie would be
              promising something we can't hold on to. */}
                    {/* The community, to the left of the streak. It waits on nothing —
              the link is the same for everyone, signed in or not — so unlike
              its neighbours it is in the bar from the first paint. */}
                    <Tooltip content="Join us on Discord" placement="bottom">
                        <a
                            className="discord-link"
                            href={DISCORD_URL}
                            target="_blank"
                            rel="noopener noreferrer"
                            aria-label="Join us on Discord"
                        >
                            <DiscordMark size={18} />
                        </a>
                    </Tooltip>

                    {ready && user && me && !guest && (
                        <Tooltip
                            content={`${me.streak} days in a row`}
                            placement="bottom"
                        >
                            <div
                                className="streak"
                                aria-label={`${me.streak} day streak`}
                            >
                                <Flame size={20} />
                                <span className="streak-count">
                                    {me.streak}
                                </span>
                            </div>
                        </Tooltip>
                    )}

                    {/* stays mounted while hidden, so its profile fetch has long
              finished by the time `visible` lets it render */}
                    {ready && user && <LanguagePicker visible={settled} />}

                    {/* The same pair a signed-out visitor gets, because that is what a
              guest is: somebody without an account. A guest is now made
              automatically on the way into the course, so this is also the
              only signpost back for a returning learner who simply hasn't
              signed in yet — they must not have to guess. */}
                    {guest && settled && (
                        <div className="topbar-auth">
                            <a
                                className="bp6-button"
                                href={guestNext("/login")}
                            >
                                Sign in
                            </a>
                            <a
                                className="bp6-button bp6-intent-primary"
                                href={guestNext("/signup")}
                            >
                                Create account
                            </a>
                        </div>
                    )}

                    {ready && user && me && !guest && (
                        <Popover
                            placement="bottom-end"
                            minimal
                            content={
                                <Menu>
                                    <MenuItem
                                        icon="person"
                                        text="Profile"
                                        href="/profile"
                                    />
                                    {/* Messages are between accounts, so the way in is the
                      account menu rather than a tab of its own: an inbox is
                      one person's, unlike everything on the bar. */}
                                    <MenuItem
                                        icon="envelope"
                                        text="Inbox"
                                        href="/inbox"
                                    />
                                    <MenuItem
                                        icon="cog"
                                        text="Settings"
                                        href="/settings"
                                    />
                                    <MenuDivider />
                                    <MenuItem icon="help" text="Help" />
                                    <MenuItem
                                        icon="log-out"
                                        text="Sign out"
                                        onClick={() => {
                                            signOut().finally(() =>
                                                window.location.assign(
                                                    "/login",
                                                ),
                                            );
                                        }}
                                    />
                                </Menu>
                            }
                        >
                            <button className="account-btn" type="button">
                                <span className="avatar" aria-hidden="true">
                                    {me.avatar ? (
                                        <img
                                            src={me.avatar}
                                            alt=""
                                            referrerPolicy="no-referrer"
                                        />
                                    ) : (
                                        me.display.charAt(0)
                                    )}
                                </span>
                                {/* in a span of its own so small phones can shed it — the
                    menu the avatar opens still says who's signed in */}
                                <span className="account-name">
                                    {me.display}
                                </span>
                                <Icon
                                    className="caret"
                                    icon="caret-down"
                                    size={12}
                                />
                            </button>
                        </Popover>
                    )}

                    {ready && !user && (
                        <div className="topbar-auth">
                            <a className="bp6-button" href="/login">
                                Sign in
                            </a>
                            <a
                                className="bp6-button bp6-intent-primary"
                                href="/signup"
                            >
                                Create account
                            </a>
                        </div>
                    )}

                    {/* last in the bar, so on a phone it lands under the thumb at the
              right edge */}
                    <button
                        ref={menuBtnRef}
                        className="menu-btn"
                        type="button"
                        aria-expanded={menuOpen}
                        aria-controls="mobilenav"
                        aria-label={menuOpen ? "Close menu" : "Open menu"}
                        onClick={() => setMenuOpen((open) => !open)}
                    >
                        <Icon
                            icon={menuOpen ? "cross" : "menu"}
                            size={18}
                            aria-hidden
                        />
                    </button>
                </div>
            </div>

            {menuOpen && (
                <nav className="mobilenav" id="mobilenav" aria-label="Main">
                    {NAV.map((item) => {
                        /* Practice is a heading here, not a link: on the desktop bar its
               tab opens a hover menu, a gesture touch screens don't have —
               so the panel simply lists its subsections underneath. */
                        if (item.id === "practice") {
                            const isActive = active === item.id;
                            return (
                                <div className="mobilenav-group" key={item.id}>
                                    <span className="mobilenav-heading">
                                        {item.label}
                                    </span>
                                    {MENUS[item.id].map((sub) => (
                                        <a
                                            key={sub.label}
                                            className={
                                                isActive
                                                    ? "mobilenav-link mobilenav-sublink is-active"
                                                    : "mobilenav-link mobilenav-sublink"
                                            }
                                            href={sub.href}
                                            aria-current={
                                                isActive ? "page" : undefined
                                            }
                                        >
                                            {sub.label}
                                        </a>
                                    ))}
                                </div>
                            );
                        }

                        const isActive = active === item.id;
                        const external = "external" in item && item.external;
                        return (
                            <a
                                key={item.id}
                                className={
                                    isActive
                                        ? "mobilenav-link is-active"
                                        : "mobilenav-link"
                                }
                                href={item.href}
                                target={external ? "_blank" : undefined}
                                rel={
                                    external ? "noopener noreferrer" : undefined
                                }
                                aria-current={isActive ? "page" : undefined}
                                /* opens in a new tab, so nothing would otherwise dismiss
                   the panel underneath */
                                onClick={
                                    external
                                        ? () => setMenuOpen(false)
                                        : undefined
                                }
                            >
                                {item.label}
                                {external && (
                                    <ShareIcon
                                        className="topnav-external"
                                        size={11}
                                        aria-hidden
                                    />
                                )}
                            </a>
                        );
                    })}

                    {/* The bar's Discord icon, folded in as a row of the nav it now
              sits under. It is a link out like the editor above it, so it
              behaves like one — new tab, and it closes the panel on the way
              since nothing else will. (The bar hides its own copy at this
              breakpoint; see .mobilenav-discord in chrome.css.) */}
                    <a
                        className="mobilenav-link mobilenav-discord"
                        href={DISCORD_URL}
                        target="_blank"
                        rel="noopener noreferrer"
                        onClick={() => setMenuOpen(false)}
                    >
                        <DiscordMark size={16} />
                        Discord
                        <ShareIcon
                            className="topnav-external"
                            size={11}
                            aria-hidden
                        />
                    </a>

                    {/* The bar's auth pair, folded into the foot of the panel — the
              bar hides it at the same breakpoint (two buttons never fit a
              phone next to the flag). Same split as the bar: a guest's
              links remember where they were, a signed-out visitor's don't
              need to. The streak, flag and avatar stay in the bar: they
              answer "how am I doing", "what am I learning" and "who am I",
              questions the panel can't. */}
                    {guest && settled && (
                        <div className="mobilenav-auth">
                            <a
                                className="bp6-button"
                                href={guestNext("/login")}
                            >
                                Sign in
                            </a>
                            <a
                                className="bp6-button bp6-intent-primary"
                                href={guestNext("/signup")}
                            >
                                Create account
                            </a>
                        </div>
                    )}

                    {ready && !user && (
                        <div className="mobilenav-auth">
                            <a className="bp6-button" href="/login">
                                Sign in
                            </a>
                            <a
                                className="bp6-button bp6-intent-primary"
                                href="/signup"
                            >
                                Create account
                            </a>
                        </div>
                    )}
                </nav>
            )}
        </header>
    );
}
