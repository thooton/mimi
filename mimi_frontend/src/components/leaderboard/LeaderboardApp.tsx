import { useEffect, useState } from "react";
import { fetchLeaderboard } from "../../data/api";
import { useAuth } from "../../data/auth";
import type { Weekly } from "../../data/leaderboard";
import { weeklyFrom } from "../../data/leaderboard";
import { formatDate } from "../../data/profile";
import { formatXp } from "../../data/social";

/* The leaderboard tab's one React root: a single global board, read top to
   bottom, ranking individual learners by the XP they have earned since
   Monday. Nothing here is sorted or scored client-side — the backend serves
   the standings already ranked, because a tie shares a place and there should
   only be one opinion about what that means.

   The page is prerendered and the board fetched in the browser, the way the
   profile fetches its record: it changes every time anybody finishes a
   lesson, so there is no build at which it could be baked in. */

function WeeklyPanel({ weekly }: { weekly: Weekly }) {
    return (
        <section className="panel">
            <div className="panel-head">
                <h2 className="eyebrow panel-title">Individual Leaderboard</h2>
                <span className="eyebrow">
                    Resets {formatDate(weekly.resetsAt)} 00:00 UTC
                </span>
            </div>
            <div className="board-body">
                {weekly.entries.length === 0 ? (
                    /* a brand-new week, before anybody has finished a lesson
                       in it — an empty board is the honest answer, and it
                       fills itself in within the hour */
                    <p className="board-note">
                        Nobody has earned XP yet this week
                    </p>
                ) : (
                    <table className="board-table board-table--weekly">
                        <thead>
                            <tr>
                                <th className="col-rank">#</th>
                                <th>Player</th>
                                <th className="col-xp">XP this week</th>
                            </tr>
                        </thead>
                        <tbody>
                            {weekly.entries.map((entry) => (
                                <tr
                                    key={entry.username}
                                    className={
                                        entry.you ? "board-row-you" : undefined
                                    }
                                >
                                    <td className="col-rank">
                                        <span
                                            className={
                                                entry.rank <= 3
                                                    ? `rank rank--${entry.rank}`
                                                    : "rank"
                                            }
                                        >
                                            {entry.rank}
                                        </span>
                                    </td>
                                    <td>
                                        <span className="player">
                                            <a
                                                className="player-name"
                                                href={`/u/${encodeURIComponent(entry.username)}`}
                                            >
                                                {entry.name}
                                            </a>
                                            {entry.you && (
                                                <span className="eyebrow you-badge">
                                                    you
                                                </span>
                                            )}
                                        </span>
                                    </td>
                                    <td className="col-xp">
                                        {formatXp(entry.xp)}
                                    </td>
                                </tr>
                            ))}
                        </tbody>
                    </table>
                )}

                {/* Only for a signed-in learner who really has earned nothing
                    this week — a guest is told nothing at all, because being
                    lectured about not counting is not what somebody trying
                    the course out came here for. */}
                {weekly.missing && (
                    <p className="board-note">
                        You haven't earned any XP this week yet.
                    </p>
                )}
            </div>
        </section>
    );
}

export default function LeaderboardApp() {
    const { user, ready } = useAuth();
    const [weekly, setWeekly] = useState<Weekly | null>(null);
    const [error, setError] = useState<string | null>(null);

    /* The board is public, but which row is "you" depends on the viewer, so
       the fetch waits for the session to settle rather than rendering the
       table twice — once anonymous, once with a row highlighted.

       A guest reads the board as an anonymous visitor: they are never on it,
       so passing their name would only ever produce "you aren't here". */
    const viewer = user && !user.guest ? user.username : null;

    useEffect(() => {
        if (!ready) return;
        let cancelled = false;
        fetchLeaderboard()
            .then((api) => {
                if (!cancelled) setWeekly(weeklyFrom(api, viewer));
            })
            .catch((e) => {
                if (!cancelled)
                    setError(e instanceof Error ? e.message : String(e));
            });
        return () => {
            cancelled = true;
        };
    }, [ready, viewer]);

    if (error) {
        return (
            <section className="panel">
                <div className="board-body">
                    <p className="board-note">{error}</p>
                </div>
            </section>
        );
    }

    /* while the board is in flight the panel is simply absent — no spinner,
       no skeleton, matching the profile page */
    if (!weekly) return null;

    return <WeeklyPanel weekly={weekly} />;
}
