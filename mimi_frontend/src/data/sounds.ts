/* The chimes an answer gets on a verdict, correct.mp3 when it's right,
   wrong.mp3 when it isn't, and the fanfare a finished lesson gets,
   triumph.mp3. Static files served as-is from public/sounds, no audio API
   fancier than an <audio> element is needed for three short clips, and each
   play builds a fresh one so a quick retry can overlap the tail of the last
   chime instead of cutting it off. */

/** Play a sound. Sound is a nicety, never a blocker: a browser that refuses
    to play (autoplay policy, no audio device) fails silently. */
function play(name: string): void {
  if (typeof window === 'undefined') return;
  const audio = new Audio(`/sounds/${name}.mp3`);
  void audio.play().catch(() => {});
}

/** Play the verdict chime. */
export function playVerdict(correct: boolean): void {
  play(correct ? 'correct' : 'wrong');
}

/** Play the fanfare as the lesson summary settles in. */
export function playTriumph(): void {
  play('triumph');
}
