import { strict as assert } from "node:assert";
import { test } from "node:test";
import { ago, clockTime, withMessage } from "./inbox.ts";
import type { Message, Thread } from "./inbox.ts";

function thread(username: string, sentAt: number, unread = false): Thread {
  return {
    with: username,
    display: username,
    lastSender: username,
    last: "…",
    sentAt,
    unread,
  };
}

function message(id: number, from: string, body: string, sentAt: number): Message {
  return { id, from, body, sentAt };
}

/* The whole of the list's behaviour when something arrives: the pair is the
   thread, so a reply moves the row that is already there rather than opening
   a second one beside it. */
test("a message moves its conversation to the top instead of adding one", () => {
  const threads = [thread("ana", 300), thread("ren", 200)];

  const after = withMessage(threads, "ren", "Ren", message(9, "ren", "hey", 400), true);

  assert.deepEqual(
    after.map((row) => row.with),
    ["ren", "ana"],
  );
  assert.equal(after.length, 2);
  assert.equal(after[0].last, "hey");
  assert.equal(after[0].lastSender, "ren");
  assert.equal(after[0].sentAt, 400);
  assert.equal(after[0].unread, true);
  // the name comes from the frame, so somebody who has renamed themselves is
  // not left quoted under the name the list was built with
  assert.equal(after[0].display, "Ren");
});

test("the first message of a conversation opens a row for it", () => {
  const after = withMessage([], "ana", "Ana", message(1, "sam", "hola", 10), false);

  assert.equal(after.length, 1);
  assert.equal(after[0].with, "ana");
  assert.equal(after[0].unread, false);
});

/* Coarse on purpose: what a row answers is whether the conversation is still
   warm, not exactly when it was. */
test("a thread wears its age at the resolution somebody reads it at", () => {
  const now = 1_000_000;
  assert.equal(ago(now, now), "just now");
  assert.equal(ago(now - 60, now), "1 minute ago");
  assert.equal(ago(now - 45 * 60, now), "45 minutes ago");
  assert.equal(ago(now - 60 * 60, now), "1 hour ago");
  assert.equal(ago(now - 30 * 3600, now), "30 hours ago");
  assert.equal(ago(now - 3 * 86400, now), "3 days ago");
  assert.equal(ago(now - 100 * 86400, now), "3 months ago");
  assert.equal(ago(now - 800 * 86400, now), "2 years ago");
  // a clock that disagrees with the server's by a second reads as now, not as
  // a message from the future
  assert.equal(ago(now + 1, now), "just now");
});

test("a message wears the time of day it was sent", () => {
  const at = new Date(2026, 7, 12, 9, 5).getTime() / 1000;
  assert.equal(clockTime(at), "9:05");
  assert.equal(clockTime(at + 12 * 3600 + 19 * 60), "21:24");
});
