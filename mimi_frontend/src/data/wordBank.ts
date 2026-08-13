/* A word bank's tile order. The backend builds a bank as "the canonical
   answer's own tokens, then the authored distractors" and deliberately leaves
   it that way, in that order the tiles spell the answer out, so scrambling
   them is the client's job (see mimi_backend/src/server.rs, `bank`). Getting
   this wrong doesn't break anything visibly; it just makes every word bank
   answerable by tapping left to right, which is the one thing the exercise
   exists to prevent. */

/**
 * The bank's tokens in a random display order.
 *
 * Shuffle once per task and keep the result: the player tracks the user's
 * picks as indices into the bank it was handed, so a bank that reshuffled
 * mid-answer would rewrite what they had already picked. A retry gets a fresh
 * order, which is only fair, it's a second look at the same tiles.
 */
export function shuffledBank(bank: string[]): string[] {
  // Fisher-Yates, over a copy: the lesson's tasks belong to the caller
  const tiles = [...bank];
  for (let i = tiles.length - 1; i > 0; i--) {
    const j = Math.floor(Math.random() * (i + 1));
    [tiles[i], tiles[j]] = [tiles[j], tiles[i]];
  }
  return tiles;
}
