import type { IconName } from '@blueprintjs/icons';
import type { ApiProfile } from './api.ts';
import { languageName } from './languages.ts';

/* The public profile.

   A language gets a score, the way Duolingo gives one per course: it is the
   only way to put "two years of Spanish" and "six weeks of French" on the
   same axis, which is what the graph needs. Everything else the user does —
   words reviewed, skills cleared — is reported as itself, because a raw count
   is already the honest answer there and a score would only obscure it.

   All of it comes from the backend now (GET /users/:name/profile). The score
   and its history are computed there, from a table of what the user did on
   each day; this file's job is to put that in the shapes the components below
   it want, and to hold the handful of decorative things the backend has no
   opinion about yet — see DECOR at the bottom. */

export const DAY_MS = 86_400_000;

/** what the user has got through in a language — the raw material of a score */
export interface LanguageCounts {
  /** individual things known: words, endings, particles, sounds */
  words: number;
  /** skills cleared end to end */
  skills: number;
  lessons: number;
}

/** one sample of a language's score */
export interface Point {
  t: number;
  v: number;
}

export interface Language {
  id: string;
  name: string;
  /** the rail badge: two letters, or a sample of the script where that is
      the more useful thing to show */
  glyph: string;
  /** stroke on the graph */
  color: string;
  counts: LanguageCounts;
  score: number;
  /** too little evidence for the score to be worth trusting yet */
  provisional: boolean;
  /** movement over the last week, in points */
  delta: number;
  since: number;
  /** the score's history, ending today */
  points: Point[];
}

/** Practice that isn't tied to one course. No score: how many words you have
    been through is a better answer than any number derived from it. */
export interface Practice {
  id: string;
  name: string;
  icon: IconName;
  count: number;
  noun: string;
}

export interface Profile {
  username: string;
  display: string;
  /** a badge in front of the name */
  title?: string;
  bio: string;
  /** the outside standard the scores are anchored against; self-reported,
      so it is blank until someone says otherwise */
  cefr: string;
  /** their picture, if they have linked one that is safe to load — an
      absolute https URL on somebody else's server (see safeAvatar). Undefined
      is the ordinary case, and the initial is drawn instead. */
  avatar?: string;
  /** how many accounts follow this one, and how many it follows */
  followers: number;
  following: number;
  /** whether the reader follows this person — the state of the button */
  viewerFollows: boolean;
  memberSince: number;
  /** live server presence, independent of whether they studied today */
  online: boolean;
  /** "Today", "Yesterday", "12 days ago" — day resolution, because that is
      the resolution the record is kept at */
  lastActive: string;
  /** days they have been active at all */
  daysActive: number;
  languages: Language[];
  practice: Practice[];
  totals: {
    xp: number;
    streak: number;
    lessons: number;
    words: number;
  };
  /** midnight UTC today, as the *server* reckons it: the graph's right-hand
      edge, and what every relative date on the page is measured from. Taken
      from the response rather than the browser so that a reader in another
      timezone reads the same record the writer wrote. */
  today: number;
}

/* ------------------------------------------------------- where a profile is */

/**
 * The username out of a `/u/<name>` path, or null if the path carries none.
 *
 * Public profiles are one prerendered page serving every name: the host
 * rewrites `/u/<name>` onto `/u/`, so by the time the page runs, the address
 * bar is the only place the name still exists (see astro.config.mjs). This is
 * the parser for it, kept here rather than in the component so it can be
 * tested as what it is — a pure function over a string.
 */
export function usernameFromPath(pathname: string): string | null {
  const match = /^\/u\/([^/]+)\/?$/.exec(pathname);
  if (!match) return null;
  try {
    return decodeURIComponent(match[1]);
  } catch {
    /* a malformed percent-escape is not a username */
    return null;
  }
}

/* ------------------------------------------------------------------ dates */

const MONTHS = [
  'Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun',
  'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec',
];

/** "7 May 2024" — hand-rolled, because toLocaleDateString would render one
    string on the server and another on a browser in a different locale */
export function formatDate(t: number): string {
  const d = new Date(t);
  return `${d.getUTCDate()} ${MONTHS[d.getUTCMonth()]} ${d.getUTCFullYear()}`;
}

/** the same date shouting, for the activity feed's day headings */
export function formatDay(t: number): string {
  return formatDate(t).toUpperCase();
}

export function monthLabel(t: number): string {
  return MONTHS[new Date(t).getUTCMonth()];
}

/** the score on a given day, or null if it predates the language */
export function valueAt(points: Point[], t: number): number | null {
  let found: number | null = null;
  for (const p of points) {
    if (p.t > t) break;
    found = p.v;
  }
  return found;
}

/** how long ago, in the days the record is actually kept in */
function agoLabel(then: number | null, today: number): string {
  if (then === null) return 'Never';
  const days = Math.round((today - then) / DAY_MS);
  if (days <= 0) return 'Today';
  if (days === 1) return 'Yesterday';
  if (days < 30) return `${days} days ago`;
  return formatDate(then);
}

/* --------------------------------------------------------- from the API */

/** the API dates everything in unix seconds; the UI works in milliseconds */
const ms = (seconds: number) => seconds * 1000;

/** A stroke per language, in the order they appear. Arbitrary, and the
    backend has no business having an opinion about it — but it has to be
    stable, or a language would change colour between the tile and the line
    beside it. */
const STROKES = [
  'var(--mimi-accent)',
  'var(--mimi-gold)',
  'var(--mimi-violet)',
  'var(--mimi-done)',
];

/** the two letters on a language's badge. A script sample would read better
    for Japanese or Chinese; until one of those is a real course, the code is
    the honest placeholder. */
function glyphOf(code: string): string {
  return code.slice(0, 2).toUpperCase();
}

/**
 * An avatar URL we are willing to put in an `<img src>`, or undefined.
 *
 * The backend already checks this hard on the way in (profile.rs), so this is
 * the second of two locks rather than the only one — and it is worth having
 * because the check is one line and the thing being checked is a string from
 * one stranger that another stranger's browser is about to fetch. A profile
 * stored before the rule tightened, or served by some other deployment of the
 * backend, gets the same treatment as anything else.
 *
 * https and nothing else: naming the one safe scheme is a rule that stays
 * right, where a list of dangerous ones (`javascript:`, `data:`, `blob:`, …)
 * is a list somebody eventually forgets to extend.
 */
export function safeAvatar(url: string | null): string | undefined {
  if (!url) return undefined;
  return /^https:\/\/[^\s"'<>\\]+$/i.test(url) ? url : undefined;
}

/* Somebody edited their own profile.
 *
 * The page they edited it on refetches by itself; this is for the copies of
 * it living elsewhere in the chrome — the navbar draws the same name and
 * picture, from its own fetch made when the page loaded, and would otherwise
 * go on showing a picture its owner has just removed until the next
 * navigation. The auth store solves the same problem the same way for the
 * session (see auth.ts): one event, and whoever is showing it re-reads. */
const PROFILE_EVENT = 'mimi:profile';

export function announceProfileEdit(): void {
  window.dispatchEvent(new CustomEvent(PROFILE_EVENT));
}

/** listen for the above; returns the unsubscribe an effect wants back */
export function onProfileEdit(listener: () => void): () => void {
  window.addEventListener(PROFILE_EVENT, listener);
  return () => window.removeEventListener(PROFILE_EVENT, listener);
}

export function profileFrom(api: ApiProfile): Profile {
  const today = ms(api.today);
  const repeatedTargets = new Set(
    api.languages
      .map((language) => language.code)
      .filter((code, index, all) => all.indexOf(code) !== index),
  );
  return {
    username: api.username,
    display: api.display,
    title: api.title ?? undefined,
    bio: api.bio,
    cefr: api.cefr,
    avatar: safeAvatar(api.avatar),
    followers: api.followers,
    following: api.following,
    viewerFollows: api.viewer_follows,
    memberSince: ms(api.joined),
    online: api.online,
    lastActive: agoLabel(api.last_active === null ? null : ms(api.last_active), today),
    daysActive: api.totals.days,
    languages: api.languages.map((lang, i) => ({
      id: lang.id,
      name: repeatedTargets.has(lang.code)
        ? `${languageName(lang.code)} for ${languageName(lang.source_code)} speakers`
        : languageName(lang.code),
      glyph: glyphOf(lang.code),
      color: STROKES[i % STROKES.length],
      counts: { words: lang.words, skills: lang.skills, lessons: lang.lessons },
      score: lang.score,
      provisional: lang.provisional,
      delta: lang.delta,
      since: ms(lang.since),
      points: lang.points.map((p) => ({ t: ms(p.t), v: p.v })),
    })),
    totals: {
      xp: api.totals.xp,
      streak: api.streak,
      lessons: api.totals.lessons,
      words: api.totals.words,
    },
    practice: practiceFrom(api),
    today,
  };
}

/** What the practice behind the score amounts to, counted four ways. These
    are the totals the backend already keeps, reported as themselves — no
    score, because a count is the honest measure of "how much have you done"
    and any number derived from it would only obscure that. */
function practiceFrom(api: ApiProfile): Practice[] {
  return [
    { id: 'vocab', name: 'Vocabulary', icon: 'book', count: api.totals.words, noun: 'words' },
    { id: 'answered', name: 'Answered', icon: 'edit', count: api.totals.exercises, noun: 'exercises' },
    { id: 'right', name: 'Correct', icon: 'tick', count: api.totals.correct, noun: 'answers' },
    { id: 'days', name: 'Showed up', icon: 'calendar', count: api.totals.days, noun: 'days' },
  ];
}

/* ---------------------------------------------------------------- activity */

export interface ActivityEntry {
  icon: IconName;
  text: string;
  /** where the entry leads, when it is about somebody else: a profile */
  href?: string;
  /** where the language's score stood that day, when the entry moved one */
  score?: number;
  delta?: number;
  xp?: number;
  /** what was picked up — the actual point of the day */
  learned?: string[];
}

export interface ActivityDay {
  t: number;
  /** how many days in a row had been kept up to and including this one */
  streak: number;
  entries: ActivityEntry[];
}

/**
 * The day-by-day feed, newest first.
 *
 * One row of the backend's activity table becomes one day here, and the day's
 * lessons become one entry: how many, how they went, and the words they left
 * behind. A day that cleared a skill gets a second entry for it, because that
 * is a different kind of thing to have happened and reads as one.
 *
 * Most days have no words on them, and that is not a gap in the data: a
 * course teaches each word exactly once, so after the first week there is
 * nothing new left to meet and every lesson is review. The accuracy is what
 * those days have to say for themselves, so it is what they say.
 *
 * Not every day here is a day of study. Following somebody is dated and shows
 * up in the feed, but it earns nothing and keeps no streak, so a day with a
 * follow and no lesson has no lesson entry at all — writing "Completed 0
 * lessons" would be a report of something that didn't happen.
 */
export function activityFrom(api: ApiProfile): ActivityDay[] {
  return api.days.map((day) => {
    const lessons = `${day.lessons} lesson${day.lessons === 1 ? '' : 's'}`;
    const accuracy = day.exercises > 0 ? ` · ${day.correct} of ${day.exercises} right` : '';
    const entries: ActivityEntry[] = [
      ...(day.lessons > 0
        ? [{
            icon: 'learning' as IconName,
            text: `Completed ${lessons}${accuracy}`,
            score: day.score,
            delta: day.delta,
            xp: day.xp,
            learned: day.learned.length ? day.learned : undefined,
          }]
        : []),
      ...day.skills.map((skill): ActivityEntry => ({
        icon: 'learning',
        text: `Cleared a skill · ${skill}`,
      })),
      /* Whether the follow is still live is not asked and not shown: the
         entry says what happened on the day, and un-following somebody later
         doesn't unhappen it. */
      ...day.followed.map((follow): ActivityEntry => ({
        icon: 'following',
        text: `Followed ${follow.display}`,
        href: `/u/${encodeURIComponent(follow.username)}`,
      })),
    ];
    return { t: ms(day.t), streak: day.streak, entries };
  });
}
