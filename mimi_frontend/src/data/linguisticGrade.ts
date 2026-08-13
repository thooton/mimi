/**
 * linguisticGrade, language-agnostic fuzzy grader for short free-text answers.
 *
 * Grades a user's response against a set of accepted answers, forgiving
 * missing accents, punctuation/casing differences, and single-character
 * typos in words long enough that a typo can't produce a different word.
 *
 * An answer is its text plus the spans of that text which prove each concept:
 * `{ text: "¡Hola, Juan!", words: [{ word: "hola", start: 1, end: 5 }] }`. The
 * backend works those spans out once, when it loads the course, so nothing
 * here has to look for anything. The first answer is the canonical one; ties
 * in grading go to the earliest answer.
 *
 * Works for ANY language: tokenization is over Unicode letters/numbers, and
 * accents are forgiven via NFD decomposition + stripping combining marks,
 * rather than a per-language list of diacritics.
 */

// --- grading tuning constants -----------------------------------------------

// How long a word has to be before we forgive a single-character typo in it.
// One edit turns plenty of short words into other short words ("no" -> "nos"),
// so short words have to be spelled correctly.
const MIN_TYPO_LEN = 4;

// Costs for aligning the user's tokens against an answer's tokens.
// An exact token is free. One differing only by accents or a single typo is
// forgiven, but not quite free, so a nailed answer always beats a close one.
// Dropping, adding or substituting a whole token is a real mistake.
//
// Keep SUB_COST < 2 * GAP_COST, otherwise the aligner would rather delete a
// token and insert another than call it a substitution.
const NEAR_COST = 1;
const GAP_COST = 3;
const SUB_COST = 4;

// Aligning is quadratic in the response length, and the response may come from
// an untrusted client. Cut absurdly long responses off: they were wrong anyway.
const MAX_RESPONSE_TOKENS = 100;

// --- public types -------------------------------------------------------------

export interface ConceptGrade {
  id: string;
  correct: boolean;
}

export interface LinguisticGrade {
  overallCorrect: boolean;
  /** One entry per concept marked in the matched answer, in order of appearance. */
  concepts: ConceptGrade[];
}

/**
 * One accepted answer, exactly as the backend serves it (`ApiAnswer` in
 * api.ts; declared structurally here so this module keeps no dependencies).
 * `start`/`end` index `text` in UTF-16 code units, which is what `slice` and
 * `length` already speak, so nothing here converts anything.
 */
export interface Answer {
  text: string;
  words: { word: string; start: number; end: number }[];
}

// --- text handling ------------------------------------------------------------

interface Token {
  text: string;
  start: number;
  end: number;
}

const WORD_CHAR = /[\p{L}\p{N}]/u;

/**
 * Split text into comparable words: lowercased runs of Unicode letters and
 * numbers, with everything else treated as a separator. So "¡Hola, Juan!",
 * "hola juan" and "  HOLA,  JUAN!! " all tokenize the same. Apostrophes
 * vanish entirely ("I'm" is the single token "im") rather than splitting a
 * word in two. Accents survive here; they are forgiven later, at comparison.
 *
 * Offsets are string indices into the original text, so an answer's spans can
 * be mapped onto token indices.
 */
function tokenize(s: string): Token[] {
  const tokens: Token[] = [];
  let text = "";
  let start = 0;
  let end = 0;
  let i = 0;
  for (const c of s) {
    if (c === "'" || c === "\u2019") {
      // apostrophes glue words together: "I'm" == "im"
    } else if (WORD_CHAR.test(c)) {
      if (text === "") start = i;
      text += c.toLowerCase();
      end = i + c.length;
    } else if (text !== "") {
      tokens.push({ text, start, end });
      text = "";
    }
    i += c.length;
  }
  if (text !== "") tokens.push({ text, start, end });
  return tokens;
}

// --- token comparison ---------------------------------------------------------

/**
 * Strip the accents a learner is likely to leave off, in ANY language:
 * decompose to NFD (so "ñ" becomes "n" + combining tilde, "é" becomes
 * "e" + combining acute, ...) and drop all combining marks. Returns the
 * result as an array of code points for edit-distance checks.
 */
function foldAccents(s: string): string[] {
  return Array.from(s.normalize("NFD").replace(/\p{M}/gu, ""));
}

/** Are these two strings equal code point by code point? */
function sameChars(a: string[], b: string[]): boolean {
  return a.length === b.length && a.every((c, i) => c === b[i]);
}

/**
 * Are these two words within a single character edit, one insertion,
 * deletion, substitution, or swap of adjacent characters (the classic typo)?
 */
function withinOneEdit(a: string[], b: string[]): boolean {
  const lengthDiff = Math.abs(a.length - b.length);
  if (lengthDiff === 0) {
    const differing: number[] = [];
    for (let i = 0; i < a.length; i++) {
      if (a[i] !== b[i]) differing.push(i);
    }
    if (differing.length <= 1) return true; // identical, or one substitution
    if (differing.length === 2) {
      const [i, j] = differing;
      return j === i + 1 && a[i] === b[j] && a[j] === b[i]; // transposition
    }
    return false;
  }
  if (lengthDiff === 1) {
    // one insertion or deletion: the words must agree either side of it
    const [long, short] = a.length > b.length ? [a, b] : [b, a];
    let i = 0;
    while (i < short.length && long[i] === short[i]) i++;
    for (let k = i; k < short.length; k++) {
      if (long[k + 1] !== short[k]) return false;
    }
    return true;
  }
  return false;
}

/** What it costs to call a response token an attempt at an answer token. */
function tokenCost(answer: string, response: string): number {
  if (answer === response) return 0;
  const a = foldAccents(answer);
  const b = foldAccents(response);
  // the user didn't (or couldn't) type the accents, that's fine
  if (sameChars(a, b)) return NEAR_COST;
  // ...and so is one slip of the finger, in a word long enough that one slip
  // can't have turned it into a different word
  if (Math.max(a.length, b.length) >= MIN_TYPO_LEN && withinOneEdit(a, b)) {
    return NEAR_COST;
  }
  return SUB_COST;
}

// --- answers ------------------------------------------------------------------

interface Span {
  concept: string;
  start: number; // inclusive token index
  end: number; //   exclusive token index
}

/** One accepted answer: normalized tokens, plus which tokens prove each concept. */
interface ParsedAnswer {
  tokens: string[];
  spans: Span[];
}

/** Prepare one served answer for grading. */
function parseAnswer(answer: Answer): ParsedAnswer {
  const tokens = tokenize(answer.text);
  // a mark covers a range of characters; grading works on tokens, so
  // translate each range into the tokens that overlap it
  const spans = answer.words.map((mark) => {
    const firstIdx = tokens.findIndex((t) => t.end > mark.start);
    const start = firstIdx === -1 ? tokens.length : firstIdx;
    let end = start;
    for (let i = tokens.length - 1; i >= 0; i--) {
      if (tokens[i].start < mark.end) {
        end = i + 1;
        break;
      }
    }
    return { concept: mark.word, start, end: Math.max(end, start) };
  });
  return { tokens: tokens.map((t) => t.text), spans };
}

// --- alignment ------------------------------------------------------------------

interface Alignment {
  // how far off the response was; 0 means a perfect reproduction
  distance: number;
  // for each answer token, whether the response produced it (exactly, or
  // close enough to forgive)
  matched: boolean[];
}

/**
 * Line the user's tokens up against one answer's tokens.
 *
 * This is Needleman-Wunsch (Levenshtein with a traceback) over whole tokens
 * rather than characters: the user may have dropped a word, added one, or
 * used a different one, and we need to know which words survived so we can
 * score the concepts attached to them. Word-level costs come from `tokenCost`,
 * which is where accents and typos are forgiven.
 */
function align(answerTokens: string[], response: string[]): Alignment {
  const m = answerTokens.length;
  const n = response.length;
  const cost = answerTokens.map((a) => response.map((b) => tokenCost(a, b)));

  // dp[i][j] = cheapest alignment of the first i answer tokens against the
  // first j response tokens
  const dp: number[][] = Array.from({ length: m + 1 }, () =>
    new Array<number>(n + 1).fill(0),
  );
  for (let i = 1; i <= m; i++) dp[i][0] = dp[i - 1][0] + GAP_COST;
  for (let j = 1; j <= n; j++) dp[0][j] = dp[0][j - 1] + GAP_COST;
  for (let i = 1; i <= m; i++) {
    for (let j = 1; j <= n; j++) {
      dp[i][j] = Math.min(
        dp[i - 1][j - 1] + cost[i - 1][j - 1],
        dp[i - 1][j] + GAP_COST,
        dp[i][j - 1] + GAP_COST,
      );
    }
  }

  // walk the table back to find which answer tokens the user produced
  const matched = new Array<boolean>(m).fill(false);
  let i = m;
  let j = n;
  while (i > 0 && j > 0) {
    if (dp[i][j] === dp[i - 1][j - 1] + cost[i - 1][j - 1]) {
      matched[i - 1] = cost[i - 1][j - 1] <= NEAR_COST;
      i--;
      j--;
    } else if (dp[i][j] === dp[i - 1][j] + GAP_COST) {
      i--; // the user left this token out
    } else {
      j--; // the user typed a token the answer doesn't have
    }
  }

  return { distance: dp[m][n], matched };
}

// --- grading --------------------------------------------------------------------

/**
 * Grade a user's response against the accepted answers.
 *
 * The response is graded against whichever answer it comes closest to,
 * users are judged against the phrasing they attempted, not whichever the
 * course author listed first. Ties go to the earlier (more canonical) answer.
 *
 * `overallCorrect` requires every token of the matched answer to come back
 * (exact or forgiven) with nothing extra added. A concept is correct when
 * the user produced every token of the span that demonstrates it; a concept
 * marked in several places has to be right in all of them.
 */
export function linguisticGrade(
  provided: string,
  answers: Answer[],
): LinguisticGrade {
  if (answers.length === 0) {
    throw new Error("linguisticGrade: at least one answer is required");
  }

  const response = tokenize(provided)
    .slice(0, MAX_RESPONSE_TOKENS)
    .map((t) => t.text);

  let best: { answer: ParsedAnswer; alignment: Alignment } | undefined;
  for (const served of answers) {
    const answer = parseAnswer(served);
    const alignment = align(answer.tokens, response);
    if (best === undefined || alignment.distance < best.alignment.distance) {
      best = { answer, alignment };
    }
  }
  const { answer, alignment } = best!;

  // The whole answer is right only if every one of its tokens came back and
  // the user added nothing extra. (The alignment is monotonic, so "every
  // answer token matched" plus "the lengths agree" means every response
  // token was used too.)
  const overallCorrect =
    alignment.matched.every(Boolean) && response.length === answer.tokens.length;

  // One entry per concept, in first-appearance order. A concept marked in
  // several spans has to be right in all of them.
  const concepts: ConceptGrade[] = [];
  const indexById = new Map<string, number>();
  for (const span of answer.spans) {
    if (span.start >= span.end) continue; // a span covering no token proves nothing
    const correct = alignment.matched.slice(span.start, span.end).every(Boolean);
    const existing = indexById.get(span.concept);
    if (existing === undefined) {
      indexById.set(span.concept, concepts.length);
      concepts.push({ id: span.concept, correct });
    } else {
      concepts[existing].correct = concepts[existing].correct && correct;
    }
  }

  return { overallCorrect, concepts };
}
