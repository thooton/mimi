import assert from 'node:assert/strict';
import test from 'node:test';

import type { ApiProfile } from './api.ts';
import { activityFrom, profileFrom, safeAvatar, usernameFromPath } from './profile.ts';

/* Public profiles are one page serving every name, so this parser is the
   whole of "which profile am I looking at", the host rewrote /u/<name> onto
   /u/ and threw the name away everywhere except the address bar. */

test('a profile path yields the name in it', () => {
  assert.equal(usernameFromPath('/u/marisol'), 'marisol');
  // a trailing slash is the same address
  assert.equal(usernameFromPath('/u/marisol/'), 'marisol');
});

test('the names mimi_auth actually permits survive the trip', () => {
  // Registration preserves case and permits letters, digits and underscores.
  assert.equal(usernameFromPath('/u/Ab_cD9'), 'Ab_cD9');
});

test('an escaped name is decoded once', () => {
  assert.equal(usernameFromPath('/u/two%20words'), 'two words');
  // a guest's ~ could only be typed by hand, but it must not be mangled
  assert.equal(usernameFromPath('/u/guest%7Eabc'), 'guest~abc');
});

// A rewrite rule that is too eager, or somebody typing the bare prefix, must
// not be read as a request for a profile called "", that would send the
// profile endpoint a nonsense username.
test('a path naming nobody yields null', () => {
  assert.equal(usernameFromPath('/u/'), null);
  assert.equal(usernameFromPath('/u'), null);
  assert.equal(usernameFromPath('/profile'), null);
  assert.equal(usernameFromPath('/'), null);
});

// There is nothing underneath a profile to ask for, so a deeper path is not
// a profile, better to say "no profile here" than to show the wrong one.
test('a deeper path is not a profile', () => {
  assert.equal(usernameFromPath('/u/marisol/settings'), null);
});

// A half-written percent escape throws inside decodeURIComponent; a broken
// URL should read as "no name", not take the page down.
test('a malformed escape is not a username', () => {
  assert.equal(usernameFromPath('/u/%E0%A4%A'), null);
});

/* --- the linked picture ---

   The backend checks this before storing it, so this is the second of two
   locks. It is worth having: the string is one person's URL, rendered into
   every other person's browser as something to fetch. */

test('an avatar is an https URL or it is not shown', () => {
  assert.equal(safeAvatar('https://cdn.example.com/me.png'), 'https://cdn.example.com/me.png');
  // no picture is the ordinary case, not an error
  assert.equal(safeAvatar(null), undefined);
  assert.equal(safeAvatar(''), undefined);
});

test('the schemes that would make a picture an injection are refused', () => {
  assert.equal(safeAvatar('javascript:alert(1)'), undefined);
  assert.equal(safeAvatar('data:image/svg+xml;base64,AAAA'), undefined);
  assert.equal(safeAvatar('//cdn.example.com/me.png'), undefined);
  // and plain http, which a page served over TLS could not load anyway
  assert.equal(safeAvatar('http://cdn.example.com/me.png'), undefined);
});

test('an avatar carrying quotes, brackets or whitespace is refused', () => {
  assert.equal(safeAvatar('https://cdn.example.com/"onerror=x'), undefined);
  assert.equal(safeAvatar('https://cdn.example.com/<script>'), undefined);
  assert.equal(safeAvatar('https://cdn.example.com/a b.png'), undefined);
  assert.equal(safeAvatar('https://cdn.example.com/a\nb.png'), undefined);
});

/* --- the feed --- */

/** a profile carrying nothing but the days under test */
function profileWith(days: ApiProfile['days']): ApiProfile {
  return {
    username: 'sam',
    display: 'Sam',
    title: null,
    bio: '',
    cefr: '',
    avatar: null,
    course_id: 'spanish_for_english',
    joined: 0,
    online: false,
    last_active: null,
    today: 0,
    streak: 0,
    followers: 0,
    following: 0,
    viewer_follows: false,
    xp_schedule: { lesson: 20, perfect_lesson: 40 },
    totals: { xp: 0, lessons: 0, exercises: 0, correct: 0, words: 0, skills: 0, days: 0 },
    languages: [{
      id: 'spanish_for_english', code: 'es', source_code: 'en', score: 400, delta: 0, provisional: true,
      words: 0, skills: 0, lessons: 0, since: 0, points: [],
    }],
    days,
  };
}

/** one day of the wire format, quiet by default */
function day(over: Partial<ApiProfile['days'][number]>): ApiProfile['days'][number] {
  return {
    t: 86_400, streak: 0, lessons: 0, exercises: 0, correct: 0, xp: 0,
    learned: [], skills: [], followed: [], score: 400, delta: 0, ...over,
  };
}

test('live presence is independent of study activity', () => {
  const api = profileWith([]);
  api.online = true;
  api.last_active = null;
  const profile = profileFrom(api);
  assert.equal(profile.online, true);
  assert.equal(profile.lastActive, 'Never');
});

test('a follow is an entry in the feed, linking to whoever was followed', () => {
  const [entry] = activityFrom(profileWith([
    day({ followed: [{ username: 'ren', display: 'Ren' }] }),
  ]))[0].entries;
  assert.equal(entry.text, 'Followed Ren');
  assert.equal(entry.href, '/u/ren');
  // a follow earns nothing and moves no score, so it quotes neither
  assert.equal(entry.xp, undefined);
  assert.equal(entry.score, undefined);
});

// A day can be in the feed without being a day of study, following somebody
// is dated but is not a lesson, and "Completed 0 lessons" reports something
// that didn't happen.
test('a day with a follow and no lesson has no lesson entry', () => {
  const [followOnly] = activityFrom(profileWith([
    day({ followed: [{ username: 'ren', display: 'Ren' }] }),
  ]));
  assert.equal(followOnly.entries.length, 1);

  const [studied] = activityFrom(profileWith([
    day({ lessons: 2, exercises: 20, correct: 18, xp: 40, followed: [{ username: 'ren', display: 'Ren' }] }),
  ]));
  assert.equal(studied.entries.length, 2);
  assert.equal(studied.entries[0].text, 'Completed 2 lessons · 18 of 20 right');
});
