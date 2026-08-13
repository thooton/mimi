import { useEffect, useState } from "react";
import { Button } from "@blueprintjs/core";
import type { ApiProfile } from "../../data/api";
import { fetchProfile, followUser, unfollowUser } from "../../data/api";
import { useAuth } from "../../data/auth";
import type { Profile } from "../../data/profile";
import {
    activityFrom,
    announceProfileEdit,
    formatDate,
    profileFrom,
} from "../../data/profile";
import { formatXp } from "../../data/social";
import Flame from "../Flame";
import EditProfileDialog from "./EditProfileDialog";
import LanguageCards from "./LanguageCards";
import ScoreChart from "./ScoreChart";
import PracticeStrip from "./PracticeStrip";
import ActivityFeed from "./ActivityFeed";

/* The public profile page, read top to bottom: who this is, the languages
   they're learning and how those scores have moved, the practice behind them,
   and then the day-by-day record. Anyone can open anyone's — the viewer's
   identity only decides the row of buttons. Social actions belong to an
   account, so a signed-out visitor (including a temporary guest record) sees
   the profile without controls they cannot use. A profile is never scored
   for how filled-in it is; blank fields are a choice, not a chore.

   The page is prerendered and the profile is fetched in the browser, the way
   the learn page fetches its course: the record changes every time its owner
   finishes a lesson, so there is no build at which it could be baked in. */

/* One figure from the totals. The streak is the only live one — it can break
   tomorrow — so it carries the flame and the accent; the rest sit plain. */
function Counter({
    value,
    label,
    tone,
}: {
    value: number;
    label: string;
    tone?: "streak";
}) {
    return (
        <div className={tone ? `counter counter--${tone}` : "counter"}>
            <span className="counter-value">
                {tone === "streak" && <Flame size={16} />}
                {/* the figure carries its own box so the row can centre the
                    digits rather than the line they sit on — see
                    .counter-number */}
                <span className="counter-number">{formatXp(value)}</span>
            </span>
            <span className="eyebrow counter-label">{label}</span>
        </div>
    );
}

export default function ProfileApp({ username }: { username?: string }) {
    const { user, ready } = useAuth();
    const viewedUsername = username ?? user?.username;
    const [api, setApi] = useState<ApiProfile | null>(null);
    const [error, setError] = useState<string | null>(null);
    /* a follow or an edit that didn't land. Kept apart from `error` above,
       which is the page failing to load: this one leaves the profile on
       screen and says only that the button didn't work. */
    const [actionError, setActionError] = useState<string | null>(null);
    const [following, setFollowing] = useState(false);
    const [editing, setEditing] = useState(false);
    // shared between the cards and the graph: a card is the graph's toggle
    const [hidden, setHidden] = useState<string[]>([]);
    const toggle = (id: string) =>
        setHidden((h) =>
            h.includes(id) ? h.filter((x) => x !== id) : [...h, id],
        );

    useEffect(() => {
        let cancelled = false;
        let loaded = false;
        let pending = false;
        setApi(null);
        setError(null);
        setActionError(null);
        if (!viewedUsername) return;
        const load = () => {
            if (pending) return;
            pending = true;
            fetchProfile(viewedUsername)
                .then((profile) => {
                    if (cancelled) return;
                    loaded = true;
                    setApi(profile);
                    setError(null);
                })
                .catch((e) => {
                    // Once a profile is on screen, a missed presence poll is
                    // not a reason to replace the whole page with an error.
                    if (!cancelled && !loaded)
                        setError(e instanceof Error ? e.message : String(e));
                })
                .finally(() => {
                    pending = false;
                });
        };
        load();
        // Presence is a rolling 30-second fact. Re-reading halfway through
        // that window lets the dot turn both on and off while this page is
        // open, without inventing a second presence-only wire format.
        const interval = window.setInterval(load, 15_000);
        return () => {
            cancelled = true;
            window.clearInterval(interval);
        };
    }, [viewedUsername]);

    /* Read the profile again after something changed it, *without* blanking
       the page first: the record is already on screen and this is a redraw of
       it, not a new page. Everything a follow or an edit touches — the
       counters, the button, the feed entry the follow just wrote — comes back
       in the one response, so there is nothing to patch by hand and nothing
       that can drift from what the server thinks. */
    async function reload(username: string) {
        setApi(await fetchProfile(username));
    }

    async function toggleFollow() {
        if (!api) return;
        setActionError(null);
        setFollowing(true);
        try {
            if (api.viewer_follows) await unfollowUser(api.username);
            else await followUser(api.username);
            await reload(api.username);
        } catch (e) {
            const message = e instanceof Error ? e.message : String(e);
            setActionError(
                message.includes(": ")
                    ? message.split(": ").slice(1).join(": ")
                    : message,
            );
        } finally {
            setFollowing(false);
        }
    }

    if (!ready) {
        return <div className="shell profile-page" />;
    }

    if (!viewedUsername) {
        return (
            <div className="shell profile-page">
                <section className="panel profile-empty">
                    <h1 className="profile-name">Sign in to view your profile</h1>
                    <p className="profile-bio"><a href="/login">Sign in</a> or <a href="/signup">create an account</a>.</p>
                </section>
            </div>
        );
    }

    if (error) {
        return (
            <div className="shell profile-page">
                <section className="panel profile-empty">
                    <h1 className="profile-name">{viewedUsername}</h1>
                    <p className="profile-bio">{error}</p>
                </section>
            </div>
        );
    }

    /* while the profile is in flight the page is simply blank — no spinner,
       no skeleton; the record appears whole once it arrives */
    if (!api) {
        return <div className="shell profile-page" />;
    }

    const profile: Profile = profileFrom(api);
    const isYou = profile.username === user?.username;
    const hasAccount = user !== null && !user.guest;

    return (
        <div className="shell profile-page">
            {/* --- identity band --- */}
            <section className="panel profile-head">
                {/* A linked picture or the initial behind it. The image is
                    somebody else's file on somebody else's server (see
                    safeAvatar), so it is loaded without a referrer — which
                    profile a reader is looking at is not that host's
                    business — and it is decoration either way, because the
                    name it belongs to is right beside it. */}
                <span className="profile-avatar" aria-hidden="true">
                    {profile.avatar ? (
                        <img
                            className="profile-avatar-image"
                            src={profile.avatar}
                            alt=""
                            referrerPolicy="no-referrer"
                        />
                    ) : (
                        profile.display.charAt(0)
                    )}
                </span>

                <div className="profile-id">
                    <h1 className="profile-name">
                        <span
                            className={
                                profile.online
                                    ? "presence is-online"
                                    : "presence"
                            }
                            title={profile.online ? "Online" : "Offline"}
                        />
                        {profile.title && (
                            <span className="eyebrow profile-title">
                                {profile.title}
                            </span>
                        )}
                        {profile.display}
                    </h1>
                    <p className="profile-bio">{profile.bio}</p>
                    <dl className="profile-facts">
                        <div>
                            <dt>Joined</dt>
                            <dd>{formatDate(profile.memberSince)}</dd>
                        </div>
                        <div>
                            <dt>Active</dt>
                            <dd>{profile.lastActive}</dd>
                        </div>
                        <div>
                            <dt>Days studied</dt>
                            <dd>{formatXp(profile.daysActive)}</dd>
                        </div>
                        <div>
                            <dt>Followers</dt>
                            <dd>{formatXp(profile.followers)}</dd>
                        </div>
                        <div>
                            <dt>Following</dt>
                            <dd>{formatXp(profile.following)}</dd>
                        </div>
                    </dl>
                </div>

                <div className="profile-side">
                    {hasAccount && (
                        <div className="profile-actions">
                            {isYou ? (
                                <Button
                                    icon="edit"
                                    text="Edit"
                                    onClick={() => setEditing(true)}
                                />
                            ) : (
                                <>
                                    {/* One button, two states: following
                                        somebody and stopping are the same
                                        decision seen from either side, and
                                        the pressed state is what says which
                                        way round it currently is. */}
                                    <Button
                                        intent={
                                            api.viewer_follows
                                                ? undefined
                                                : "primary"
                                        }
                                        icon="following"
                                        text={
                                            api.viewer_follows
                                                ? "Following"
                                                : "Follow"
                                        }
                                        active={api.viewer_follows}
                                        disabled={following}
                                        onClick={toggleFollow}
                                    />
                                    {/* the inbox opens on this conversation,
                                        which may not exist yet — a thread is
                                        the pair of people in it, so there is
                                        nothing to create first */}
                                    <Button
                                        icon="chat"
                                        text="Message"
                                        onClick={() =>
                                            window.location.assign(
                                                `/inbox?with=${encodeURIComponent(api.username)}`,
                                            )
                                        }
                                    />
                                </>
                            )}
                        </div>
                    )}

                    {actionError && (
                        <p className="profile-action-error" role="alert">
                            {actionError}
                        </p>
                    )}

                    <div className="profile-counters">
                        <Counter value={profile.totals.xp} label="XP" />
                        <Counter
                            value={profile.totals.streak}
                            label="Day streak"
                            tone="streak"
                        />
                        <Counter
                            value={profile.totals.lessons}
                            label="Lessons"
                        />
                        <Counter
                            value={profile.totals.words}
                            label="Words"
                        />
                    </div>
                </div>
            </section>

            {/* --- languages + the graph they drive --- */}
            <section className="panel profile-progress">
                <div className="panel-head">
                    <h2 className="eyebrow panel-title">Languages</h2>
                </div>
                <div className="profile-progress-body">
                    <LanguageCards
                        languages={profile.languages}
                        hidden={hidden}
                        onToggle={toggle}
                    />
                    <ScoreChart
                        languages={profile.languages}
                        hidden={hidden}
                        today={profile.today}
                    />
                </div>
            </section>

            <PracticeStrip practice={profile.practice} />

            {/* --- the record --- */}
            <section className="profile-activity">
                <h2 className="eyebrow profile-activity-title">Activity</h2>
                <ActivityFeed days={activityFrom(api)} />
            </section>

            {isYou && (
                <EditProfileDialog
                    profile={profile}
                    isOpen={editing}
                    onClose={() => setEditing(false)}
                    onSaved={() => {
                        /* the navbar draws the same name and picture from a
                           fetch of its own; this is how it hears */
                        announceProfileEdit();
                        reload(profile.username).catch((e) =>
                            setActionError(
                                e instanceof Error ? e.message : String(e),
                            ),
                        );
                    }}
                />
            )}
        </div>
    );
}
