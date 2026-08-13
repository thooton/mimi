import { strict as assert } from "node:assert";
import { test } from "node:test";
import type { ApiResponse, ApiTask } from "./api.ts";
import type { Verdict } from "./grading.ts";
import { advanceQueue, queueOf } from "./lessonQueue.ts";

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

  // first attempt: wrong — reported, and a retry joins the end of a lesson
  // that is now one task longer ("1 / 2" becomes "2 / 3", not "2 / 2")
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

  // …then the retry: wrong again — another one goes on the end, with nothing
  // further to report
  step = advanceQueue(queue, 2, wrong, responses);
  assert.equal(step.responses, responses); // untouched
  assert.equal(step.queue.length, 4);
  assert.equal(step.done, false);
  queue = step.queue;

  // the retry's retry: right at last — still nothing further to report,
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
