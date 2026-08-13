import { Icon } from '@blueprintjs/core';
import type { Practice } from '../../data/profile';
import { formatXp } from '../../data/social';

/* Practice that isn't tied to one course — words drilled, listening and
   speaking reps, review sprints. These carry no score on purpose: the count
   is the honest measure, so it is what gets shown, big and plain. */

export default function PracticeStrip({ practice }: { practice: Practice[] }) {
  return (
    <section className="panel practice">
      <div className="panel-head">
        <h2 className="eyebrow panel-title">Practice</h2>
      </div>
      <div className="practice-body">
        {practice.map((item) => (
          <div className="practice-tile" key={item.id}>
            <span className="practice-icon" aria-hidden="true">
              <Icon icon={item.icon} size={16} />
            </span>
            <span className="practice-count">{formatXp(item.count)}</span>
            <span className="practice-noun">
              {item.noun} · {item.name}
            </span>
          </div>
        ))}
      </div>
    </section>
  );
}
