/* Everything the community tab shows, as example data.

   None of this has a backend yet — clubs, forums and chat matching are all
   mocked, the same way dailyQuests are. The shapes below are the contract a
   real endpoint would honour, and the pages render from them alone, so the
   day the server grows these collections the views don't change. */

/* ------------------------------------------------------------------ clubs */

export interface Club {
  /** the short tag worn in front of a member's name — [IVY] Clover */
  tag: string;
  name: string;
  /** one line of self-description, written by the club */
  about: string;
  /** the language the club centres on, or null if it takes all comers */
  focus: string | null;
  members: number;
  /** mean XP earned per member this week — the number the board ranks by */
  avgXp: number;
}

export const CLUBS: Club[] = [
  { tag: 'KOI', name: 'Koi Pond', about: 'Japanese, from kana to novels. Weekly read-alongs.', focus: 'ja', members: 412, avgXp: 1042 },
  { tag: 'NOVA', name: 'Nova', about: 'Small club, big weeks. 50 XP a day or you answer for it.', focus: null, members: 204, avgXp: 915 },
  { tag: 'IVY', name: 'Ivy Circle', about: 'The oldest club on mimi. Steady hands, long streaks.', focus: null, members: 1284, avgXp: 861 },
  { tag: 'NORD', name: 'Nord', about: 'Scandinavian languages over strong coffee.', focus: 'sv', members: 156, avgXp: 844 },
  { tag: 'SOL', name: 'Sol y Sombra', about: 'Spanish every day, siesta optional.', focus: 'es', members: 967, avgXp: 733 },
  { tag: 'OLE', name: 'Casa Olé', about: 'Beginners welcome. We celebrate every first lesson.', focus: 'es', members: 512, avgXp: 698 },
  { tag: 'ATLAS', name: 'Atlas', about: 'A different language every season. Tourists at heart.', focus: null, members: 743, avgXp: 588 },
  { tag: 'LUNA', name: 'Luna', about: 'French after dark. Evening sessions, slow Sundays.', focus: 'fr', members: 629, avgXp: 512 },
];

export type ClubSort = 'avgXp' | 'members';

/** the club leaderboard: a copy of the roster ordered by the chosen metric */
export function clubBoard(sort: ClubSort): Club[] {
  return [...CLUBS].sort((a, b) => b[sort] - a[sort]);
}

/* The club the signed-in viewer belongs to — the tag the weekly board shows
   beside "You" and the one the directory marks as joined. */
export const VIEWER_CLUB = 'SOL';

/* ----------------------------------------------------------------- forums */

export interface ForumBoard {
  id: string;
  name: string;
  blurb: string;
  threads: number;
}

export const FORUM_BOARDS: ForumBoard[] = [
  { id: 'es-help', name: 'Spanish help', blurb: 'Grammar, vocab and usage — ask anything.', threads: 12408 },
  { id: 'ja-help', name: 'Japanese help', blurb: 'From kana to keigo, answered patiently.', threads: 6153 },
  { id: 'exchange', name: 'Language exchange', blurb: 'Find a partner and trade half an hour.', threads: 3987 },
  { id: 'stories', name: 'Success stories', blurb: 'Breakthroughs, big and small.', threads: 1741 },
  { id: 'clubs', name: 'Clubs & events', blurb: 'Recruiting, challenges and weekly sprints.', threads: 986 },
  { id: 'feedback', name: 'Site feedback', blurb: 'What works, what doesn’t, what’s next.', threads: 1214 },
];

export interface Thread {
  id: string;
  title: string;
  /** ForumBoard id it was posted in */
  board: string;
  author: string;
  club?: string;
  replies: number;
  likes: number;
  /** relative time, preformatted — the mock has no clock to compute it from */
  ago: string;
}

/** what's hot this week, in the order the community page previews it */
export const TRENDING: Thread[] = [
  { id: 't1', title: 'Weekly club challenge: the 500 XP sprint — sign-ups open', board: 'clubs', author: 'Clover', club: 'IVY', replies: 156, likes: 341, ago: '3h' },
  { id: 't2', title: 'Why does “lo” refuse to make sense? A rant and a plea', board: 'es-help', author: 'Diego', club: 'OLE', replies: 84, likes: 212, ago: '2h' },
  { id: 't3', title: 'I finally understood a whole podcast episode!', board: 'stories', author: 'Priya', club: 'NOVA', replies: 47, likes: 298, ago: '5h' },
  { id: 't4', title: 'Ser vs estar: the flowchart I wish someone had handed me', board: 'es-help', author: 'Ines', club: 'SOL', replies: 39, likes: 187, ago: '8h' },
  { id: 't5', title: 'Pitch accent — does it really matter this much?', board: 'ja-help', author: 'Ren', club: 'KOI', replies: 62, likes: 143, ago: '11h' },
  { id: 't6', title: 'Looking for a Spanish ⇄ German partner, evenings CET', board: 'exchange', author: 'Lena', club: 'NORD', replies: 12, likes: 28, ago: '14h' },
  { id: 't7', title: '100 days in a row — ask me anything', board: 'stories', author: 'Omar', club: 'ATLAS', replies: 71, likes: 256, ago: '1d' },
  { id: 't8', title: 'The new audio exercises are too quiet on mobile', board: 'feedback', author: 'Sora', club: 'KOI', replies: 23, likes: 95, ago: '1d' },
];

export function boardName(id: string): string {
  return FORUM_BOARDS.find((b) => b.id === id)?.name ?? id;
}

/* ------------------------------------------------------------- chat match */

export interface ChatPartner {
  name: string;
  club?: string;
  /** language code they speak natively */
  native: string;
  /** language code they're learning */
  learning: string;
  /** CEFR in the language they're learning */
  level: string;
  online: boolean;
}

/* The mock roster: who could plausibly answer a ping right now. It doubles
   as the example set for the matching rule below — pairs are complementary,
   so each direction of an exchange needs representation here. */
const CHAT_ROSTER: ChatPartner[] = [
  { name: 'Diego', club: 'OLE', native: 'es', learning: 'en', level: 'B2', online: true },
  { name: 'Ines', club: 'SOL', native: 'es', learning: 'fr', level: 'B1', online: false },
  { name: 'Aiko', club: 'KOI', native: 'ja', learning: 'en', level: 'B1', online: true },
  { name: 'Chiyo', club: 'KOI', native: 'ja', learning: 'es', level: 'B2', online: true },
  { name: 'Kenji', native: 'ja', learning: 'en', level: 'A2', online: true },
  { name: 'Felix', club: 'NORD', native: 'de', learning: 'ja', level: 'B1', online: true },
  { name: 'Lena', club: 'NORD', native: 'de', learning: 'es', level: 'C1', online: true },
  { name: 'Marco', club: 'ATLAS', native: 'it', learning: 'es', level: 'B2', online: true },
  { name: 'Bruno', club: 'LUNA', native: 'pt', learning: 'es', level: 'A2', online: false },
  { name: 'Yuki', club: 'LUNA', native: 'ja', learning: 'fr', level: 'A1', online: false },
  { name: 'Zoe', club: 'NOVA', native: 'en', learning: 'ja', level: 'A2', online: true },
  { name: 'Omar', club: 'ATLAS', native: 'ar', learning: 'en', level: 'B1', online: true },
  { name: 'Gwen', native: 'en', learning: 'es', level: 'B1', online: true },
  { name: 'Hugo', club: 'LUNA', native: 'fr', learning: 'es', level: 'B2', online: true },
];

export type MatchKind = 'native' | 'learner';

/* The matching rule. You speak A and learn B:
   - a native match speaks B natively — ideally while learning A, so the
     conversation can trade halves instead of one side always tutoring;
   - a fellow learner is learning B alongside you — ideally with A as their
     native language, so they can still correct you.
   Online partners float above offline ones, then the closer fits. */
export function findMatches(speak: string, learn: string, kind: MatchKind): ChatPartner[] {
  return CHAT_ROSTER.map((p) => {
    let fit = -1;
    if (kind === 'native') {
      if (p.native === learn) fit = p.learning === speak ? 2 : 1;
    } else if (p.learning === learn && p.native !== learn) {
      fit = p.native === speak ? 2 : 1;
    }
    return { p, fit };
  })
    .filter((s) => s.fit >= 0)
    .sort((a, b) => Number(b.p.online) - Number(a.p.online) || b.fit - a.fit)
    .slice(0, 3)
    .map((s) => s.p);
}
