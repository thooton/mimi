import Flag from '../Flag';
import { AVAILABLE } from '../../data/languages';
import type { LanguageOption } from '../../data/languages';

/* The first thing a new user sees on /: nothing else on the page makes
   sense before this is answered, so it takes the whole page rather than
   sitting in a modal over a course map that isn't theirs yet.

   It's shown exactly once — after this, the choice lives on the account
   (see targetLang.ts) and the navbar dropdown is where it gets changed. */

interface Props {
  onPick: (code: string) => void;
}

export default function LanguageChooser({ onPick }: Props) {
  function tile(l: LanguageOption) {
    return (
      <button
        key={l.code}
        type="button"
        className="lang-tile"
        onClick={() => onPick(l.code)}
      >
        <Flag region={l.region} size={44} className="flag-tile" />
        <span className="lang-tile-name">{l.name}</span>
        <span className="lang-tile-endonym">{l.endonym}</span>
      </button>
    );
  }

  return (
    <div className="shell chooser">
      <h1 className="chooser-title">What do you want to learn?</h1>
      <p className="chooser-sub">
        You can change this at any time from the flag in the top right.
      </p>

      <div className="lang-grid">{AVAILABLE.map(tile)}</div>
    </div>
  );
}
