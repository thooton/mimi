import type { ApiCourse, ApiSkill, ApiSkillState } from './api';
import { languageByCode } from './languages.ts';

export interface SkillNode extends ApiSkill {
  castle: number;
  row: number;
}

export interface SkillRow {
  id: string;
  castle: number;
  row: number;
  skills: SkillNode[];
}

export interface CastleGroup {
  castle: number;
  state: 'passed' | 'available' | 'locked';
  rows: SkillRow[];
}

export function courseName(course: ApiCourse): string {
  return languageByCode(course.target_lang)?.name ??
    course.id.charAt(0).toUpperCase() + course.id.slice(1);
}

export function treeFromCourse(course: ApiCourse): CastleGroup[] {
  let row = 0;
  return course.castles.map((castle) => ({
    castle: castle.castle,
    state: castle.state,
    rows: castle.rows.map((source) => {
      const result: SkillRow = {
        id: `c${castle.castle}r${row}`,
        castle: castle.castle,
        row,
        skills: source.skills.map((skill) => ({ ...skill, castle: castle.castle, row })),
      };
      row += 1;
      return result;
    }),
  }));
}

export function allSkills(tree: CastleGroup[]): SkillNode[] {
  return tree.flatMap((castle) => castle.rows.flatMap((row) => row.skills));
}

export function levelProgress(skill: ApiSkill): number {
  return skill.lessons === 0 ? 0 : Math.min(1, skill.lessons_done / skill.lessons);
}

export function skillAction(skill: ApiSkill, lessonXp: number): string {
  if (skill.state === 'completed') return 'Practice';
  return `Start +${lessonXp} XP`;
}

export function stateLabel(state: ApiSkillState): string {
  if (state === 'locked') return 'Locked';
  if (state === 'completed') return 'Completed';
  return 'Available';
}
