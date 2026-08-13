/* The flashcard run queue. Scheduling and memory live on the backend; this
   file only keeps a missed card in the current run until the learner clears
   it, while retaining the first verdict that must be reported to FSRS.

   The run has no end. The server hands out cards a batch at a time, and a
   finished batch is reported and replaced while the learner is still working,
   so `deck` grows for as long as they keep going. Everything here is indexed
   into that growing deck rather than into any one batch: a lapsed card must
   survive the batch it arrived in, because the retry that clears it may well
   happen after the next batch has already been appended. */

import type { ApiFlashcard, ApiFlashcardResponse } from './api';

export interface ReviewState {
  /** every card handed to this run, in the order the server sent it */
  deck: ApiFlashcard[];
  /** deck indices still to see, in order, grows when a card lapses */
  queue: number[];
  /** where in the queue the current card sits */
  pos: number;
  /** unique cards cleared so far (anything but Again clears) */
  cleared: number;
  /** unique cards that lapsed at least once, as deck indices */
  lapsed: number[];
  /** the first verdict per card; retries are practice, not extra reviews */
  firstAnswers: Array<boolean | null>;
  /** deck entries below this index have already been accepted by the server */
  reported: number;
}

/** A finished batch, ready to send. `through` is the watermark to record once
 * the server has accepted it, carried alongside the cards because more may
 * arrive while the request is in flight. */
export interface PendingReport {
  cards: ApiFlashcardResponse[];
  through: number;
}

export function startReview(): ReviewState {
  return {
    deck: [],
    queue: [],
    pos: 0,
    cleared: 0,
    lapsed: [],
    firstAnswers: [],
    reported: 0,
  };
}

/** Append a freshly fetched batch. It queues behind anything still waiting,
 * so lapsed cards from the previous batch keep their place at the front and
 * are cleared before the new material starts. */
export function addBatch(state: ReviewState, cards: ApiFlashcard[]): ReviewState {
  const first = state.deck.length;
  return {
    ...state,
    deck: [...state.deck, ...cards],
    queue: [...state.queue, ...cards.map((_, i) => first + i)],
    firstAnswers: [...state.firstAnswers, ...cards.map(() => null)],
  };
}

/** Apply a verdict to the current card. Again sends it to the back of the
 * queue; Good clears it. Only the first verdict is retained for the backend,
 * because seeing the revealed answer makes every retry practice rather than
 * a second independent memory test. */
export function rateCard(state: ReviewState, correct: boolean): ReviewState {
  const card = state.queue[state.pos];
  const firstAnswers = state.firstAnswers.slice();
  if (firstAnswers[card] === null) firstAnswers[card] = correct;
  return {
    ...state,
    queue: correct ? state.queue : [...state.queue, card],
    pos: state.pos + 1,
    cleared: correct ? state.cleared + 1 : state.cleared,
    lapsed: !correct && !state.lapsed.includes(card) ? [...state.lapsed, card] : state.lapsed,
    firstAnswers,
  };
}

/** Nothing left to show, the learner has to wait for the next batch. */
export function outOfCards(state: ReviewState): boolean {
  return state.pos >= state.queue.length;
}

/** The batch to send, or null while any card the server has handed us is
 * still undecided. Because a card is decided the first time it is seen and
 * the queue serves unseen cards in deck order, the decided entries are always
 * a prefix, so "every entry above the watermark has a verdict" is the same
 * question as "is the outstanding batch finished".
 *
 * This goes true while the learner is still clearing that batch's lapses,
 * which is the point: reporting then leaves the round trip to the server
 * overlapping practice they were going to do anyway, instead of stalling them
 * at the bottom of the queue. */
export function pendingReport(state: ReviewState): PendingReport | null {
  if (state.deck.length === state.reported) return null;
  const verdicts = state.firstAnswers.slice(state.reported);
  if (verdicts.some((answer) => answer === null)) return null;
  return {
    cards: state.deck.slice(state.reported).map((card, i) => ({
      word: card.word,
      direction: card.direction,
      correct: verdicts[i] === true,
    })),
    through: state.deck.length,
  };
}

export function markReported(state: ReviewState, through: number): ReviewState {
  return { ...state, reported: through };
}
