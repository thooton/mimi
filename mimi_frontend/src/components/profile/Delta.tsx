import { Icon } from '@blueprintjs/core';

/* A movement in points, with the arrow that says which way. The language
   tiles and the activity feed both report one, and they have to look the
   same: the reader is comparing the number in a tile against the number on
   the day that moved it.

   Renders nothing when the score didn't move — "0" with an arrow beside it
   would claim a direction that isn't there. */
export default function Delta({ value }: { value: number }) {
  if (value === 0) return null;
  const up = value > 0;
  return (
    <span className={up ? 'delta is-up' : 'delta is-down'}>
      <Icon icon={up ? 'arrow-up' : 'arrow-down'} size={9} />
      {Math.abs(value)}
    </span>
  );
}
