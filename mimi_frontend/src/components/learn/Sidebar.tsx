import { Icon } from '@blueprintjs/core';
import type { ApiQuest } from '../../data/api';
import type { CastleGroup } from '../../data/course';
import { allSkills } from '../../data/course';

function Metric({ label, value, total }: { label: string; value: number; total?: number }) {
  return <div className="metric"><span className="metric-label">{label}</span><span className="metric-value">{value}{total !== undefined && <span className="metric-total"> / {total}</span>}</span></div>;
}

function Quest({ quest }: { quest: ApiQuest }) {
  const complete = quest.done >= quest.total;
  return <div className={complete ? 'quest is-done' : 'quest'}>
    <span className="quest-mark"><Icon icon={complete ? 'tick' : 'dot'} size={10} /></span>
    <div className="quest-body"><div className="quest-title">{quest.title}</div><div className="meter" role="progressbar" aria-label={quest.title} aria-valuemin={0} aria-valuemax={quest.total} aria-valuenow={Math.min(quest.done, quest.total)}><div className={complete ? 'meter-fill is-done' : 'meter-fill'} style={{ width: `${Math.min(quest.done / quest.total, 1) * 100}%` }} /></div></div>
    <span className="quest-count">{quest.done}/{quest.total}</span>
  </div>;
}

export default function Sidebar({ tree, quests }: { tree: CastleGroup[]; quests: ApiQuest[] }) {
  const skills = allSkills(tree);
  const levels = skills.reduce((sum, skill) => sum + skill.level, 0);
  const opened = skills.filter((skill) => skill.level >= 2).length;
  const castles = tree.filter((castle) => castle.state === 'passed').length;
  return <>
    <section className="panel"><div className="panel-head"><h2 className="eyebrow panel-title">Tree progress</h2></div><div className="panel-body">
      <Metric label="Total skill levels" value={levels} />
      <Metric label="Skills at level 2" value={opened} total={skills.length} />
      <Metric label="Castles passed" value={castles} total={tree.length} />
    </div></section>
    <section className="panel"><div className="panel-head"><h2 className="eyebrow panel-title">Daily quests</h2></div><div className="panel-body">
      {quests.map((quest) => <Quest quest={quest} key={quest.id} />)}
    </div></section>
  </>;
}
