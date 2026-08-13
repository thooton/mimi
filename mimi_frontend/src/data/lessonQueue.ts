/* The lesson's run queue. The backend serves a lesson as an ordered list of
   tasks, but a wrongly-answered exercise doesn't just fall behind: a retry of
   it is added to the end of the queue and keeps coming back until the user
   gets it right. That's purely for the user's benefit — only each exercise's
   first attempt is reported, so the backend still sees the wrong verdict (see
   mimi_backend AGENTS.md, "Grading (the client's job)").

   The retry is *appended*, and the task it retries stays retired where it
   was, so the queue grows by one for every mistake. The player reads its
   position and its total straight off this queue, and that is the point: a
   mistake really does mean one more task to answer, and both the count and
   the progress strip say so. (Rotating the wrong task into the end of a
   fixed-length queue — the older arrangement — left the player answering
   more tasks than the total it was showing, so the count could reach "5 / 5"
   with questions still to come and the strip stuck short of the end.) */

import type { ApiResponse, ApiTask } from './api';
import type { Verdict } from './grading';

/** one slot in the queue. A retry was already reported (as wrong) when its
    first attempt finished, so it answers again but reports nothing more. */
export interface QueueEntry {
  task: ApiTask;
  retry: boolean;
}

/** the lesson's tasks, as the backend sent them */
export function queueOf(tasks: ApiTask[]): QueueEntry[] {
  return tasks.map((task) => ({ task, retry: false }));
}

export interface Advance {
  /** the queue with the finished task behind us — plus, on a wrong answer, a
      retry of it on the end (the exercise isn't done until it's right) */
  queue: QueueEntry[];
  /** the verdicts to report so far — grows only on first attempts */
  responses: ApiResponse[];
  /** the answer was wrong, so a retry was added to the end: the lesson is one
      task longer than it was */
  requeued: boolean;
  /** nothing left in the queue: submit */
  done: boolean;
}

/**
 * Finish the task at `index` with the given verdict (null for material, which
 * is never answered). The caller always moves on to `index + 1`; a wrong
 * exercise simply leaves a retry of itself waiting at the end of the queue,
 * and keeps doing so until it's answered right.
 */
export function advanceQueue(
  queue: QueueEntry[],
  index: number,
  verdict: Verdict | null,
  responses: ApiResponse[],
): Advance {
  const entry = queue[index];
  let all = responses;
  let next = queue;
  let requeued = false;
  if (entry.task.kind === 'exercise') {
    // only the first attempt reports: a retry was already recorded as the
    // wrong answer it was, and getting it right now doesn't change that
    if (!entry.retry) {
      all = [
        ...responses,
        {
          ask: entry.task.task.ask,
          correct: verdict?.correct ?? false,
          words: verdict?.concepts ?? {},
        },
      ];
    }
    // wrong earns another go at the end of the queue, until it's right
    if (!verdict?.correct) {
      next = [...queue, { task: entry.task, retry: true }];
      requeued = true;
    }
  }
  // an appended retry is itself something left to do, so this is simply
  // "was that the last of them?" against the queue we're handing back
  return { queue: next, responses: all, requeued, done: index + 1 === next.length };
}
