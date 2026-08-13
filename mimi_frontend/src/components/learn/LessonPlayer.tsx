import { Fragment, useEffect, useId, useMemo, useRef, useState } from 'react';
import { Button, Icon, Popover } from '@blueprintjs/core';
import type { ApiGloss, ApiLesson, ApiMark, ApiMaterial, ApiResponse, ApiSubmitResult } from '../../data/api';
import type { Verdict } from '../../data/grading';
import { attachGlosses } from '../../data/glosses';
import { grade, gradeWordBank } from '../../data/grading';
import { languageName } from '../../data/languages';
import { advanceQueue, clearedCount, queueOf } from '../../data/lessonQueue';
import { DEFAULT_NEXT } from '../../data/next';
import { shuffledBank } from '../../data/wordBank';
import { playTriumph, playVerdict } from '../../data/sounds';
import { markdown } from './markdown';

/* Plays one served lesson, task by task, in the order the backend sent them.
   A lesson is part authored script and part generated for this user, but the
   client doesn't care which is which: a task is either material to read (and
   hear) or an exercise to answer.

   Grading is local (the lesson arrives with its answers; see grading.ts);
   the verdicts go back to the backend at the end, which is what advances the
   user's position on the map (unless the lesson was a re-take, since reviewing
   moves nothing). Material carries no verdict and changes no memory state.

   A wrongly-answered exercise isn't done, though: a retry of it joins the end
   of the queue and keeps reappearing until it's answered right (see
   lessonQueue.ts), and the returning question wears a "previous mistake" tag
   so it isn't mistaken for the lesson repeating itself. Only the first
   attempt is reported: the backend still hears the answer was wrong; the
   retries are just the user practicing.

   The lesson's length is what the backend served and nothing the user does
   changes it, so the count and the strip are read off `cleared` and
   `total` rather than off the queue, which does grow. A miss holds both
   where they are, the finish line staying put while the user is not yet at
   it, and the strip fills exactly as the last retry is answered. */

/* Between tasks the old one shifts out to the left and the next shifts in
   from the right (both keyframes live in styles/motion.css).
   EXIT_MS is how long the old task is held on screen before the swap; it
   must match the stylesheet's lesson-task-out duration, so the exit
   finishes exactly as the new task lands. (The whole-lesson exit works the
   same way, but LearnApp owns that timer: it holds the unmount while this
   player's is-quitting fade reveals the map underneath.) */
const EXIT_MS = 140;

/** "es->en" → "Translate to English" */
function directionLabel(direction: string): string {
  return `Translate to ${languageName(direction.split('->')[1])}`;
}

interface Props {
  lesson: ApiLesson;
  /** the language being learned, so spoken lines get the right voice */
  targetLang: string;
  /** learning without an account: the summary offers to save what they've done */
  guest: boolean;
  /** hand the collected verdicts to the backend; resolves with the score */
  onFinish: (responses: ApiResponse[]) => Promise<ApiSubmitResult>;
  /** leave the lesson. Called the moment the exit fade starts; LearnApp
      mounts the map beneath us and
      holds the unmount until the fade is done (see quit()) */
  onExit: () => void;
}

export default function LessonPlayer({ lesson, targetLang, guest, onFinish, onExit }: Props) {
  const [queue, setQueue] = useState(() => queueOf(lesson.tasks));
  const [index, setIndex] = useState(0);
  /* counts every task swap. A wrong answer keeps `index` put (the queue
     rotates underneath it), so this rather than the index keys the body's
     remount, the input's focus and the bank's reshuffle */
  const [swap, setSwap] = useState(0);
  const [input, setInput] = useState('');
  /** the bank tokens picked so far, as indices into `bank`, in pick order */
  const [chosen, setChosen] = useState<number[]>([]);
  const [verdict, setVerdict] = useState<Verdict | null>(null);
  const [responses, setResponses] = useState<ApiResponse[]>([]);
  const [result, setResult] = useState<ApiSubmitResult | null>(null);
  const [submitting, setSubmitting] = useState(false);
  const [submitError, setSubmitError] = useState<string | null>(null);
  /* true while the old task plays its shift-out: the body keeps rendering
     the finished task (progress strip and footer unmoved) until the timer
     commits the swap */
  const [leaving, setLeaving] = useState(false);
  /* true while the whole player settles away on the way back to the map */
  const [quitting, setQuitting] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);
  const leaveTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);

  useEffect(() => () => clearTimeout(leaveTimer.current), []);

  /* The lesson as served: the denominator of everything on the progress
     line, fixed for the life of the player. `queue.length` is a different
     number the moment anything is missed: it is the run, not the lesson. */
  const total = lesson.tasks.length;
  const cleared = clearedCount(queue, index, total);
  const entry = queue[index];
  const current = entry.task;
  const answered = verdict !== null;
  // a word-bank exercise without its bank is a backend bug; fall back to the
  // text input rather than strand the user on an unanswerable task
  const served =
    current.kind === 'exercise' &&
    current.task.kind === 'word_bank' &&
    current.task.bank?.length
      ? current.task.bank
      : null;
  /* The tiles, scrambled once and then held still. The backend hands the bank
     over answer-first on purpose and leaves the shuffling to us (wordBank.ts),
     and `chosen` holds indices into whatever order we settle on, so this must
     not move again while the user is picking. Keyed on the swap as well as
     the bank itself, so a retry of the same exercise gets a fresh order. */
  const bank = useMemo(() => (served ? shuffledBank(served) : null), [swap, served]);

  useEffect(() => {
    inputRef.current?.focus();
  }, [swap]);

  /* Enter settles the task on screen: it advances after a verdict (or on
     material, which has nothing to check), and on a word bank it checks:
     the text input's Enter comes from its form's own submit, but a word bank
     has no form. (The input is disabled once answered, so the "continue"
     half has to live here regardless.) A focused button handles its own
     Enter as a click, so it must not also fire here. */
  useEffect(() => {
    if (result) return;
    function onKeyDown(e: KeyboardEvent) {
      if (e.key !== 'Enter' || e.repeat || submitting) return;
      if (e.target instanceof HTMLButtonElement) return;
      if (current.kind === 'material' || answered) {
        e.preventDefault(); // stop a focused Continue button firing a second advance
        void advance();
      } else if (bank) {
        check();
      }
    }
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  });

  function check() {
    if (current.kind !== 'exercise' || answered || leaving || quitting) return;
    let v: Verdict;
    if (bank) {
      if (chosen.length === 0) return;
      v = gradeWordBank(chosen.map((i) => bank[i]), current.task.answers, current.task.words);
    } else {
      if (!input.trim()) return;
      v = grade(input, current.task.answers, current.task.words);
    }
    setVerdict(v);
    playVerdict(v.correct);
  }

  /** finish the task on screen and move on, submitting once the queue runs out */
  async function advance() {
    // mid-shift or mid-exit (a double-press of Continue, Enter, or the X)
    if (leaving || quitting || submitting) return;
    const step = advanceQueue(queue, index, verdict, responses);

    /** retire the finished task: the queue moves on and the answer controls
        come back empty, ready for whatever is next */
    const commit = () => {
      setQueue(step.queue);
      setResponses(step.responses);
      setVerdict(null);
      setInput('');
      setChosen([]);
    };

    if (!step.done) {
      /* hold the finished task while it shifts out, then land the next one
         with its shift-in: all the state commits in one batch so the exit
         and entrance read as a single push, progress strip included */
      setLeaving(true);
      leaveTimer.current = setTimeout(() => {
        commit();
        // every task retires where it stands, a wrong one leaving a retry of
        // itself at the end of the (now longer) queue: the position always moves
        setIndex(index + 1);
        setSwap(swap + 1);
        setLeaving(false);
      }, EXIT_MS);
      return;
    }

    /* Keep the final, answered task intact while the completion request is in
       flight. If the server rejects it, clearing the verdict here would turn
       a submit failure into what looks like the same unanswered question in
       an endless 5/5 loop. */
    setSubmitError(null);
    setSubmitting(true);
    try {
      setResult(await onFinish(step.responses));
      playTriumph();
    } catch (e) {
      setSubmitError(e instanceof Error ? e.message : String(e));
    } finally {
      setSubmitting(false);
    }
  }

  /** leave the lesson: start the exit fade (css: lesson-shell-out) and tell
      LearnApp right away, so it can mount the map underneath us. The shell
      then fades to transparency over the page it's returning to, and
      LearnApp unmounts us as the fade completes. The X and the summary's
      Continue share this path, so a finished lesson leaves as gently as an
      abandoned one. */
  function quit() {
    if (quitting) return;
    clearTimeout(leaveTimer.current); // a mid-shift swap dies with the lesson
    setQuitting(true);
    onExit();
  }

  /* ---- summary ---- */
  if (result) {
    const castle = result.passed !== null;
    const perfect = result.correct === result.total;
    return (
      <div className={`lesson-shell${quitting ? ' is-quitting' : ''}`}>
        <div className="lesson-summary">
          <span className={perfect ? 'lesson-summary-mark is-perfect' : 'lesson-summary-mark'}>
            <Icon icon={perfect ? 'star' : 'tick'} size={40} />
          </span>
          <h1 className="lesson-summary-title">{castle ? (result.passed ? 'Castle passed!' : 'Keep raising your levels') : perfect ? 'Perfect lesson!' : 'Lesson complete'}</h1>
          <p className="lesson-summary-score">
            You got <strong>{result.correct}</strong> of <strong>{result.total}</strong> right.
          </p>
          {castle && !result.passed && <p className="lesson-summary-note">Review the skills in this stretch, then try a fresh castle test.</p>}
          {guest && <SaveProgress />}
          <Button
            large
            /* the save offer is the thing to look at when there is one, so
               Continue steps back and becomes the way past it */
            intent={guest ? 'none' : 'primary'}
            text={guest ? 'Not now' : castle && !result.passed ? 'Back to the tree' : 'Continue'}
            onClick={quit}
          />
        </div>
      </div>
    );
  }

  /* a wrong answer re-queues this exercise, so the apparent last task isn't
     the end after all, so don't promise a finish that isn't coming. The run is
     what has to be at its end here, not the lesson: with a retry outstanding
     there is still something to answer after this. */
  const last = index + 1 === queue.length && (!answered || verdict.correct);

  /* A correct answer counts as soon as it's checked rather than when Continue
     is pressed, so the strip and the count move with the green feedback
     instead of a beat after it. Both read this one number, so they cannot
     disagree, and neither can pass `total`, since the only thing that
     advances it is a task being cleared for good. */
  const scored = cleared + (verdict?.correct ? 1 : 0);
  return (
    <div className={`lesson-shell${quitting ? ' is-quitting' : ''}`}>
      <div className="lesson-top">
        <button className="lesson-quit" type="button" aria-label="Quit lesson" onClick={quit}>
          <Icon icon="cross" size={18} />
        </button>
        <div className="lesson-progress">
          <div
            className="lesson-progress-fill"
            style={{ width: `${(scored / total) * 100}%` }}
          />
        </div>
        <span className="lesson-count tnum">
          {scored} / {total}
        </span>
      </div>

      {/* remounted per task (key) so the shift-in replays on every swap;
          the very first task skips it, since the shell's own entrance animation
          already covers that arrival */}
      <div
        key={swap}
        className={`lesson-body${leaving ? ' is-out' : swap > 0 ? ' is-in' : ''}`}
      >
        {current.kind === 'material' ? (
          <Material material={current.task} targetLang={targetLang} />
        ) : (
          <>
            <p className="eyebrow lesson-kind">{directionLabel(current.task.direction)}</p>
            {/* A question the user has already seen and missed looks exactly
                like a fresh one, which is its own small insult: they read it
                as the lesson repeating itself rather than as the mistake
                coming back around. Say which it is. */}
            {entry.retry && (
              <p className="lesson-retry">
                <Icon icon="warning-sign" size={14} /> Previous mistake
              </p>
            )}
            {current.task.new_words.length > 0 && <p className="lesson-new-word"><Icon icon="new-object" size={14} /> New word</p>}
            <Prompt
              text={current.task.prompt}
              glosses={current.task.prompt_glosses}
              newWords={current.task.new_words}
            />

            {bank ? (
              <WordBank
                bank={bank}
                chosen={chosen}
                disabled={answered}
                onChoose={(i) => setChosen([...chosen, i])}
                onUnchoose={(pos) => setChosen(chosen.filter((_, p) => p !== pos))}
              />
            ) : (
              <form
                onSubmit={(e) => {
                  e.preventDefault();
                  if (answered) void advance();
                  else check();
                }}
              >
                <input
                  ref={inputRef}
                  className="lesson-input"
                  type="text"
                  value={input}
                  onChange={(e) => setInput(e.target.value)}
                  placeholder="Type your answer…"
                  autoComplete="off"
                  autoCapitalize="off"
                  spellCheck={false}
                  disabled={answered}
                />
              </form>
            )}

            {verdict && (
              <div
                className={verdict.correct ? 'lesson-feedback is-correct' : 'lesson-feedback is-wrong'}
              >
                <div className="lesson-feedback-head">
                  <Icon icon={verdict.correct ? 'tick-circle' : 'cross-circle'} size={20} />
                  <strong>{verdict.correct ? 'Correct!' : 'Not quite'}</strong>
                </div>
                {/* The answer, right or wrong. A correct answer used to be
                    followed by every word of it and everything each word can
                    mean: a dictionary column under a sentence the user had
                    just proved they could write. The sentence is the thing
                    worth seeing; the hints belong on the prompt, where they
                    are still asked for a word at a time. */}
                <p className="lesson-feedback-answer">
                  {verdict.correct ? 'Answer' : 'Correct answer'}: <strong>{verdict.canonical}</strong>
                </p>
              </div>
            )}
          </>
        )}
      </div>

      <div className="lesson-foot">
        {submitError && (
          <p className="lesson-submit-error" role="alert">
            Couldn't finish the lesson: {submitError}
          </p>
        )}
        {current.kind === 'material' ? (
          <Button
            large
            intent="primary"
            text={last ? 'Finish' : 'Got it'}
            onClick={() => void advance()}
            loading={submitting}
          />
        ) : answered ? (
          <Button
            large
            intent={verdict.correct ? 'success' : 'primary'}
            text={last ? 'Finish' : 'Continue'}
            onClick={() => void advance()}
            loading={submitting}
          />
        ) : (
          <Button
            large
            intent="primary"
            text="Check"
            disabled={bank ? chosen.length === 0 : !input.trim()}
            onClick={check}
          />
        )}
      </div>
    </div>
  );
}

/* ---- saving a guest's progress ---- */

/**
 * The offer at the end of every lesson a guest finishes.
 *
 * A guest's work is already on the backend and already theirs: registering
 * claims that record rather than starting a fresh one (see
 * mimi_backend/AGENTS.md), so nothing here is lost by carrying on without an
 * account. What is at stake is only how long it lasts: a guest lives in one
 * cookie, on one browser, for a week. The copy says that rather than
 * pretending the lesson evaporates if they don't sign up now, and "Not now"
 * is a real answer, and the prompt comes back after the next lesson.
 */
function SaveProgress() {
  return (
    <div className="lesson-save">
      <p className="lesson-save-title">
        <Icon icon="floppy-disk" size={14} /> Save your progress
      </p>
      <p className="lesson-save-detail">
        You're learning as a guest, so this only lives in this browser. Create an account
        and everything you've done so far comes with you.
      </p>
      <a className="bp6-button bp6-intent-primary bp6-large" href={`/signup?next=${encodeURIComponent(DEFAULT_NEXT)}`}>
        Create account
      </a>
    </div>
  );
}

/* ---- word bank ---- */

/**
 * A word-bank exercise's answer control, in place of the text input: an
 * assembly line the picked tokens sit on (tap one to send it back) above the
 * bank of tokens still on offer (tap one to pick it). Tokens are tracked by
 * index, not text, so a bank holding the same word twice works: each copy
 * is spent separately. Once the answer is checked (`disabled`) the tokens
 * stay put so the user can compare their assembly with the feedback.
 */
function WordBank({
  bank,
  chosen,
  disabled,
  onChoose,
  onUnchoose,
}: {
  bank: string[];
  /** indices into `bank`, in pick order */
  chosen: number[];
  disabled: boolean;
  onChoose: (index: number) => void;
  /** remove the pick at this position in the assembly line */
  onUnchoose: (position: number) => void;
}) {
  const spent = new Set(chosen);
  return (
    <>
      <div className="lesson-bank-answer" aria-label="Your answer">
        {chosen.length === 0 ? (
          <span className="lesson-bank-placeholder">Tap the words to build your answer…</span>
        ) : (
          chosen.map((token, pos) => (
            <button
              key={`${token}:${pos}`}
              className="lesson-bank-token"
              type="button"
              disabled={disabled}
              onClick={() => onUnchoose(pos)}
            >
              {bank[token]}
            </button>
          ))
        )}
      </div>
      <div className="lesson-bank" aria-label="Word bank">
        {bank.map((text, i) => (
          <button
            key={i}
            // a spent token keeps its slot (the bank doesn't reflow
            // mid-exercise) but leaves only a hole
            className={spent.has(i) ? 'lesson-bank-token is-spent' : 'lesson-bank-token'}
            type="button"
            disabled={disabled || spent.has(i)}
            tabIndex={spent.has(i) ? -1 : undefined}
            onClick={() => onChoose(i)}
          >
            {text}
          </button>
        ))}
      </div>
    </>
  );
}

/* ---- material ---- */

/** A teaching task: prose to read, lines to hear, and what it all teaches. */
function Material({ material }: { material: ApiMaterial; targetLang: string }) {
  return (
    <>
      <p className="eyebrow lesson-kind">New</p>
      <div className="lesson-material">
        {material.text.split(/\n\s*\n/).map((paragraph, i) => <p className="lesson-material-text" key={i}>{markdown(paragraph)}</p>)}
      </div>
    </>
  );
}

/* ---- word hints ---- */

/**
 * The sentence to translate, with the lesson's hints hanging off the words
 * they explain: a word the lesson can gloss carries a dotted underline, and
 * its meanings drop out of it on hover, tap or keyboard focus.
 *
 * The hints used to sit in a row of chips below the prompt, which said what
 * every word meant without saying which word it was about, so the reader had to
 * match them up themselves, which is the work the exercise is asking for.
 * Under the word, a hint is a hint. (Glosses that can't be placed in the
 * sentence still fall back to that row: see glosses.ts.)
 */
function Prompt({
  text,
  glosses,
  newWords,
}: {
  text: string;
  glosses: ApiGloss[];
  newWords: ApiMark[];
}) {
  const hints = useId();
  const { spans, orphans } = useMemo(
    () => {
      // A gloss with nothing to say is not worth underlining a word for. The
      // offsets added here only place the independently served new-word
      // marks inside each rendered run; gloss attachment itself remains
      // exactly the text-matching hint path it was before.
      const attached = attachGlosses(text, glosses.filter((gloss) => gloss.meanings.length > 0));
      let start = 0;
      const placed = attached.spans.map((span) => {
        const at = start;
        start += span.text.length;
        return { ...span, start: at };
      });
      return { spans: placed, orphans: attached.orphans };
    },
    [text, glosses],
  );
  return (
    <>
      <h1 className="lesson-prompt">
        {spans.map((span, i) => {
          if (!span.gloss) {
            return (
              <span key={i}>
                <PromptText text={span.text} start={span.start} newWords={newWords} />
              </span>
            );
          }
          const meanings = span.gloss.meanings;
          const isNew = newWords.some(
            (mark) => mark.start < span.start + span.text.length && mark.end > span.start,
          );
          return (
            <Fragment key={i}>
              <Popover
                // hover rather than hover-target: the meanings stay put while
                // the pointer travels down into them, so they can be read
                interactionKind="hover"
                placement="bottom"
                hoverOpenDelay={40}
                hoverCloseDelay={0}
                transitionDuration={0}
                popupKind="dialog"
                // no transition lifecycle: the hint vanishes as soon as the
                // pointer leaves, while CSS also keeps its entrance instant
                portalClassName="lesson-hint-portal"
                content={
                  <div className="lesson-hint">
                    {meanings.map((meaning, m) => (
                      <span className="lesson-hint-meaning" key={m}>{meaning}</span>
                    ))}
                  </div>
                }
              >
                {/* a button, so the hint opens on tap and on keyboard focus
                    too: a word is no use to a reader who can't reach it */}
                <button
                  className={isNew ? 'lesson-hint-word is-new' : 'lesson-hint-word'}
                  type="button"
                  aria-describedby={`${hints}${i}`}
                >
                  <PromptText text={span.text} start={span.start} newWords={newWords} />
                </button>
              </Popover>
              {/* the same meanings as the word's description: the popover
                  hangs off the end of the document, too far from the word to
                  be read with it, and hidden here so the sentence still reads
                  as a sentence */}
              <span className="sr-only" id={`${hints}${i}`} aria-hidden="true">
                {meanings.join(', ')}
              </span>
            </Fragment>
          );
        })}
      </h1>
      {orphans.length > 0 && <Glosses glosses={orphans} />}
    </>
  );
}

/** Apply only the explicit prompt spans the backend labelled as new. This is
 * intentionally ignorant of glosses: a word can be new without a dictionary
 * hint, and can carry both treatments when a hint does exist. `start` and the
 * marks are JavaScript/UTF-16 offsets, so ordinary `slice` is exact. */
function PromptText({
  text,
  start,
  newWords,
}: {
  text: string;
  start: number;
  newWords: ApiMark[];
}) {
  const end = start + text.length;
  const marks = newWords
    .filter((mark) => mark.start < end && mark.end > start)
    .sort((a, b) => a.start - b.start);
  if (marks.length === 0) return <>{text}</>;

  const pieces = [];
  let cursor = 0;
  for (const mark of marks) {
    const from = Math.max(mark.start, start) - start;
    const to = Math.min(mark.end, end) - start;
    if (from > cursor) {
      pieces.push(<Fragment key={`plain:${cursor}`}>{text.slice(cursor, from)}</Fragment>);
    }
    pieces.push(
      <mark className="lesson-new-word-highlight" key={`new:${mark.word}:${from}`}>
        {text.slice(from, to)}
      </mark>,
    );
    cursor = Math.max(cursor, to);
  }
  if (cursor < text.length) {
    pieces.push(<Fragment key={`plain:${cursor}`}>{text.slice(cursor)}</Fragment>);
  }
  return <>{pieces}</>;
}

/** Hints with no word of their own: the ones the sentence didn't quote back.
    A chip carries the word it explains along with it. */
function Glosses({ glosses }: { glosses: ApiGloss[] }) {
  return <div className="lesson-glosses" aria-label="Word hints">{glosses.map((gloss, i) =>
    <span className="lesson-gloss" key={`${gloss.text}:${i}`}><b>{gloss.text}</b><span>{gloss.meanings.join(', ')}</span></span>
  )}</div>;
}
