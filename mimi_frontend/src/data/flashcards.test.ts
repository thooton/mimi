import { strict as assert } from "node:assert";
import { test } from "node:test";
import type { ApiFlashcard } from "./api.ts";
import {
  addBatch,
  markReported,
  outOfCards,
  pendingReport,
  rateCard,
  startReview,
} from "./flashcards.ts";
import type { ReviewState } from "./flashcards.ts";

function card(word: string): ApiFlashcard {
  return {
    word,
    direction: "target_to_source",
    language_direction: "es->en",
    front: word,
    back: word,
    example: null,
  };
}

/** a run holding one batch of `size` cards, named a, b, c, … */
function runOf(size: number): ReviewState {
  const names = "abcdefghij".slice(0, size).split("");
  return addBatch(startReview(), names.map(card));
}

test("anything but a lapse clears the card and moves on", () => {
  const start = runOf(2);
  const step = rateCard(start, true);
  assert.equal(step.cleared, 1);
  assert.equal(step.pos, 1);
  assert.equal(step.queue.length, 2); // nothing appended
  assert.equal(outOfCards(step), false);
});

test("a lapse re-queues the card at the end and clears nothing", () => {
  const start = runOf(2);
  const step = rateCard(start, false);
  assert.equal(step.cleared, 0);
  assert.equal(step.queue.length, 3);
  // the lapsed card sits at the back, behind the unseen one
  assert.equal(step.queue[2], start.queue[0]);
  assert.deepEqual(step.lapsed, [start.queue[0]]);
});

test("a lapsed card comes back and can still be cleared", () => {
  let state = runOf(1);
  state = rateCard(state, false);
  assert.equal(outOfCards(state), false); // the retry means the end moved
  state = rateCard(state, true); // the retry itself
  assert.equal(state.cleared, 1);
  assert.equal(outOfCards(state), true);
  assert.deepEqual(state.lapsed, [0]); // the tally still knows it lapsed
  assert.deepEqual(state.firstAnswers, [false]); // FSRS hears the real first try
});

test("lapsing the same card twice counts the card once", () => {
  let state = runOf(1);
  state = rateCard(state, false);
  state = rateCard(state, false);
  assert.deepEqual(state.lapsed, [0]);
  assert.equal(state.queue.length, 3);
});

test("a batch is reported once every card in it has a first verdict", () => {
  let state = runOf(2);
  state = rateCard(state, false);
  assert.equal(pendingReport(state), null); // b is still undecided
  state = rateCard(state, true);
  const report = pendingReport(state);
  assert.ok(report);
  assert.equal(report.through, 2);
  assert.deepEqual(
    report.cards.map((c) => [c.word, c.correct]),
    [["a", false], ["b", true]],
  );
  // the lapsed card is still queued for practice, so the report lands while
  // the learner is busy rather than stalling them at an empty queue
  assert.equal(outOfCards(state), false);
});

test("a reported batch is not offered again", () => {
  let state = runOf(1);
  state = rateCard(state, true);
  state = markReported(state, pendingReport(state)!.through);
  assert.equal(pendingReport(state), null);
});

test("a new batch queues behind the leftovers and keeps the run going", () => {
  let state = runOf(1);
  state = rateCard(state, false); // "a" lapses and is requeued
  state = markReported(state, pendingReport(state)!.through);
  state = addBatch(state, [card("b")]);
  assert.equal(state.deck.length, 2);
  // the retry of "a" comes first, then the new card
  assert.deepEqual(state.queue.slice(state.pos), [0, 1]);
  assert.equal(outOfCards(state), false);
});

test("only the new batch is reported the second time round", () => {
  let state = runOf(1);
  state = rateCard(state, true);
  state = markReported(state, pendingReport(state)!.through);
  state = addBatch(state, [card("b")]);
  assert.equal(pendingReport(state), null); // "b" has no verdict yet
  state = rateCard(state, true);
  const report = pendingReport(state);
  assert.deepEqual(report!.cards.map((c) => c.word), ["b"]);
  assert.equal(report!.through, 2);
});

test("the tally counts every card cleared across batches", () => {
  let state = runOf(1);
  state = rateCard(state, true);
  state = addBatch(state, [card("b")]);
  state = rateCard(state, true);
  assert.equal(state.cleared, 2);
});
