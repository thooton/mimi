import type { Language } from '../../data/profile';
import { formatXp } from '../../data/social';
import Delta from './Delta';

/* One tile per language the person is learning, the score first, the way a
   Duolingo course leads with its number. The row sits directly above the
   graph and is its legend: clicking a tile drops that language's line out,
   so the trend lives in the chart and the tiles stay a compact summary of
   where each one stands right now. */

interface Props {
  languages: Language[];
  hidden: string[];
  onToggle: (id: string) => void;
}

export default function LanguageCards({ languages, hidden, onToggle }: Props) {
  return (
    <div className="lang-cards">
      {languages.map((lang) => {
        const off = hidden.includes(lang.id);
        return (
          <button
            key={lang.id}
            type="button"
            className={off ? 'lang-card is-off' : 'lang-card'}
            style={{ '--lang': lang.color } as React.CSSProperties}
            aria-pressed={!off}
            title={off ? `Show ${lang.name} on the graph` : `Hide ${lang.name} from the graph`}
            onClick={() => onToggle(lang.id)}
          >
            <span className="lang-glyph" aria-hidden="true">
              {lang.glyph}
            </span>

            <span className="lang-body">
              <span className="lang-name">{lang.name}</span>
              {/* what the score is made of, which is the honest thing to put
                  under it, there is no leaderboard to quote a rank from */}
              <span className="lang-foot">
                {formatXp(lang.counts.lessons)} lessons · {formatXp(lang.counts.words)} words
              </span>
            </span>

            <span className="lang-figure">
              <span className="lang-score">{formatXp(lang.score)}</span>
              <Delta value={lang.delta} />
            </span>
          </button>
        );
      })}
    </div>
  );
}
