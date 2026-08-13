import { useEffect, useState } from 'react';
import { Button, Dialog, Spinner } from '@blueprintjs/core';
import type { ApiMaterial, ApiPosition } from '../../data/api';
import { fetchTips } from '../../data/api';
import { markdown } from './markdown';

/* The skill card's "Tips" button: every material block the lesson carries,
   fetched on open and read in a dialog — starting the lesson not required.
   `position` doubles as the open state: null is closed. */
export default function TipsDialog({ skillName, position, onClose }: {
  /** the skill's display name, for the title */
  skillName: string;
  /** the lesson to show tips for (the one the card's Start button would begin) */
  position: ApiPosition | null;
  onClose: () => void;
}) {
  const [tips, setTips] = useState<ApiMaterial[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  /* refetch on every open: the tips are cheap, and a stale copy from the
     last card opened would flash under the spinner either way */
  useEffect(() => {
    if (!position) return;
    let cancelled = false;
    setTips(null);
    setError(null);
    fetchTips(position).then(
      (result) => { if (!cancelled) setTips(result.tips); },
      (e) => { if (!cancelled) setError(e instanceof Error ? e.message : String(e)); },
    );
    return () => { cancelled = true; };
  }, [position]);

  return (
    <Dialog
      isOpen={position !== null}
      onClose={onClose}
      icon="lightbulb"
      title={`${skillName} tips`}
      className="tips-dialog"
    >
      <div className="tips-body">
        {error ? (
          <p className="tips-text tips-empty">{error}</p>
        ) : tips === null ? (
          <Spinner className="tips-spinner" />
        ) : tips.length === 0 ? (
          <p className="tips-text tips-empty">This lesson has no tips.</p>
        ) : (
          tips.map((tip, i) =>
            tip.text.split(/\n\s*\n/).map((paragraph, j) => (
              <p className="tips-text" key={`${i}:${j}`}>{markdown(paragraph)}</p>
            )),
          )
        )}
      </div>
      <div className="tips-foot">
        <Button fill intent="primary" text="Got it" onClick={onClose} />
      </div>
    </Dialog>
  );
}
