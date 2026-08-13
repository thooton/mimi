/* Hanging a lesson's hints on the words they explain.

   The backend sends an exercise's prompt as one string ("Yo soy un hombre.")
   and its hints beside it as glosses — {text: "hombre", meanings: [...]} —
   quoting the lexical run of the prompt each one is about, in the order those
   runs occur. Sentence punctuation is deliberately outside that quote so it
   does not become interactive. Nothing but the quote joins the two, so the
   player has to find it back in the sentence before it can put a hint under
   the word.

   Which the backend's two promises make a walk rather than a search: the
   quotes are exact (surface text, punctuation and all) and they come in
   order, so one cursor moving forward places them all. A gloss the sentence
   doesn't quote back is a backend bug either way; it comes back as an orphan
   for the caller to show some other way, because a hint the user never sees
   is worse than a hint out of place. */

import type { ApiGloss } from './api';

/** a run of the sentence: either plain text or a word with a hint on it */
export interface GlossSpan {
  text: string;
  gloss?: ApiGloss;
}

export interface GlossedText {
  /** the whole sentence, in order — glossed runs and the plain text between */
  spans: GlossSpan[];
  /** the glosses with no word to sit under (see above) */
  orphans: ApiGloss[];
}

/**
 * Cut `text` into runs, each glossed word its own run.
 *
 * The search runs forward from the last hint placed, which is what keeps the
 * second "que" of a sentence under the second "que"'s hint.
 */
export function attachGlosses(text: string, glosses: ApiGloss[]): GlossedText {
  const spans: GlossSpan[] = [];
  const orphans: ApiGloss[] = [];
  let cursor = 0;

  for (const gloss of glosses) {
    const at = gloss.text ? text.indexOf(gloss.text, cursor) : -1;
    if (at === -1) {
      orphans.push(gloss);
      continue;
    }
    if (at > cursor) spans.push({ text: text.slice(cursor, at) });
    spans.push({ text: gloss.text, gloss });
    cursor = at + gloss.text.length;
  }

  if (cursor < text.length) spans.push({ text: text.slice(cursor) });
  return { spans, orphans };
}
