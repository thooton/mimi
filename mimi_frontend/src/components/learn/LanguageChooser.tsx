import Flag from '../Flag';
import { languageByCode, languageName } from '../../data/languages';
import type { ApiCourseSummary } from '../../data/api';

/* The first thing a new user sees on /: nothing else on the page makes
   sense before this is answered, so it takes the whole page rather than
   sitting in a modal over a course map that isn't theirs yet.

   It's shown exactly once, after this, the choice lives on the account
   (see courseSelection.ts) and the navbar dropdown is where it gets changed. */

interface Props {
  courses: ApiCourseSummary[];
  onPick: (courseId: string) => void;
}

export default function LanguageChooser({ courses, onPick }: Props) {
  function tile(course: ApiCourseSummary) {
    const language = languageByCode(course.target_lang);
    return (
      <button
        key={course.id}
        type="button"
        className="lang-tile"
        onClick={() => onPick(course.id)}
      >
        {language && <Flag region={language.region} size={44} className="flag-tile" />}
        <span className="lang-tile-name">{languageName(course.target_lang)}</span>
        <span className="lang-tile-endonym">
          {language?.endonym ?? course.target_lang.toUpperCase()} · from {languageName(course.source_lang)}
        </span>
      </button>
    );
  }

  return (
    <div className="shell chooser">
      <h1 className="chooser-title">What do you want to learn?</h1>
      <p className="chooser-sub">
        You can change this at any time from the flag in the top right.
      </p>

      <div className="lang-grid">{courses.map(tile)}</div>
    </div>
  );
}
