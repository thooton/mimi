import { Icon } from '@blueprintjs/core';
import type { ActivityDay } from '../../data/profile';
import { formatDay } from '../../data/profile';
import Delta from './Delta';

/* What the person actually did, newest day first. Each day leads with a
   flame when it is part of a real streak (a run longer than three days), so
   scrolling the feed shows the streak being kept alive rather than just
   quoting a number for it. The lessons are reported plainly — how many, and
   the words they left behind — with no win/loss scoring, because a lesson
   isn't a match. */

/** the smallest run worth calling a streak */
const STREAK_MIN = 3;

export default function ActivityFeed({ days }: { days: ActivityDay[] }) {
  return (
    <div className="activity">
      {days.map((day) => (
        <section className="act-day" key={day.t}>
          <div className="act-date-row">
            <h3 className="act-date">{formatDay(day.t)}</h3>
            {day.streak > STREAK_MIN && (
              <span className="act-streak" title={`${day.streak}-day streak`}>
                <Icon icon="flame" size={12} />
                {day.streak}-day streak
              </span>
            )}
          </div>

          <div className="act-rows">
            {day.entries.map((entry, i) => (
              <div className="act-row" key={i}>
                <span className="act-icon" aria-hidden="true">
                  <Icon icon={entry.icon} size={15} />
                </span>

                <div className="act-main">
                  <p className="act-text">
                    {/* an entry about somebody else leads to them */}
                    {entry.href ? (
                      <a className="act-link" href={entry.href}>{entry.text}</a>
                    ) : (
                      entry.text
                    )}
                    {entry.score !== undefined && (
                      <>
                        <span className="act-score">{entry.score}</span>
                        <Delta value={entry.delta ?? 0} />
                      </>
                    )}
                    {entry.xp !== undefined && <span className="act-xp">+{entry.xp} XP</span>}
                  </p>

                  {entry.learned && (
                    <ul className="act-learned">
                      {entry.learned.map((word) => (
                        <li className="act-word" key={word}>
                          {word}
                        </li>
                      ))}
                    </ul>
                  )}
                </div>
              </div>
            ))}
          </div>
        </section>
      ))}
    </div>
  );
}
