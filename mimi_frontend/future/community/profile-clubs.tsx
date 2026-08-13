import { Icon } from '@blueprintjs/core';

/* Panel removed from ProfileApp while clubs are deferred. */
export function ProfileClubs({ clubs }: { clubs: string[] }) {
  return (
    <section className="panel profile-clubs">
      <div className="panel-head">
        <h2 className="eyebrow panel-title">Clubs</h2>
      </div>
      <div className="profile-clubs-body">
        {clubs.map((club) => (
          <a className="club" href="/community/clubs" key={club}>
            <Icon icon="people" size={15} />
            {club}
          </a>
        ))}
      </div>
    </section>
  );
}

export const PROFILE_CLUBS = ['Valencia Table', 'Tokyo Coffee Club'];

