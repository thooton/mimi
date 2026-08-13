import { strict as assert } from "node:assert";
import { test } from "node:test";
import { linguisticGrade, type Answer } from "./linguisticGrade.ts";

/* The cases below write an answer in a compact marker notation,
   "¡[C_hola=Hola], Juan!", and this expands it into the shape the backend
   actually serves: the plain text, plus the span of it that proves each
   concept. The notation is a fixture and nothing more; no code outside these
   tests has parsed anything like it since the wire format became structured. */
const answer = (marked: string): Answer => {
  let text = "";
  const words: Answer["words"] = [];
  let rest = marked;
  for (let open = rest.indexOf("["); open !== -1; open = rest.indexOf("[")) {
    const close = rest.indexOf("]", open);
    const [id, surface] = rest.slice(open + 1, close).split("=");
    text += rest.slice(0, open);
    words.push({ word: id, start: text.length, end: text.length + surface.length });
    text += surface;
    rest = rest.slice(close + 1);
  }
  return { text: text + rest, words };
};

const answers_ = (marked: string[]): Answer[] => marked.map(answer);

// shorthand for the tests that only care whether the whole answer landed
const isCorrect = (answers: string[], response: string) =>
  linguisticGrade(response, answers_(answers)).overallCorrect;

const concept = (answers: string[], response: string, id: string) =>
  linguisticGrade(response, answers_(answers)).concepts.find((c) => c.id === id)
    ?.correct;

// --- ported from given.rs ---

test("correct accepts canonical and alternatives", () => {
  const answers = ["¡[C_hola=Hola], Juan!", "¡[C_hola=Buenas], Juan!"];
  assert.equal(isCorrect(answers, "¡Hola, Juan!"), true);
  assert.equal(isCorrect(answers, "buenas juan"), true); // sloppy but right
});

test("correct rejects wrong answers", () => {
  const answers = ["¡[C_hola=Hola], Juan!", "¡[C_hola=Buenas], Juan!"];
  assert.equal(isCorrect(answers, "adiós juan"), false);
  assert.equal(isCorrect(answers, ""), false);
});

test("correct forgives accents and typos", () => {
  const answers = ["[C_hasta_manana=Hasta mañana], Ana."];
  assert.equal(isCorrect(answers, "Hasta manana, Ana."), true); // no ñ available
  assert.equal(isCorrect(answers, "Hasta mañaan, Ana."), true); // fumbled it
  assert.equal(isCorrect(answers, "Hasta mañana."), false); // dropped a word
  assert.equal(isCorrect(answers, "Hasta mañana, Ana, adiós."), false); // added one
});

// the point of the concept spans: a half-right sentence tells us something
// different about each concept in it
test("grading scores each concept separately", () => {
  const answers = ["[C_hola=Hola] y [C_adios=adiós]."];
  const grade = linguisticGrade("Hola y hasta mañana.", answers_(answers));
  assert.equal(grade.overallCorrect, false);
  assert.deepEqual(grade.concepts, [
    { id: "C_hola", correct: true },
    { id: "C_adios", correct: false },
  ]);
});

test("grading scores multi-word concepts", () => {
  const answers = ["¿[C_como_estas=Cómo estás], [C_sofia=Sofía]?"];
  // only half of the two-word concept survived
  const grade = linguisticGrade("¿Cómo, Sofía?", answers_(answers));
  assert.equal(concept(answers, "¿Cómo, Sofía?", "C_como_estas"), false);
  assert.equal(concept(answers, "¿Cómo, Sofía?", "C_sofia"), true);
  assert.equal(grade.overallCorrect, false);
});

test("a perfect answer marks every concept right", () => {
  const answers = ["[C_hola=Hola] y [C_adios=adiós]."];
  const grade = linguisticGrade("hola y adios", answers_(answers)); // no accent, still right
  assert.equal(grade.overallCorrect, true);
  assert.ok(grade.concepts.every((c) => c.correct));
});

// the user is graded against the alternative they actually attempted, not
// against whichever one the course lists first
test("grading picks the closest accepted answer", () => {
  const answers = [
    "[C_que_tal=What's up], Carlos?",
    "[C_que_tal=How are you], Carlos?",
  ];
  assert.equal(isCorrect(answers, "How are you, Carlos?"), true);
  // and a near miss on the alternative is judged against the alternative
  const grade = linguisticGrade("How are you, Ana?", answers_(answers));
  assert.equal(grade.overallCorrect, false);
  assert.equal(concept(answers, "How are you, Ana?", "C_que_tal"), true);
});

// an omission in the middle must not drag the words after it out of
// alignment, that's why grading aligns instead of comparing pairwise
test("grading realigns around a missing word", () => {
  const answers = ["[C_muy=Muy] [C_mal=mal], [C_ana=Ana]."];
  const grade = linguisticGrade("Mal, Ana.", answers_(answers));
  assert.deepEqual(grade.concepts, [
    { id: "C_muy", correct: false },
    { id: "C_mal", correct: true },
    { id: "C_ana", correct: true },
  ]);
});

// a client can send anything; grading it must stay cheap and say "no"
test("an absurdly long response is just wrong", () => {
  const answers = ["[C_hola=Hola], Juan!"];
  const grade = linguisticGrade("hola juan ".repeat(10_000), answers_(answers));
  assert.equal(grade.overallCorrect, false);
  assert.equal(concept(answers, "hola juan ".repeat(10_000), "C_hola"), true); // it did say "hola"
});

test("an empty response gets every concept wrong", () => {
  const answers = ["[C_hola=Hola] y [C_adios=adiós]."];
  const grade = linguisticGrade("", answers_(answers));
  assert.equal(grade.overallCorrect, false);
  assert.ok(grade.concepts.every((c) => !c.correct));
});

// a concept the matched answer says nothing about simply isn't reported
test("unmarked concepts are absent from the report", () => {
  const grade = linguisticGrade("Hola, Juan!", answers_(["[C_hola=Hola], Juan!"]));
  assert.equal(grade.concepts.find((c) => c.id === "C_adios"), undefined);
});

// a concept marked in two places has to be right in both
test("a repeated concept must be right everywhere", () => {
  const answers = ["[C_no=No], [C_no=no], [C_no=no]!"];
  assert.equal(concept(answers, "No, no, no!", "C_no"), true);
  assert.equal(concept(answers, "No, no, sí!", "C_no"), false);
});

// token-level behavior, exercised through whole grades
test("short words get no typo forgiveness", () => {
  // "no" -> "yo" and "mal" -> "más" are one edit, but the words are too short
  assert.equal(isCorrect(["[C_no=No] voy"], "Yo voy"), false);
  assert.equal(isCorrect(["[C_mal=mal] hoy"], "más hoy"), false);
});

// The old wire format wrote its concepts into the answer string, so a "[" in
// a real sentence was ambiguous with a marker and had to be escaped or banned.
// Spans have no such problem: an answer's text is text.
test("brackets in an answer are ordinary text", () => {
  const grade = linguisticGrade("a [b] c", [
    { text: "a [b] c", words: [{ word: "C_a", start: 0, end: 1 }] },
  ]);
  assert.equal(grade.overallCorrect, true);
  assert.deepEqual(grade.concepts, [{ id: "C_a", correct: true }]);
});

test("apostrophes are optional", () => {
  assert.equal(isCorrect(["[C_im=I'm] bad"], "im bad"), true);
  assert.equal(isCorrect(["[C_im=Im] bad"], "I'm bad"), true);
});

// --- generalized to any language ---

test("french: accents forgiven, typos forgiven", () => {
  const answers = ["[C_eleve=Élève] [C_ecoute=écoute] [C_prof=le professeur]"];
  assert.equal(isCorrect(answers, "Eleve ecoute le professeur"), true); // no accents
  assert.equal(isCorrect(answers, "élève ecote le professeur"), true); // one typo
  assert.equal(isCorrect(answers, "élève regarde le professeur"), false);
});

test("german: umlauts forgiven", () => {
  const answers = ["[C_madchen=Das Mädchen] [C_isst=isst] [C_brot=das Brot]"];
  assert.equal(isCorrect(answers, "das madchen isst das brot"), true);
  assert.equal(isCorrect(["[C_uber=Über] [C_morgen=morgen]"], "uber morgen"), true);
});

test("portuguese: tildes and cedillas forgiven", () => {
  const answers = ["[C_acao=A ação] [C_coracao=do coração]"];
  assert.equal(isCorrect(answers, "a acao do coracao"), true);
});

test("non-latin scripts grade by the same rules", () => {
  // greek: tonos (ά -> α) forgiven via combining-mark stripping
  assert.equal(isCorrect(["[C_kalimera=Καλημέρα σας]"], "καλημερα σας"), true);
  // cyrillic: exact match required (no accents to fold)
  assert.equal(isCorrect(["[C_privet=Привет мир]"], "привет мир"), true);
  assert.equal(isCorrect(["[C_privet=Привет мир]"], "привет"), false);
});

test("cjk: each run of ideographs is a token", () => {
  assert.equal(isCorrect(["[C_nihao=你好]"], "你好"), true);
  assert.equal(isCorrect(["[C_nihao=你好]"], "您好"), false);
});

test("digits are word characters", () => {
  assert.equal(isCorrect(["[C_num=I have 3 cats]"], "i have 3 cats"), true);
  assert.equal(isCorrect(["[C_num=I have 3 cats]"], "i have 4 cats"), false);
});

test("answers must be non-empty", () => {
  assert.throws(() => linguisticGrade("hola", []));
});
