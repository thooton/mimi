import assert from 'node:assert/strict';
import test from 'node:test';

import { usernameContainsReservedTerm } from './username.ts';

test('role and permission terms are reserved as case-insensitive substrings', () => {
  for (const username of [
    'administrator',
    'the_bureaucrat',
    'Steward123',
    'CHECKUSER_1',
    'oversight_team',
    'SuperAdmin',
    'wiki_sysop',
    'MyModeratorName',
  ]) {
    assert.equal(usernameContainsReservedTerm(username), true, username);
  }
});

test('ordinary usernames are not reserved', () => {
  for (const username of ['AbCdE_9', 'sam', 'language_learner']) {
    assert.equal(usernameContainsReservedTerm(username), false, username);
  }
});
