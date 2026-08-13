import { useCallback, useEffect, useRef, useState } from 'react';
import { Button } from '@blueprintjs/core';
import type { ApiCourse, ApiDailyQuests, ApiLesson, ApiLessonTarget, ApiPosition, ApiResponse, ApiSubmitResult, ApiXpSchedule } from '../../data/api';
import { createCastle, createLesson, ensureUser, fetchCourse, fetchProfile, fetchQuests, submitLesson } from '../../data/api';
import { useAuth } from '../../data/auth';
import { treeFromCourse } from '../../data/course';
import { languageByCode } from '../../data/languages';
import { useTargetLang } from '../../data/targetLang';
import CourseMap from './CourseMap';
import Sidebar from './Sidebar';
import LessonPlayer from './LessonPlayer';
import LanguageChooser from './LanguageChooser';

/* How long a leaving lesson stays mounted while it fades — mirrors
   lesson-shell-out's 180ms in styles/motion.css, so the player unmounts
   exactly as it reaches transparency. */
const QUIT_MS = 180;

/* The learn page's one React root: it owns the signed-in learner's course map
   and lesson in flight and hands slices of them to
   the presentational components. Everything backend-shaped lives here. */
export default function LearnApp() {
  const { user, ready: authReady, startAsGuest } = useAuth();
  const { lang, ready, setLang } = useTargetLang();
  const language = languageByCode(lang);
  const [course, setCourse] = useState<ApiCourse | null>(null);
  const [dailyQuests, setDailyQuests] = useState<ApiDailyQuests | null>(null);
  const [xpSchedule, setXpSchedule] = useState<ApiXpSchedule | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [lesson, setLesson] = useState<ApiLesson | null>(null);
  const [starting, setStarting] = useState(false);
  /* guards the one-shot guest sign-up below: an effect can run twice (React
     strict mode does it deliberately), and two of these racing would open two
     records and leave the cookie pointing at whichever landed second */
  const opening = useRef(false);
  /* remounted every time progress changes, so the map lands back on the
     (new) current node instead of wherever the user last scrolled */
  const [mapKey, setMapKey] = useState(0);
  /* true while a leaving lesson plays its exit fade: the map is already
     mounted beneath the player, which is what the fade reveals */
  const [dismissing, setDismissing] = useState(false);
  const dismissTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);

  useEffect(() => () => clearTimeout(dismissTimer.current), []);

  /* Nobody without an account is *asked* for one: arriving at the course
     opens a guest record and drops them straight into it. A first screen
     offering to begin is a decision about a thing they haven't seen yet, and
     the honest moment to ask is after the first lesson, when there is
     something worth keeping (see LessonPlayer's summary).

     This is the only place a guest is ever created — the rest of the site
     reads whoever the cookie says, so a visitor reading the leaderboard
     doesn't quietly become a learner. */
  useEffect(() => {
    if (!authReady || user || opening.current) return;
    opening.current = true;
    startAsGuest().catch((e) => setError(e instanceof Error ? e.message : String(e)));
  }, [authReady, user, startAsGuest]);

  const load = useCallback(async () => {
    if (!user) return;
    await ensureUser();
    const [loadedCourse, profile, quests] = await Promise.all([
      fetchCourse(),
      fetchProfile(user.username),
      fetchQuests(),
    ]);
    setCourse(loadedCourse);
    setXpSchedule(profile.xp_schedule);
    setDailyQuests(quests);
  }, [user]);

  /* `lang` is a dependency even though nothing below reads it: the backend
     serves one course and `/course` takes no language, so switching can't
     change the answer *today*. Keying the effect on it anyway means that the
     day the endpoint grows a target_lang, switching already refetches — and
     in the meantime it clears a course belonging to the language you just
     left. */
  useEffect(() => {
    if (!user || !language?.available) return;
    let cancelled = false;
    setCourse(null);
    setDailyQuests(null);
    setXpSchedule(null);
    setLesson(null);
    setError(null);
    load().catch((e) => {
      if (!cancelled) setError(e instanceof Error ? e.message : String(e));
    });
    return () => {
      cancelled = true;
    };
  }, [load, lang, language, user]);

  /* A tree has no global position: the selected skill supplies its reached
     lesson address, and completed skills may request their last one again. */
  async function startLesson(position?: ApiPosition) {
    setStarting(true);
    try {
      setLesson(await createLesson(position));
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setStarting(false);
    }
  }

  async function startCastle() {
    setStarting(true);
    try {
      setLesson(await createCastle());
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setStarting(false);
    }
  }

  async function finishLesson(target: ApiLessonTarget, responses: ApiResponse[]): Promise<ApiSubmitResult> {
    const result = await submitLesson(target, responses);
    // refetch before the summary screen is dismissed so the map is already
    // up to date when it reappears, including level and castle changes
    const [updatedCourse, updatedQuests] = await Promise.all([
      fetchCourse(),
      fetchQuests(),
    ]);
    setCourse(updatedCourse);
    setDailyQuests(updatedQuests);
    setMapKey((k) => k + 1);
    return result;
  }

  /* Nothing here means anything until we know what's being learnt, and the
     prerendered HTML can't know it — so the three language states come
     first, before any of the backend's.

     While anything is loading the page renders nothing at all: no spinner,
     no skeleton, no guessed data. The flex layout (body → .app-main) holds
     the page at full height regardless, so the content simply appears in
     place once it's ready — the way web users expect things to. */
  if (!authReady || !ready) {
    return null;
  }

  /* Opening the guest record is one more thing to wait for, and it is on the
     same footing as the rest: blank until it lands. An error while it is
     happening falls through to the report below. */
  if (!user && !error) {
    return null;
  }

  // no choice yet, or one this build does not offer
  if (user && !language?.available) {
    return <LanguageChooser onPick={setLang} />;
  }

  if (error) {
    return (
      <div className="learn-status">
        <p className="learn-status-title">Can't reach the mimi server</p>
        <p className="learn-status-detail">{error}</p>
        <Button
          intent="primary"
          text="Retry"
          onClick={() => {
            setError(null);
            setCourse(null);
            setDailyQuests(null);
            setXpSchedule(null);
            /* the failure may have been opening the guest record in the first
               place, in which case there is nothing loaded yet to reload */
            const again = user ? load() : startAsGuest().then(() => {});
            again.catch((e) => setError(e instanceof Error ? e.message : String(e)));
          }}
        />
      </div>
    );
  }

  /* the course, profile policy and quest fetches are one loading wait —
     blank, like `!ready` above */
  if (!user || !course || !dailyQuests || !xpSchedule) {
    return null;
  }

  /* onExit doesn't unmount the player at once: the map goes back on the
     page immediately, but underneath the still-mounted player, which fades
     to transparency over it (css: lesson-shell-out) — a crossfade home
     instead of fade-to-beige, cut-to-map. The player's own quit() has
     already guarded against repeats, but a second path here (a double
     unmount) is just as possible, so guard this one too. */
  function exitLesson() {
    if (dismissing) return;
    setDismissing(true);
    dismissTimer.current = setTimeout(() => {
      setLesson(null);
      setDismissing(false);
    }, QUIT_MS);
  }

  const tree = treeFromCourse(course);
  const map = (
    <div className="shell">
      <div className="learn-grid">
        <main>
          <CourseMap key={mapKey} tree={tree} lessonXp={xpSchedule.lesson} onStart={startLesson} onCastle={startCastle} starting={starting} />
        </main>
        <aside className="side-rail" aria-label="Progress and stats">
          <Sidebar tree={tree} quests={dailyQuests.quests} />
        </aside>
      </div>
    </div>
  );

  if (lesson) {
    return (
      <>
        {dismissing && map}
        <LessonPlayer
          lesson={lesson}
          targetLang={course.target_lang}
          guest={user.guest}
          onFinish={(responses) => finishLesson(lesson.target, responses)}
          onExit={exitLesson}
        />
      </>
    );
  }

  return map;
}
