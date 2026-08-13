/* The lesson's run queue. The backend serves a lesson as an ordered list of
   tasks, but a wrongly-answered exercise doesn't just fall behind: it goes
   back on the end of the queue and keeps coming back until the user gets it
   right. That's purely for the user's benefit — only each exercise's first
   attempt is reported, so the backend still sees the wrong verdict (see
   mimi_backend AGENTS.md, "Grading (the client's job)"). */

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
  /** the queue with the finished task behind us — or, on a wrong answer,
      with that task moved to the end (the queue doesn't grow: the exercise
      isn't done until it's answered right) */
  queue: QueueEntry[];
  /** the verdicts to report so far — grows only on first attempts */
  responses: ApiResponse[];
  /** the task wasn't finished but moved to the end of the queue: whatever
      was next slides into its slot, so the caller keeps the same index
      (and the same question count) */
  moved: boolean;
  /** nothing left in the queue: submit */
  done: boolean;
}

/**
 * Finish the task at `index` with the given verdict (null for material, which
 * is never answered). A wrong exercise isn't finished: it moves to the end of
 * the queue and keeps coming back until it's answered right; everything else
 * just moves on.
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
  let moved = false;
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
    // wrong goes back on the end of the queue until it's answered right
    if (!verdict?.correct) {
      next = [...queue.slice(0, index), ...queue.slice(index + 1), { task: entry.task, retry: true }];
      moved = true;
    }
  }
  return { queue: next, responses: all, moved, done: !moved && index + 1 === queue.length };
}
