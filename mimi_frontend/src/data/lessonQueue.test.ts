import { strict as assert } from "node:assert";
import { test } from "node:test";
import type { ApiResponse, ApiTask } from "./api.ts";
import type { Verdict } from "./grading.ts";
import { advanceQueue, clearedCount, queueOf } from "./lessonQueue.ts";

const material: ApiTask = {
  kind: "material",
  task: { text: "Say **hi**." },
};

const exercise = (id: string): ApiTask => ({
  kind: "exercise",
  task: {
    id,
    ask: "write_target",
    kind: "translate",
    direction: "en->es",
    prompt: "Hello",
    words: ["C_hola"],
    answers: [{ text: "Hola", words: [{ word: "C_hola", start: 0, end: 4 }] }],
    introduces: [],
    new_words: [],
    prompt_glosses: [],
    answer_glosses: [],
  },
});

const right: Verdict = { correct: true, concepts: { C_hola: true }, canonical: "Hola" };
const wrong: Verdict = { correct: false, concepts: { C_hola: false }, canonical: "Hola" };

test("material is never reported", () => {
  const queue = queueOf([material]);
  const step = advanceQueue(queue, 0, null, []);
  assert.equal(step.responses.length, 0);
  assert.equal(step.queue.length, 1);
  assert.equal(step.requeued, false);
  assert.equal(step.done, true);
});

test("a right answer is reported once and moves on", () => {
  const queue = queueOf([exercise("E1"), exercise("E2")]);
  const step = advanceQueue(queue, 0, right, []);
  assert.deepEqual(step.responses, [
    { ask: "write_target", correct: true, words: { C_hola: true } },
  ]);
  assert.equal(step.queue.length, 2);
  assert.equal(step.requeued, false);
  assert.equal(step.done, false);
});

test("a wrong answer is reported wrong and earns a retry on the end", () => {
  const queue = queueOf([exercise("E1"), exercise("E2")]);
  const step = advanceQueue(queue, 0, wrong, []);
  assert.deepEqual(step.responses, [
    { ask: "write_target", correct: false, words: { C_hola: false } },
  ]);
  assert.equal(step.queue.length, 3); // one more task to answer than before…
  assert.equal(step.requeued, true);
  assert.deepEqual(step.queue.slice(0, 2), queue); // …the run so far untouched…
  assert.deepEqual(step.queue[2], { task: queue[0].task, retry: true }); // …and the retry at the back
  assert.equal(step.done, false);
});

test("a wrong answer on the last task isn't the end", () => {
  const queue = queueOf([exercise("E1")]);
  const step = advanceQueue(queue, 0, wrong, []);
  assert.equal(step.queue.length, 2);
  assert.equal(step.requeued, true);
  assert.equal(step.done, false);
});

test("a re-queued exercise keeps coming back until it's right", () => {
  let queue = queueOf([exercise("E1"), material]);
  let responses: ApiResponse[] = [];

  // first attempt: wrong, reported, and a retry joins the end of the run.
  // The lesson is still two tasks long; it is just not finished (the count
  // holds at "0 / 2", see the cleared tests below)
  let step = advanceQueue(queue, 0, wrong, responses);
  responses = step.responses;
  queue = step.queue;
  assert.equal(queue.length, 3);
  assert.equal(queue[2].retry, true);

  // the material advances as usual…
  step = advanceQueue(queue, 1, null, responses);
  assert.equal(step.done, false);
  assert.equal(step.queue.length, 3);
  queue = step.queue;

  // …then the retry: wrong again, another one goes on the end, with nothing
  // further to report
  step = advanceQueue(queue, 2, wrong, responses);
  assert.equal(step.responses, responses); // untouched
  assert.equal(step.queue.length, 4);
  assert.equal(step.done, false);
  queue = step.queue;

  // the retry's retry: right at last, still nothing further to report,
  // and the queue finally runs out
  step = advanceQueue(queue, 3, right, responses);
  assert.equal(step.responses.length, 1);
  assert.equal(step.done, true);
  assert.deepEqual(step.responses[0], {
    ask: "write_target",
    correct: false, // the backend still sees the first, wrong attempt
    words: { C_hola: false },
  });
});

/* ---- what the progress line reads (cleared / total) ---- */

test("a clean run clears one task per answer", () => {
  const queue = queueOf([exercise("E1"), exercise("E2"), material]);
  assert.equal(clearedCount(queue, 0, 3), 0);
  assert.equal(clearedCount(queue, 1, 3), 1);
  assert.equal(clearedCount(queue, 2, 3), 2);
  assert.equal(clearedCount(queue, 3, 3), 3); // full, exactly as the lesson ends
});

test("a miss holds the count instead of lengthening the lesson", () => {
  let queue = queueOf([exercise("E1"), exercise("E2")]);
  const total = 2;

  // miss the first: the run grows to three, the reading stays at nothing done
  queue = advanceQueue(queue, 0, wrong, []).queue;
  assert.equal(queue.length, 3);
  assert.equal(clearedCount(queue, 1, total), 0);

  // answer the second correctly: one of the two, with the retry still to come
  queue = advanceQueue(queue, 1, right, []).queue;
  assert.equal(clearedCount(queue, 2, total), 1);

  // miss the retry as well, a second retry joins the end and the reading
  // still holds at one, rather than the total climbing to four
  queue = advanceQueue(queue, 2, wrong, []).queue;
  assert.equal(queue.length, 4);
  assert.equal(clearedCount(queue, 3, total), 1);

  // get it right at last: both tasks cleared, the strip full, the lesson over
  const step = advanceQueue(queue, 3, right, []);
  assert.equal(step.done, true);
  assert.equal(clearedCount(step.queue, 4, total), 2);
});

test("the count never reaches the total with questions still to come", () => {
  // the failure the reading exists to prevent: miss the last task of a
  // lesson and the finish is still one question away, so it must not read
  // as finished
  let queue = queueOf([exercise("E1"), exercise("E2")]);
  const total = 2;
  queue = advanceQueue(queue, 0, right, []).queue;
  const step = advanceQueue(queue, 1, wrong, []);
  assert.equal(step.done, false);
  assert.equal(clearedCount(step.queue, 2, total), 1); // "1 / 2", not "2 / 2"
});
