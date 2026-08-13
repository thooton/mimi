import { useEffect, useState } from 'react';
import { Button, HTMLSelect, Icon, SegmentedControl } from '@blueprintjs/core';
import type { ChatPartner, MatchKind } from '../../data/community';
import { findMatches } from '../../data/community';
import { LANGUAGES, languageByCode, languageName } from '../../data/languages';
import { useTargetLang } from '../../data/targetLang';
import Flag from '../Flag';

/* Chat match: pair up with someone to actually talk to.

   Two dropdowns — what you speak, what you're learning — and a choice of
   who to meet. A native speaker makes the obvious exchange (their B2 for
   your C1); a fellow learner is company on the same road. The roster behind
   it is example data until the backend grows one (see data/community.ts). */

const LANGUAGE_OPTIONS = LANGUAGES.map((l) => ({ value: l.code, label: l.name }));

/** "speaks Spanish → learning English", with the flags beside the names */
function LangPair({ from, to }: { from: string; to: string }) {
  const a = languageByCode(from);
  const b = languageByCode(to);
  return (
    <span className="match-langs">
      {a && <Flag region={a.region} size={13} />}
      speaks {languageName(from)}
      <Icon icon="arrow-right" size={10} className="match-arrow" />
      {b && <Flag region={b.region} size={13} />}
      learning {languageName(to)}
    </span>
  );
}

function MatchCard({ partner }: { partner: ChatPartner }) {
  return (
    <li className="match-card">
      <span className="avatar match-avatar" aria-hidden="true">
        {partner.name.charAt(0)}
      </span>
      <span className="match-id">
        <span className="match-name">
          {partner.club && <span className="tag-chip">{partner.club}</span>}
          {partner.name}
        </span>
        <LangPair from={partner.native} to={partner.learning} />
      </span>
      <span className="match-side">
        <span className="level-chip">{partner.level}</span>
        <span
          className={partner.online ? 'presence is-online' : 'presence'}
          title={partner.online ? 'Online now' : 'Away'}
        />
        <Button small icon="comment" text="Say hello" />
      </span>
    </li>
  );
}

export default function ChatMatch() {
  const { lang, ready } = useTargetLang();
  const [speak, setSpeak] = useState('en');
  const [learn, setLearn] = useState('es');
  const [kind, setKind] = useState<MatchKind>('native');
  /* null = haven't searched yet, [] = searched and nobody fit */
  const [results, setResults] = useState<ChatPartner[] | null>(null);
  const [sameLang, setSameLang] = useState(false);

  /* prefill "I'm learning" with the site-wide language once the backend has
     answered — never before, or the prerendered HTML and the first client
     render would disagree */
  useEffect(() => {
    if (ready && lang) setLearn(lang);
  }, [ready, lang]);

  function search() {
    if (speak === learn) {
      setSameLang(true);
      setResults(null);
      return;
    }
    setSameLang(false);
    setResults(findMatches(speak, learn, kind));
  }

  /* a search answers the controls as they were; changing one after the fact
     clears the answer rather than leaving it lying about the question */
  function stale() {
    setResults(null);
    setSameLang(false);
  }

  return (
    <section className="panel match-panel">
      <div className="panel-head">
        <h2 className="eyebrow panel-title">Chat match</h2>
        <span className="eyebrow">142 online now</span>
      </div>
      <div className="match-body">
        <div className="match-controls">
          <label className="match-field">
            <span className="eyebrow match-label">I speak</span>
            <HTMLSelect
              fill
              value={speak}
              options={LANGUAGE_OPTIONS}
              onChange={(e) => {
                setSpeak(e.currentTarget.value);
                stale();
              }}
            />
          </label>

          <button
            type="button"
            className="match-swap"
            aria-label="Swap languages"
            title="Swap languages"
            onClick={() => {
              setSpeak(learn);
              setLearn(speak);
              stale();
            }}
          >
            <Icon icon="exchange" size={13} />
          </button>

          <label className="match-field">
            <span className="eyebrow match-label">I&apos;m learning</span>
            <HTMLSelect
              fill
              value={learn}
              options={LANGUAGE_OPTIONS}
              onChange={(e) => {
                setLearn(e.currentTarget.value);
                stale();
              }}
            />
          </label>

          <div className="match-kind">
            <span className="eyebrow match-label">Match me with</span>
            <SegmentedControl
              fill
              value={kind}
              onValueChange={(value) => {
                setKind(value as MatchKind);
                stale();
              }}
              options={[
                { value: 'native', label: 'A native speaker' },
                { value: 'learner', label: 'A fellow learner' },
              ]}
            />
          </div>

          <Button
            className="match-go"
            intent="primary"
            icon="chat"
            text="Find a partner"
            onClick={search}
          />
        </div>

        {sameLang && (
          <p className="match-empty">
            Those are the same language — pick two different ones and your
            exchange has something to trade.
          </p>
        )}

        {results !== null && results.length === 0 && (
          <p className="match-empty">
            Nobody fits that combination right now. Try another pair, or
            browse the <a href="/community/forums">language exchange board</a>.
          </p>
        )}

        {results !== null && results.length > 0 && (
          <ul className="match-results">
            {results.map((p) => (
              <MatchCard key={p.name} partner={p} />
            ))}
          </ul>
        )}
      </div>
    </section>
  );
}
