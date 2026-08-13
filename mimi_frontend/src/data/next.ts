/* Where to go after signing in or signing up.

   A guest is prompted to save their progress from wherever they happen to be
   — the end of a lesson, the top bar — and should land back there rather than
   at some fixed page, so the auth pages take a `?next=` and honour it.

   Which makes this an open-redirect if it takes the parameter at its word: a
   link to `/signup?next=https://evil.example` would send somebody away from
   mimi wearing the trust of having just logged in. So only a *path on this
   site* is accepted, and everything else falls back to /learn — including
   protocol-relative `//host`, which is a URL wearing a path's clothes. */

/** where the auth pages go when nothing (usable) was asked for */
export const DEFAULT_NEXT = '/learn';

/**
 * The `next` parameter of a query string, if it is somewhere on this site.
 *
 * @param search a `location.search`, with or without its leading "?"
 */
export function safeNext(search: string): string {
  const next = new URLSearchParams(search).get('next');
  if (!next || !next.startsWith('/') || next.startsWith('//')) return DEFAULT_NEXT;
  return next;
}

/** a link to `path`, carrying the current `next` on with it, so switching
    between the sign-in and sign-up pages doesn't lose where we were headed */
export function withNext(path: string, search: string): string {
  const next = safeNext(search);
  return next === DEFAULT_NEXT ? path : `${path}?next=${encodeURIComponent(next)}`;
}
