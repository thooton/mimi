/* Client-side grading. The backend serves lessons with their answers — each
   the text plus the spans of it that prove each concept — and expects the
   client's verdicts back, per exercise and per concept (see mimi_backend
   AGENTS.md, "Grading (the client's job)"). The judging itself lives in
   linguisticGrade.ts; this module only adapts it to the shapes the lesson
   player and the API speak. */

// the .ts extension keeps this import resolvable when node runs the tests
// straight off the filesystem (see the test script in package.json)
import { linguisticGrade, type Answer } from './linguisticGrade.ts';

export interface Verdict {
  /** whether the exercise as a whole passed */
  correct: boolean;
  /** per-concept verdict, keyed by concept id (only concepts the answer marks up) */
  concepts: Record<string, boolean>;
  /** the canonical answer's plain text, to show when the user is wrong */
  canonical: string;
}

/**
 * Grade what the user typed against an exercise's accepted answers (the
 * first is canonical). Answers may be missing accents or carry a typo, and
 * a half-right sentence still credits the concepts it got right — see
 * linguisticGrade.ts for exactly how forgiving that is.
 */
export function grade(
  input: string,
  answers: Answer[],
  expectedConcepts: string[] = [],
): Verdict {
  // An exercise with no accepted answers is a backend bug, but it shouldn't
  // take the lesson down with it: nothing to match, so nothing is right.
  if (answers.length === 0) {
    return {
      correct: false,
      concepts: Object.fromEntries(expectedConcepts.map((id) => [id, false])),
      canonical: '',
    };
  }

  const { overallCorrect, concepts } = linguisticGrade(input, answers);
  const reported = Object.fromEntries(concepts.map((c) => [c.id, c.correct]));
  /* A concept with no span takes the whole answer's verdict, and the backend
     leaves a concept unspanned in two cases.

     One is deliberate: an exercise testing a single concept is graded all or
     nothing, because the sentence *is* that concept's question and there is
     no credit to divide — "Juan girl" for "The girl." is a wrong answer, not
     a right `girl`.

     The other is unavoidable: a translation need not contain a glossary
     spelling the loader can locate ("apples" for a glossary holding "apple").
     Those concepts still belong to the exercise, and the whole answer's
     verdict is the most honest evidence available — less precise, but it
     never drops the word from the submission and strands the lesson at its
     final question. */
  for (const id of expectedConcepts) {
    if (!(id in reported)) reported[id] = overallCorrect;
  }
  return {
    correct: overallCorrect,
    concepts: reported,
    canonical: answers[0].text,
  };
}

/**
 * Grade a word-bank answer: the tokens the user picked out of the bank, in
 * the order they picked them. The bank's tokens are bare words, so the
 * assembled string can never reproduce the canonical answer's punctuation —
 * and doesn't have to: the tokens are simply joined and passed to the same
 * lenient grading a typed answer gets, which ignores punctuation and casing.
 * That's deliberate: it means the course author doesn't have to keep the
 * bank's tokens in punctuational agreement with the accepted answers.
 */
export function gradeWordBank(
  chosen: string[],
  answers: Answer[],
  expectedConcepts: string[] = [],
): Verdict {
  return grade(chosen.join(' '), answers, expectedConcepts);
}
