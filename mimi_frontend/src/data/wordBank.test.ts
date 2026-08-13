import { strict as assert } from "node:assert";
import { test } from "node:test";
import { shuffledBank } from "./wordBank.ts";

test("the tiles are the bank's, duplicates and all", () => {
  // a word appearing twice in the answer appears twice in the bank, and each
  // copy is a tile the user can spend
  const bank = ["muy", "bien", "muy", "gracias"];
  const tiles = shuffledBank(bank);
  assert.deepEqual([...tiles].sort(), [...bank].sort());
});

test("the bank it was handed is left alone", () => {
  const bank = ["uno", "dos", "tres", "cuatro", "cinco"];
  shuffledBank(bank);
  assert.deepEqual(bank, ["uno", "dos", "tres", "cuatro", "cinco"]);
});

// the regression this module exists for: the backend's order spells the answer
// out, so serving it through unscrambled makes every bank tappable left to
// right. Ten tries of a 6-tile bank land on the original order by chance about
// once in 10^13 runs.
test("the answer's own order does not survive", () => {
  const bank = ["Hello", "there", "my", "old", "dear", "friend"];
  const scrambled = Array.from({ length: 10 }, () => shuffledBank(bank));
  assert.ok(
    scrambled.some((tiles) => !tiles.every((tile, i) => tile === bank[i])),
    "shuffledBank returned the bank's own order every time",
  );
});

test("a bank too small to shuffle still comes back whole", () => {
  assert.deepEqual(shuffledBank([]), []);
  assert.deepEqual(shuffledBank(["sí"]), ["sí"]);
});
