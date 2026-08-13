import { strict as assert } from "node:assert";
import { test } from "node:test";
import { attachGlosses } from "./glosses.ts";

const gloss = (text: string, ...meanings: string[]) => ({ text, meanings });

/** the spans as "word[hint]" / "plain", which is what the reading looks like */
const shape = (text: string, glosses: { text: string; meanings: string[] }[]) =>
  attachGlosses(text, glosses).spans.map((span) =>
    span.gloss ? `${span.text}[${span.gloss.meanings.join("|")}]` : span.text,
  );

test("every glossed word carries its own hint while punctuation stays plain", () => {
  const spans = shape("Yo soy un hombre.", [
    gloss("Yo", "I"),
    gloss("soy", "to be"),
    gloss("un", "a, one"),
    gloss("hombre", "man"),
  ]);
  assert.deepEqual(spans, ["Yo[I]", " ", "soy[to be]", " ", "un[a, one]", " ", "hombre[man]", "."]);
});

test("the text between the hints is kept, so the sentence still reads", () => {
  const { spans, orphans } = attachGlosses("¿Dónde está la casa?", [gloss("casa", "house")]);
  assert.equal(spans.map((span) => span.text).join(""), "¿Dónde está la casa?");
  assert.deepEqual(orphans, []);
});

// the walk is what tells the two apart: the second hint is for the second one
test("a word glossed twice gets a hint on each occurrence, in order", () => {
  assert.deepEqual(
    shape("que sea lo que sea", [gloss("que", "that"), gloss("que", "what")]),
    ["que[that]", " sea lo ", "que[what]", " sea"],
  );
});

/* The case this gives up on rather than guesses at: a gloss quoting something
   the sentence doesn't contain has nothing to underline. It comes back as an
   orphan so the player can still show it — dropping a hint silently is the
   failure worth avoiding. */
test("a gloss the sentence never quotes comes back as an orphan", () => {
  const { spans, orphans } = attachGlosses("Yo soy un hombre.", [
    gloss("ser", "to be"),
    gloss("hombre.", "man"),
  ]);
  assert.deepEqual(orphans.map((orphan) => orphan.text), ["ser"]);
  assert.equal(spans.map((span) => span.text).join(""), "Yo soy un hombre.");
  assert.equal(spans.filter((span) => span.gloss).length, 1);
});

test("an orphan doesn't cost the hints after it their place", () => {
  assert.deepEqual(shape("Yo soy", [gloss("Yo", "I"), gloss("ser", "to be"), gloss("soy", "am")]), [
    "Yo[I]",
    " ",
    "soy[am]",
  ]);
});

test("no glosses leaves the sentence in one piece", () => {
  const { spans, orphans } = attachGlosses("Buenos días", []);
  assert.deepEqual(spans, [{ text: "Buenos días" }]);
  assert.deepEqual(orphans, []);
});
