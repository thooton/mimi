import { useEffect, useMemo, useState } from 'react';
import { Button, Icon } from '@blueprintjs/core';
import { gradeWordBank } from '../data/grading';
import type { Verdict } from '../data/grading';
import { speak, speechAvailable } from '../data/speech';
import { playVerdict } from '../data/sounds';
import { shuffledBank } from '../data/wordBank';

/* Two exercises from the front of the Spanish course, playable on the landing
   page. The point is that this is not a mock-up: the tiles are scrambled by
   the same shuffledBank the lesson player uses, the answer goes through the
   same gradeWordBank — accents and punctuation forgiven exactly as they are in
   a real lesson — and the speaker button is the same speech synthesiser. A
   drawing of an exercise would have been less code and would have proved
   nothing.

   What it deliberately doesn't do is talk to the backend. Nothing here is
   recorded, so there is no account to make and nothing to lose, which is also
   the honest reason it stops after two: past that you are doing the course,
   and the course lives at /. */

interface Task {
  /** the English to translate */
  prompt: string;
  /** Accepted answers, canonical first. Plain text: a real exercise carries
      the spans that let each word be graded on its own, and this demo grades
      nothing per word — there is no account here for a verdict to land in. */
  answers: string[];
  /**
   * The answer's tokens plus a few distractors, written down in an order that
   * already doesn't spell the answer. A real bank arrives from the backend
   * answer-first and is scrambled on arrival, but this one is also what the
   * server renders into the HTML — so its authored order has to be safe on its
   * own, for the moment before the shuffle below can run.
   */
  bank: string[];
}

const TASKS: Task[] = [
  {
    prompt: 'The cat drinks the milk.',
    answers: ['El gato bebe la leche.'],
    bank: ['la', 'gatos', 'bebe', 'El', 'al', 'leche', 'beben', 'gato'],
  },
  {
    prompt: 'Good morning! How are you?',
    answers: ['¡Buenos días! ¿Cómo estás?'],
    bank: ['Cómo', 'Buenas', 'estás', 'días', 'eres', 'Buenos', 'noches'],
  },
];

export default function TryIt() {
  const [index, setIndex] = useState(0);
  /** bank tokens picked so far, as indices into `bank`, in pick order */
  const [chosen, setChosen] = useState<number[]>([]);
  const [verdict, setVerdict] = useState<Verdict | null>(null);
  /* Whether we're past hydration. Math.random() during the first render would
     deal a different hand than the server put in the HTML, and React answers a
     mismatch by throwing the island away and rebuilding it — so the authored
     order stands until the client is definitely the one rendering. */
  const [mounted, setMounted] = useState(false);
  useEffect(() => setMounted(true), []);

  const task = TASKS[index];
  /* Scrambled once per task and then held still: `chosen` holds indices into
     whichever order we settle on, so a reshuffle mid-answer would rewrite what
     the reader had already picked. */
  const bank = useMemo(
    () => (task ? (mounted ? shuffledBank(task.bank) : task.bank) : []),
    [index, mounted],
  );

  if (!task) {
    return (
      <div className="tryit tryit--done">
        <Icon icon="tick-circle" size={26} />
        <p className="tryit-done-text">
          That is roughly a lesson. A real one runs about fifteen of these, with
          reading and listening in between.
        </p>
        <a className="land-btn" href="/learn">
          Start the course
        </a>
        <button className="tryit-again" type="button" onClick={() => setIndex(0)}>
          Or go again
        </button>
      </div>
    );
  }

  const answered = verdict !== null;

  return (
    <div className="tryit">
      <div className="tryit-head">
        <span className="eyebrow tryit-kind">Translate to Spanish</span>
        <span className="tryit-count tnum">
          {index + 1} / {TASKS.length}
        </span>
      </div>

      <p className="tryit-prompt">{task.prompt}</p>

      <div className="lesson-bank-answer" aria-label="Your answer">
        {chosen.length === 0 ? (
          <span className="lesson-bank-placeholder">Tap the words to build your answer…</span>
        ) : (
          chosen.map((token, pos) => (
            <button
              key={`${token}:${pos}`}
              className="lesson-bank-token"
              type="button"
              disabled={answered}
              onClick={() => setChosen((picks) => picks.filter((_, p) => p !== pos))}
            >
              {bank[token]}
            </button>
          ))
        )}
      </div>

      <div className="lesson-bank" aria-label="Word bank">
        {bank.map((text, i) => {
          const spent = chosen.includes(i);
          return (
            <button
              key={i}
              className={spent ? 'lesson-bank-token is-spent' : 'lesson-bank-token'}
              type="button"
              disabled={answered || spent}
              tabIndex={spent ? -1 : undefined}
              /* updater form, so two taps landing in one batch both count —
                 React would otherwise hand the second one a stale `chosen` */
              onClick={() => setChosen((picks) => [...picks, i])}
            >
              {text}
            </button>
          );
        })}
      </div>

      {verdict && (
        <div
          className={verdict.correct ? 'lesson-feedback is-correct' : 'lesson-feedback is-wrong'}
        >
          <div className="lesson-feedback-head">
            <Icon icon={verdict.correct ? 'tick-circle' : 'cross-circle'} size={20} />
            <strong>{verdict.correct ? 'Correct!' : 'Not quite'}</strong>
          </div>
          <p className="lesson-feedback-answer">
            <span>
              {verdict.correct ? 'Answer' : 'Correct answer'}: <strong>{verdict.canonical}</strong>
            </span>
            {speechAvailable() && (
              <button
                className="tryit-say"
                type="button"
                onClick={() => speak(verdict.canonical, 'es')}
              >
                <Icon icon="volume-up" size={14} />
                Hear it
              </button>
            )}
          </p>
        </div>
      )}

      <div className="tryit-foot">
        {answered ? (
          <Button
            intent={verdict.correct ? 'success' : 'primary'}
            text={index + 1 === TASKS.length ? 'Finish' : 'Continue'}
            onClick={() => {
              setIndex(index + 1);
              setChosen([]);
              setVerdict(null);
            }}
          />
        ) : (
          <Button
            intent="primary"
            text="Check"
            disabled={chosen.length === 0}
            onClick={() => {
              const v = gradeWordBank(
                chosen.map((i) => bank[i]),
                task.answers.map((text) => ({ text, words: [] })),
              );
              setVerdict(v);
              playVerdict(v.correct);
            }}
          />
        )}
      </div>
    </div>
  );
}
