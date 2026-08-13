/* The lesson's run queue. The backend serves a lesson as an ordered list of
   tasks, but a wrongly-answered exercise doesn't just fall behind: a retry of
   it is added to the end of the queue and keeps coming back until the user
   gets it right. That's purely for the user's benefit — only each exercise's
   first attempt is reported, so the backend still sees the wrong verdict (see
   mimi_backend AGENTS.md, "Grading (the client's job)").

   The retry is *appended*, and the task it retries stays retired where it
   was, so the queue itself grows by one for every mistake. What the queue
   must not do is *say* so. A lesson is the set of tasks the backend served;
   a mistake is not a longer lesson, it is the same lesson not finished yet —
   which is the promise Duolingo's fixed bar makes, and the one to keep.

   So progress is measured in `cleared` (below) rather than in queue
   positions: how many of the served tasks are behind the user for good.
   A wrong answer clears nothing, so the count and the strip simply hold
   where they are until its retry comes round and is answered right. The
   three readings this has to keep honest, all at once:

     - the total never moves — it is `lesson.tasks.length`, start to finish;
     - the strip only ever advances on real progress, and is full exactly
       when the lesson ends;
     - the count can never read "5 / 5" with questions still to come, which
       is the failure that made an earlier arrangement (rotating the wrong
       task into a fixed-length queue) unworkable.

   The arrangement in between those two — growing the displayed total with
   the queue — kept all three honest and was still wrong: it answered a
   mistake by moving the finish line, so the lesson got longer the worse it
   went and the end never came closer. */

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

/**
 * How many of the lesson's `total` served tasks are behind the user for
 * good, with `index` tasks retired from `queue` so far. This is the numerator
 * of both the count and the progress strip; `total` is the denominator and
 * never changes.
 *
 * Every entry past the first `total` is a retry, and a retry exists exactly
 * because the task it copies was missed — so the appended retries *are* the
 * tally of what is still outstanding, and
 *
 *     cleared = tasks retired − retries appended
 *
 * with no need to look at any entry. Worked through, on a lesson of 12 where
 * the first answer is wrong: index 1, queue 13, so cleared 0 — the strip
 * doesn't move for a miss. Answer the next right and it is 2 − 1 = 1. Reach
 * the retry at the end and it reads 11 of 12, with one question to go; get it
 * right and index 13 − 1 retry = 12, full and finished. Get it wrong again
 * and a second retry joins the queue, holding the reading at 11 rather than
 * pushing the total to 14.
 */
export function clearedCount(queue: QueueEntry[], index: number, total: number): number {
  return index - (queue.length - total);
}

export interface Advance {
  /** the queue with the finished task behind us — plus, on a wrong answer, a
      retry of it on the end (the exercise isn't done until it's right) */
  queue: QueueEntry[];
  /** the verdicts to report so far — grows only on first attempts */
  responses: ApiResponse[];
  /** the answer was wrong, so a retry was added to the end: one of the
      lesson's tasks is still outstanding, and `cleared` holds where it is */
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
