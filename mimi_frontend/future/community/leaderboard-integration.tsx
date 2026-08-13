import ClubBoard from './components/ClubBoard';

/* The rail removed when the active leaderboard became individual-only. */
export function ClubLeaderboardRail() {
  return (
    <aside className="board-rail">
      <ClubBoard />
    </aside>
  );
}

/* Club affiliations were previously rendered before each individual name. */
export interface ClubAffiliation {
  club?: string;
}

export function ClubTag({ club }: ClubAffiliation) {
  return club ? <span className="tag-chip">{club}</span> : null;
}

