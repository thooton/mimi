/* Saying a line of the target language out loud.

   The backend has no audio to serve: a `text_audio` block is text plus the
   name of the character who speaks it (see mimi_backend script.rs), and
   playing it is left to the client. The browser's speech synthesiser is what
   we have, so that is what we use — it is not a voice actor, but a silent
   speaker button would be worse than an approximate one. */

/** language codes as the course writes them → what a voice is tagged with */
const VOICE_LANG: Record<string, string> = {
  en: 'en-US',
  es: 'es-ES',
  ja: 'ja-JP',
};

export function speechAvailable(): boolean {
  return typeof window !== 'undefined' && 'speechSynthesis' in window;
}

/* The course names its characters ("girl_generic", "boy_generic"). We can't
   cast them, but pitch costs nothing and keeps two speakers in a dialogue
   apart, which is the part that carries meaning. */
function pitchFor(character: string): number {
  if (/girl|woman|female/.test(character)) return 1.2;
  if (/boy|man|male/.test(character)) return 0.8;
  return 1;
}

/**
 * Speak one line, cancelling whatever was being said before it — a learner
 * who taps two lines in a row wants the second one, not both at once.
 */
export function speak(text: string, lang: string, character = ''): void {
  if (!speechAvailable()) return;
  const utterance = new SpeechSynthesisUtterance(text);
  utterance.lang = VOICE_LANG[lang] ?? lang;
  utterance.pitch = pitchFor(character);
  // native pace is a lot for a beginner reading along
  utterance.rate = 0.9;
  window.speechSynthesis.cancel();
  window.speechSynthesis.speak(utterance);
}
