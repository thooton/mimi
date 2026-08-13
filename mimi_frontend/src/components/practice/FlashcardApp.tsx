import { useCallback, useEffect, useRef, useState } from 'react';
import { AnchorButton, Button, Icon } from '@blueprintjs/core';
import { fetchFlashcards, submitFlashcards } from '../../data/api';
import { useAuth } from '../../data/auth';
import {
  addBatch,
  markReported,
  outOfCards,
  pendingReport,
  rateCard,
  startReview,
} from '../../data/flashcards';
import type { PendingReport, ReviewState } from '../../data/flashcards';
import { speak, speechAvailable } from '../../data/speech';

/* Standalone vocabulary practice over the learner's real backend cards. The
   first answer is reported, while an Again stays in the run until it is
   cleared.

   A card's `direction` decides which memory the verdict lands on, target to
   source is recognition, source to target production, but it is deliberately
   not shown. A lesson labels its tasks because they vary in kind; here the
   only variation is which language the front happens to be in, which the word
   on the card already says.

   The run is endless, there is no deck to finish, only a batch at a time.
   Order matters at the seam: the server picks the most urgent cards from the
   learner's whole vocabulary, so asking for more before reporting the last
   batch would hand back the very cards just practiced. Report, then fetch.
   `pendingReport` goes true as soon as every card of a batch has a first
   verdict, which is usually a few retries before the queue actually empties,
   so that round trip normally finishes out of sight. */

/* Keep this in step with lesson-task-out in styles/motion.css. */
const EXIT_MS = 140;

export default function FlashcardApp() {
  const { user, ready } = useAuth();
  const [review, setReview] = useState<ReviewState>(startReview);
  const [targetLang, setTargetLang] = useState<string | null>(null);
  const [revealed, setRevealed] = useState(false);
  const [leaving, setLeaving] = useState(false);
  /* the server had nothing left to send, only reachable with no vocabulary,
     since a run cannot shrink the set of words the learner has encountered */
  const [exhausted, setExhausted] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const leaveTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const advancing = useRef(false);

  useEffect(() => () => clearTimeout(leaveTimer.current), []);

  /* Hand back the finished batch and take the next one. The watermark moves
     the moment the server accepts the report and not before, so a failed
     fetch never costs the learner their verdicts, and a failed report is
     simply offered again on the next rating. */
  const advance = useCallback(async (report: PendingReport | null) => {
    if (advancing.current) return;
    advancing.current = true;
    try {
      if (report) {
        await submitFlashcards(report.cards);
        setReview((state) => markReported(state, report.through));
      }
      const batch = await fetchFlashcards();
      setTargetLang(batch.target_lang);
      setExhausted(batch.cards.length === 0);
      setReview((state) => addBatch(state, batch.cards));
      setError(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      advancing.current = false;
    }
  }, []);

  useEffect(() => {
    if (!ready || !user) return;
    void advance(null);
  }, [advance, ready, user]);

  function reveal() {
    if (leaving || outOfCards(review)) return;
    setRevealed(true);
  }

  function rate(correct: boolean) {
    if (leaving || !revealed || outOfCards(review)) return;
    const next = rateCard(review, correct);
    setLeaving(true);
    leaveTimer.current = setTimeout(() => {
      setReview(next);
      setRevealed(false);
      setLeaving(false);
      const report = pendingReport(next);
      if (report) void advance(report);
    }, EXIT_MS);
  }

  useEffect(() => {
    if (outOfCards(review)) return;
    function onKeyDown(e: KeyboardEvent) {
      if (e.repeat || leaving) return;
      if (e.target instanceof HTMLButtonElement) return;
      if (e.key === 'Enter' || e.key === ' ') {
        e.preventDefault();
        if (revealed) rate(true);
        else reveal();
      } else if (revealed && e.key === '1') {
        rate(false);
      } else if (revealed && e.key === '2') {
        rate(true);
      }
    }
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  });

  if (!ready) return null;

  if (!user) {
    return (
      <EmptyState
        title="Learn a few words first"
        detail="Flashcards only use vocabulary you have already encountered in a lesson."
      />
    );
  }

  /* An error only stops the run once the learner has nothing left to answer.
     While cards remain, the next rating quietly tries the same report again. */
  if (error && outOfCards(review)) {
    const saving = pendingReport(review) !== null;
    return (
      <div className="learn-status">
        <p className="learn-status-title">
          {saving ? 'Couldn\'t save your flashcards' : 'Can\'t reach the mimi server'}
        </p>
        <p className="learn-status-detail">{error}</p>
        <Button
          intent="primary"
          text={saving ? 'Try saving again' : 'Retry'}
          onClick={() => void advance(pendingReport(review))}
        />
      </div>
    );
  }

  if (targetLang === null) return null;

  if (exhausted && outOfCards(review)) {
    return (
      <EmptyState
        title="No flashcards yet"
        detail="Finish a lesson to encounter some vocabulary, then come back here to practice it."
      />
    );
  }

  if (outOfCards(review)) {
    return (
      <div className="lesson-shell">
        <Tally cleared={review.cleared} />
        <div className="lesson-summary">
          <span className="lesson-summary-mark">
            <Icon icon="refresh" size={40} />
          </span>
          <p className="lesson-summary-note">Dealing the next few cards…</p>
        </div>
      </div>
    );
  }

  const card = review.deck[review.queue[review.pos]];
  const frontIsTarget = card.direction === 'target_to_source';

  return (
    <div className="lesson-shell">
      <Tally cleared={review.cleared} />

      <div
        key={review.pos}
        className={`lesson-body${leaving ? ' is-out' : review.pos > 0 ? ' is-in' : ''}`}
      >
        <div
          className={`flash-stage${revealed ? ' is-revealed' : ''}`}
          onClick={() => !revealed && reveal()}
        >
          <div className="flash-stage-word">
            <h1 className="flash-word">{card.front}</h1>
            {frontIsTarget && speechAvailable() && (
              <button
                className="flash-say"
                type="button"
                aria-label={`Hear "${card.front}"`}
                onClick={(e) => {
                  e.stopPropagation();
                  speak(card.front, targetLang);
                }}
              >
                <Icon icon="volume-up" size={19} />
              </button>
            )}
          </div>

          {revealed && (
            <div className="flash-reveal">
              <p className="flash-answer">{card.back}</p>
              {card.example && (
                <p className="flash-example">
                  {speechAvailable() && (
                    <button
                      className="flash-example-say"
                      type="button"
                      aria-label={`Hear "${card.example}"`}
                      onClick={(e) => {
                        e.stopPropagation();
                        speak(card.example!, targetLang);
                      }}
                    >
                      <Icon icon="volume-up" size={14} />
                    </button>
                  )}
                  {card.example}
                </p>
              )}
            </div>
          )}
        </div>
      </div>

      <div className="lesson-foot">
        {revealed ? (
          <div className="flash-grades" role="group" aria-label="Did you remember it?">
            <button className="flash-grade is-again" type="button" onClick={() => rate(false)}>
              <span className="flash-grade-label">Again</span>
              <span className="flash-grade-when tnum">1</span>
            </button>
            <button className="flash-grade" type="button" onClick={() => rate(true)}>
              <span className="flash-grade-label">Good</span>
              <span className="flash-grade-when tnum">2</span>
            </button>
          </div>
        ) : (
          <Button large intent="primary" text="Show answer" onClick={reveal} />
        )}
      </div>
    </div>
  );
}

/* A lesson's chrome is a progress bar because a lesson ends. This one counts
   up instead: there is no denominator to fill, and the learner stops by
   leaving. */
function Tally({ cleared }: { cleared: number }) {
  return (
    <div className="lesson-top">
      <a className="lesson-quit" href="/learn" aria-label="Back to learning">
        <Icon icon="cross" size={18} />
      </a>
      <span className="lesson-count flash-tally tnum">
        {cleared} {cleared === 1 ? 'card' : 'cards'}
      </span>
    </div>
  );
}

function EmptyState({ title, detail }: { title: string; detail: string }) {
  return (
    <div className="lesson-shell">
      <div className="lesson-summary">
        <span className="lesson-summary-mark">
          <Icon icon="book" size={40} />
        </span>
        <h1 className="lesson-summary-title">{title}</h1>
        <p className="lesson-summary-note">{detail}</p>
        <AnchorButton large intent="primary" text="Go to lessons" href="/learn" />
      </div>
    </div>
  );
}
