import { strict as assert } from "node:assert";
import { test } from "node:test";
import { DEFAULT_NEXT, safeNext, withNext } from "./next.ts";

test("a path on this site is where we go", () => {
  assert.equal(safeNext("?next=/learn"), "/learn");
  assert.equal(safeNext("next=/practice/flashcards"), "/practice/flashcards");
  // a query of its own survives the round trip
  assert.equal(safeNext("?next=%2Flearn%3Fskill%3Dgreetings"), "/learn?skill=greetings");
});

test("nothing asked for lands on the course", () => {
  assert.equal(safeNext(""), DEFAULT_NEXT);
  assert.equal(safeNext("?other=1"), DEFAULT_NEXT);
  assert.equal(safeNext("?next="), DEFAULT_NEXT);
});

/* The reason this function exists. Somebody who has just typed a password is
   exactly who a redirect to another site wants to catch, so anything that
   isn't a path here is refused rather than trusted. */
test("anywhere off this site is refused", () => {
  assert.equal(safeNext("?next=https://evil.example"), DEFAULT_NEXT);
  // a URL wearing a path's clothes: //host is protocol-relative
  assert.equal(safeNext("?next=//evil.example"), DEFAULT_NEXT);
  assert.equal(safeNext("?next=" + encodeURIComponent("//evil.example/learn")), DEFAULT_NEXT);
  assert.equal(safeNext("?next=javascript:alert(1)"), DEFAULT_NEXT);
});

test("switching between the auth pages carries the destination along", () => {
  assert.equal(withNext("/login", "?next=/leaderboard"), "/login?next=%2Fleaderboard");
  // the default needs no spelling out
  assert.equal(withNext("/login", ""), "/login");
  assert.equal(withNext("/signup", "?next=https://evil.example"), "/signup");
});
