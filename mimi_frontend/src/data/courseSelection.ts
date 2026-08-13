import { useEffect, useState } from 'react';
import {
  ensureUser,
  fetchCourses,
  fetchProfile,
  fetchViewer,
  setActiveCourse,
} from './api';
import type { ApiCourseSummary } from './api';

/* The account's active course, shared by the navbar and learn page even
   though they are separate React roots. The backend owns both the choice and
   the catalog: a wiki course becomes selectable as soon as a valid snapshot
   has been assembled, without a frontend release changing an `available`
   flag. */

const EVENT = 'mimi:course';

interface CourseState {
  courseId: string | null;
  courses: ApiCourseSummary[];
  ready: boolean;
  error: string | null;
}

let current: CourseState = { courseId: null, courses: [], ready: false, error: null };
let inflight: Promise<void> | null = null;

function announce() {
  window.dispatchEvent(new CustomEvent(EVENT));
}

function load(): void {
  if (inflight) return;
  inflight = (async () => {
    const courses = await fetchCourses();
    let courseId: string | null = null;
    try {
      const viewer = await fetchViewer();
      const profile = await fetchProfile(viewer.username);
      courseId = profile.course_id;
    } catch {
      // The catalog is public and useful before the learn page has created
      // its guest account. No session means only that nothing is selected.
    }
    current = { courseId, courses, ready: true, error: null };
  })()
    .catch((error) => {
      current = {
        courseId: null,
        courses: [],
        ready: true,
        error: error instanceof Error ? error.message : String(error),
      };
    })
    .finally(announce);
}

function select(courseId: string): void {
  ensureUser()
    .then(() => setActiveCourse(courseId))
    .then(() => {
      current = { ...current, courseId, ready: true, error: null };
      announce();
    })
    .catch((error) => {
      current = {
        ...current,
        error: error instanceof Error ? error.message : String(error),
      };
      announce();
    });
}

function retry(): void {
  inflight = null;
  current = { ...current, ready: false, error: null };
  announce();
  load();
}

export interface CourseSelection extends CourseState {
  course: ApiCourseSummary | null;
  setCourse: (courseId: string) => void;
  retry: () => void;
}

export function useCourseSelection(): CourseSelection {
  const [state, setState] = useState<CourseState>(current);

  useEffect(() => {
    const sync = () => setState(current);
    sync();
    load();
    window.addEventListener(EVENT, sync);
    return () => window.removeEventListener(EVENT, sync);
  }, []);

  return {
    ...state,
    course: state.courses.find((course) => course.id === state.courseId) ?? null,
    setCourse: select,
    retry,
  };
}
