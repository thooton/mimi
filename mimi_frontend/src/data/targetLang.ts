import { useEffect, useState } from 'react';
import { ensureUser, fetchProfile, fetchViewer, setTargetLang } from './api';

/* What the user is learning right now — one global choice, shared by every
   page.

   The choice is the account's, not the browser's: it lives on the backend as
   `target_lang` of the user's profile (the one writable field there — see
   mimi_backend/src/store.rs), so it follows the user across browsers and
   devices rather than being stuck in one localStorage.

   The store still lives outside React on purpose. The navbar and the learn
   page are separate `client:load` roots (see AppLayout.astro), so there is
   no common ancestor to hang a context off: the two are siblings in the DOM
   and strangers to each other. A module-level cache plus an event is the
   whole store, and both roots subscribe to it. The cache is fetched once per
   page load no matter how many roots ask, because the fetch starts on first
   use and the promise is shared.

   Cross-tab sync is the one thing given up versus localStorage: two open
   tabs no longer mirror a pick live (the native `storage` event did that
   before). A reload puts them right, and the backend keeps them from
   disagreeing permanently — a trade worth making for the choice surviving
   the browser. */

/** same-tab, cross-root. */
const EVENT = 'mimi:target-lang';

interface LangState {
  /** the chosen language code, or null if nothing has been picked yet */
  lang: string | null;
  /**
   * Whether the backend has answered.
   *
   * These pages are prerendered, so the server has no idea what the user
   * picked and renders as though nothing was. The first client render has
   * to agree with that HTML or hydration breaks — which means "no language"
   * and "haven't asked yet" are different states, and callers that would
   * show a first-run screen need to tell them apart.
   */
  ready: boolean;
}

/* the one copy — replaced wholesale on every update, never mutated, so a
   React state snapshot of it is always consistent */
let current: LangState = { lang: null, ready: false };
let inflight: Promise<void> | null = null;

function announce() {
  window.dispatchEvent(new CustomEvent(EVENT));
}

function load(): void {
  if (inflight) return;
  inflight = (async () => {
    const viewer = await fetchViewer();
    const profile = await fetchProfile(viewer.username);
    current = { lang: profile.target_lang, ready: true };
  })()
    .catch(() => {
      /* a backend that won't answer is no reason to hold every page blank:
         render as unpicked and let each page's own error handling report
         the backend, which it already does for everything else */
      current = { lang: null, ready: true };
    })
    .finally(announce);
}

function pick(code: string): void {
  // optimistic: the page moves on at once and the backend records the choice
  // behind it. If the write fails, the choice won't outlive the session but
  // should still take effect on it — the same trade the old localStorage
  // store made when it threw.
  current = { lang: code, ready: true };
  announce();
  ensureUser()
    .then(() => setTargetLang(code))
    .catch(() => {});
}

export interface TargetLang extends LangState {
  setLang: (code: string) => void;
}

export function useTargetLang(): TargetLang {
  const [state, setState] = useState<LangState>(current);

  useEffect(() => {
    const sync = () => setState(current);
    sync(); // a root mounted after the value landed has missed the event
    load();
    window.addEventListener(EVENT, sync);
    return () => window.removeEventListener(EVENT, sync);
  }, []);

  return { ...state, setLang: pick };
}
