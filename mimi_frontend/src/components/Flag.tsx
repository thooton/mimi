interface Props {
  /** ISO 3166-1 alpha-2, naming a file in /public/flags */
  region: string;
  size?: number;
  className?: string;
  /** 1x1 for the round badges, 4x3 for the rectangular navbar flag */
  aspect?: '1x1' | '4x3';
}

/* The flag SVGs are vendored from flag-icons (MIT) — the square 1x1 cuts
   suit the round language badges, and the 4x3 cuts fill the compact navbar
   flag. The navbar can't just crop the square art: cover-cropping a square
   to 4:3 shaves the top and bottom, which for a three-bar flag like Germany
   leaves the middle bar visibly taller than its neighbours.

   Importing the package's stylesheet would have been less typing and dragged
   all 546 flags into the build; the thirty we offer are 200KB sitting in
   /public, fetched one at a time as they appear.

   Always decorative: a flag never appears without its language named beside
   it, so announcing it again would only make a screen reader say Spanish
   twice. */
export default function Flag({ region, size = 22, className, aspect = '1x1' }: Props) {
  return (
    <img
      className={className ? `flag ${className}` : 'flag'}
      src={aspect === '4x3' ? `/flags/4x3/${region}.svg` : `/flags/${region}.svg`}
      width={size}
      height={size}
      alt=""
      aria-hidden="true"
      loading="lazy"
      draggable={false}
    />
  );
}
