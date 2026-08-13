import { strict as assert } from "node:assert";
import { test } from "node:test";
import { grade, gradeWordBank } from "./grading.ts";
import type { Answer } from "./linguisticGrade.ts";

// the forgiving details (accents, typos, per-concept spans) are covered in
// linguisticGrade.test.ts; these only cover the two adapters the lesson
// player speaks to

/* Answers as the backend serves them, written here in the compact marker
   notation "¡[C_hola=Hola], Juan!" and expanded into text-plus-spans. See the
   same helper in linguisticGrade.test.ts. */
const answers = (...marked: string[]): Answer[] =>
  marked.map((m) => {
    let text = "";
    const words: Answer["words"] = [];
    let rest = m;
    for (let open = rest.indexOf("["); open !== -1; open = rest.indexOf("[")) {
      const close = rest.indexOf("]", open);
      const [id, surface] = rest.slice(open + 1, close).split("=");
      text += rest.slice(0, open);
      words.push({ word: id, start: text.length, end: text.length + surface.length });
      text += surface;
      rest = rest.slice(close + 1);
    }
    return { text: text + rest, words };
  });

test("the right tokens in the right order are right", () => {
  const verdict = gradeWordBank(["Buenos", "días"], answers("[C_buenos_dias=Buenos días]."));
  assert.equal(verdict.correct, true);
  assert.deepEqual(verdict.concepts, { C_buenos_dias: true });
});

// the whole point of joining before grading: the bank's bare words can't
// reproduce "¡Hola, Juan!", and don't have to
test("the canonical answer's punctuation is not the user's problem", () => {
  const verdict = gradeWordBank(["Hola", "Juan"], answers("¡[C_hola=Hola], Juan!"));
  assert.equal(verdict.correct, true);
});

test("wrong words, extra tokens and the wrong order all fail", () => {
  const accepted = answers("¡[C_hola=Hola], Juan!");
  assert.equal(gradeWordBank(["Adiós"], accepted).correct, false);
  assert.equal(gradeWordBank(["Hola", "Juan", "Adiós"], accepted).correct, false);
  assert.equal(gradeWordBank(["Juan", "Hola"], accepted).correct, false);
});

test("a word-bank answer is graded as leniently as a typed one", () => {
  // missing accent (mañana -> manana), one typo (Hasta -> Hsata)
  const verdict = gradeWordBank(
    ["Hsata", "manana", "Ana"],
    answers("[C_hasta_manana=Hasta mañana], Ana."),
  );
  assert.equal(verdict.correct, true);
});

// a half-assembled sentence still credits the concepts it got right
test("per-concept verdicts come through the join", () => {
  const verdict = gradeWordBank(
    ["Hola", "y", "hasta", "mañana"],
    answers("[C_hola=Hola] y [C_adios=adiós]."),
  );
  assert.equal(verdict.correct, false);
  assert.deepEqual(verdict.concepts, { C_hola: true, C_adios: false });
});

/* An exercise testing one concept arrives with no spans, because the sentence
   is that concept's whole question. Getting the word right inside a wrong
   sentence is not getting the exercise right, and the ladder must not be told
   it was, this is the case the span format would otherwise be too clever
   about. */
test("a single-concept exercise is graded all or nothing", () => {
  const accepted = answers("The girl."); // no spans: one concept under test
  assert.deepEqual(gradeWordBank(["The", "girl"], accepted, ["nina"]).concepts, {
    nina: true,
  });
  // the right word in the wrong sentence is a wrong answer, `nina` included
  const wrong = gradeWordBank(["Juan", "girl"], accepted, ["nina"]);
  assert.equal(wrong.correct, false);
  assert.deepEqual(wrong.concepts, { nina: false });
});

// Some legitimate translations use a form the glossary does not enumerate,
// so the backend cannot put a precise span around it. The served word list
// keeps that concept in the memory delta using the whole-answer fallback.
test("unmarked expected concepts take the overall verdict", () => {
  assert.deepEqual(grade("Juan eats apples.", answers("Juan eats apples."), ["manzana"]), {
    correct: true,
    concepts: { manzana: true },
    canonical: "Juan eats apples.",
  });
  assert.deepEqual(grade("Juan eats pears.", answers("Juan eats apples."), ["manzana"]), {
    correct: false,
    concepts: { manzana: false },
    canonical: "Juan eats apples.",
  });
});

test("marked concepts stay precise while only missing concepts fall back", () => {
  const verdict = grade("Hola y mañana", answers("[hola=Hola] y [adios=adiós]"), [
    "hola",
    "adios",
    "scenery",
  ]);
  assert.equal(verdict.correct, false);
  assert.deepEqual(verdict.concepts, { hola: true, adios: false, scenery: false });
});

test("a wrong word-bank answer still shows the canonical answer, punctuation and all", () => {
  const verdict = gradeWordBank(["Adiós"], answers("¡[C_hola=Hola], Juan!"));
  assert.equal(verdict.canonical, "¡Hola, Juan!");
});

test("an exercise with no accepted answers is wrong, however answered", () => {
  assert.deepEqual(grade("anything", []), { correct: false, concepts: {}, canonical: "" });
  assert.deepEqual(gradeWordBank(["anything"], []), {
    correct: false,
    concepts: {},
    canonical: "",
  });
});
