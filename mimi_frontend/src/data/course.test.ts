import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import type { ApiCourse, ApiSkill } from './api.ts';
import { allSkills, levelProgress, skillAction, treeFromCourse } from './course.ts';

const skill = (id: string, level = 0, lessons_done = 0): ApiSkill => ({
  id, name: id, focus: `${id} focus`, state: 'available', level, lessons: 6, lessons_done,
});

test('course castles and sibling rows keep their branching structure', () => {
  const course: ApiCourse = {
    id: 'spanish', source_lang: 'en', target_lang: 'es',
    castles: [{ castle: 0, state: 'available', rows: [{ skills: [skill('a'), skill('b')] }, { skills: [skill('c')] }] }],
  };
  const tree = treeFromCourse(course);
  assert.equal(tree.length, 1);
  assert.deepEqual(tree[0].rows.map((row) => row.skills.map((item) => item.id)), [['a', 'b'], ['c']]);
  assert.deepEqual(allSkills(tree).map((item) => item.row), [0, 0, 1]);
});

test('level progress and actions follow the backend level window', () => {
  assert.equal(levelProgress(skill('a', 2, 3)), 0.5);
  assert.equal(skillAction(skill('a'), 20), 'Start +20 XP');
  assert.equal(skillAction(skill('a', 1, 2), 20), 'Start +20 XP');
  assert.equal(skillAction({ ...skill('a', 5, 6), state: 'completed' }, 20), 'Practice');
});
