/* The weekly leaderboard.

   The board is global and resets every Monday: the only thing it ranks is the
   XP earned between then and now. All of it comes from the backend
   (GET /leaderboard), which sums it out of the same day-by-day activity rows
   a profile is derived from — so a standing here and the XP on someone's
   profile can never disagree, and neither is stored.

   This file's job is the same as profile.ts's: put the response in the shape
   the table wants, and hold the couple of presentational decisions the
   backend has no opinion about — which row is "you", and how a week is
   spelled out for a human. */

import type { ApiLeaderboard, ApiStanding } from './api';

/** the API dates everything in unix seconds; the UI works in milliseconds */
const ms = (seconds: number) => seconds * 1000;

export interface WeeklyEntry {
  /** competition rank from the backend: a tie shares a place, so this is
      *not* the row's index and two rows may carry the same number */
  rank: number;
  username: string;
  /** what to print — their display name, or the username behind it */
  name: string;
  /** XP earned since Monday 00:00 UTC */
  xp: number;
  /** the signed-in reader's own row, highlighted in the table */
  you?: boolean;
}

export interface Weekly {
  /** the Monday the week began and the Monday it empties on, in ms */
  weekStart: number;
  resetsAt: number;
  entries: WeeklyEntry[];
  /** whether the reader is signed in but hasn't earned anything this week —
      the board can't show them a row, so the page says so instead of leaving
      them wondering where they are */
  missing: boolean;
}

/**
 * Shape one response for the table.
 *
 * `viewer` is the signed-in username or null. Marking "you" is done here
 * rather than by the backend because the client already knows who it is: the
 * board reads the same for everybody, which keeps the endpoint public and
 * means two readers can be handed the identical response.
 */
export function weeklyFrom(
  api: ApiLeaderboard,
  viewer: string | null,
): Weekly {
  const entries = api.standings.map((standing: ApiStanding) => ({
    rank: standing.rank,
    username: standing.username,
    name: standing.display,
    xp: standing.xp,
    ...(standing.username === viewer ? { you: true as const } : {}),
  }));
  return {
    weekStart: ms(api.week_start),
    resetsAt: ms(api.resets_at),
    entries,
    missing: viewer !== null && !entries.some((entry) => entry.you),
  };
}
