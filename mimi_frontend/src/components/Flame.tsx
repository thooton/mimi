interface Props {
  size?: number;
  className?: string;
}

/* Streak flame — a round-bodied glyph drawn for solid fill (the borrowed
   outline glyph it replaced pinched and blobbed at navbar size).
   Colored entirely by the parent's `color`, via currentColor. */
export default function Flame({ size = 20, className }: Props) {
  return (
    <svg
      className={className}
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="currentColor"
      aria-hidden="true"
    >
      <path d="M12 3.2c.5 2.6-.7 4.1-1.8 5.7a6.4 6.4 0 0 0-1.7 4c-.8-.6-1.4-1.5-1.6-2.6-1.3 1.7-2 3.7-2 5.2a7.1 7.1 0 1 0 14.2 0c0-1.9-.5-3.5-1.3-4.9-.3 1.1-.9 2-1.8 2.5.5-3.3-.9-7.3-4-9.9z" />
    </svg>
  );
}
