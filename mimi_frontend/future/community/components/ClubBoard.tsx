import { useState } from 'react';
import { SegmentedControl } from '@blueprintjs/core';
import { clubBoard, type ClubSort } from '../data/community';
import { languageByCode } from '../data/languages';
import { formatXp } from '../data/social';
import Flag from './Flag';

/* The club leaderboard panel, shared by the leaderboard tab and the clubs
   page, so the two can never drift apart: same head, same sort toggle, same
   table. Ranked by the average XP each member put in this week, or by sheer
   headcount — the toggle picks. */
export default function ClubBoard() {
  const [sort, setSort] = useState<ClubSort>('avgXp');
  const clubs = clubBoard(sort);

  return (
    <section className="panel">
      <div className="panel-head">
        <h2 className="eyebrow panel-title">Club leaderboard</h2>
        <SegmentedControl
          size="small"
          value={sort}
          onValueChange={(value) => setSort(value as ClubSort)}
          options={[
            { value: 'avgXp', label: 'Avg XP' },
            { value: 'members', label: 'Members' },
          ]}
        />
      </div>
      <div className="board-body">
        <table className="board-table club-table">
          <thead>
            <tr>
              <th className="col-rank">#</th>
              <th>Club</th>
              <th className={sort === 'members' ? 'col-num is-sorted' : 'col-num'}>Members</th>
              <th className={sort === 'avgXp' ? 'col-num is-sorted' : 'col-num'}>Avg XP</th>
            </tr>
          </thead>
          <tbody>
            {clubs.map((club, i) => {
              const rank = i + 1;
              const focus = club.focus ? languageByCode(club.focus) : undefined;
              return (
                <tr key={club.tag}>
                  <td className="col-rank">
                    <span className={rank <= 3 ? `rank rank--${rank}` : 'rank'}>{rank}</span>
                  </td>
                  <td>
                    <span className="player">
                      <span className="tag-chip">{club.tag}</span>
                      <span className="player-name">{club.name}</span>
                      <span className="club-focus">
                        {focus ? (
                          <>
                            <Flag region={focus.region} size={13} />
                            {focus.name}
                          </>
                        ) : (
                          'All languages'
                        )}
                      </span>
                    </span>
                  </td>
                  <td className="col-num">{formatXp(club.members)}</td>
                  <td className="col-num">{formatXp(club.avgXp)}</td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
    </section>
  );
}
