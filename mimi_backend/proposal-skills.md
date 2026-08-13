# Proposal: the skill tree — words, skills, rows, castles

**Status:** implemented. Supersedes the course-shape half of the previous design; the
three-mode spaced repetition engine survives intact underneath it.

## 1. Why

The course model today is a four-level coordinate (`section > unit > node > lesson`)
addressing hand-authored lesson files, each of which carries a task `pattern`, a
`content` map keyed by pattern letters, a `new_exercises` pool, and answers marked up
by hand with `[C_hola=Hola]` concept spans. Every one of those pieces is authored, and
every one of them is a place two authors can disagree with each other.

It also doesn't scale to a generated course. If sentences are produced offline in bulk,
nobody is going to hand-place them into `pattern` strings or hand-mark their concept
spans.

This proposal replaces that with a structure borrowed from old Duolingo, in which the
unit of authoring is the **skill**: a themed batch of words that also carries a grammar
focus. It is a smaller model in every dimension — fewer files, fewer levels, no pattern
language, no marker syntax, no scripted/pool distinction — and it is a model a generator
can fill in.

**None of the learning machinery changes.** `card.rs`, `concept.rs`, the ladder, and
both phases of the lesson builder care only about which words a user has state for and
which exercises are eligible. This is a change to how content is *addressed*, not to how
it is *taught*.

## 2. The model

A course is:

- **A word list.** A few thousand words in roughly frequency order, in dictionary form.
  A word is the atom of spaced repetition — one `WordState`, up to three cards, exactly
  as concepts work today. **The learner is tested on whatever inflection a sentence
  happens to use, and all of them feed the one card.** This is deliberate: "do you know
  *comer*" is the question, and asking it through `comí` one day and `comen` the next is
  a feature.
- **Skills.** A skill is a small batch of words with a name (`Food 1`, `Past 1`) and a
  **grammar focus** — a short instruction describing what shape its sentences take.
  Every word in the course belongs to **exactly one** skill, so skills partition the
  vocabulary. The words say what the sentences are *about*; the focus says what shape
  they take. The focus is an authoring instruction and a blurb for the learner; **it is
  never tracked, scheduled, or graded.**
- **Rows.** A few skills side by side. Skills in a row unlock together and may be done
  in any order; the next row opens when the current one is finished. This is what makes
  the course a tree rather than a queue.
- **Castles.** A castle groups a run of consecutive rows and ends in a test drawn
  exclusively from the words in those rows. Castle 0 is the stretch before the first
  test. Passing a castle unlocks the first row of the next one.
- **Sentences.** Per skill, a corpus of source/target sentence pairs, each explicitly
  tagged with the skill's words it exercises.

```
         castle 0                          castle 1
 ┌────────────────────────┐       ┌──────────────────────────┐
 row 1   [Greetings][Numbers]      row 4  [Food 1][Body]
 row 2   [Family][Verbs 1]         row 5  [Past 1][Places]
 row 3   [Colours]                 row 6  [Food 2]
         ═══ TEST ═══                      ═══ TEST ═══
```

### Why skills own their sentences

A sentence may tag **only words from its own skill**. This is a deliberate restriction,
and it buys three things:

1. **Generation is well-posed.** "Write sentences using these seven words in this
   grammar focus" is a prompt a generator can execute and a human can check.
2. **The corpus is predictable.** Every word's review pool is fixed at authoring time
   and knowable by counting: it is exactly the sentences in its own skill that tag it.
   Letting later skills tag earlier words would not create sentences, only redistribute
   a fixed budget across thousands of competing words — with the count for any given
   word becoming a matter of luck.
3. **Related words appear together.** A skill's words are thematically close, so
   sentences that combine them are pedagogically better than sentences that combine a
   food word with whatever else happened to be in scope.

A sentence may contain untagged words from earlier rows — "Como pan" in a food skill
uses `comer` — and those simply aren't graded. **The consequence to design around is
that sentences-per-word is a hard quality knob**: a word is reviewed through its own
skill's sentences for the rest of the course, so author generously (8–12 per word,
before bracket expansion) rather than minimally.

## 3. On disk

```
courses/spanish/
├── index.json          {"kind": "course_index", id, source_lang, target_lang, credits}
├── words.json          the vocabulary, in frequency order
├── layout.json         castles → rows → skill ids: the whole structure of the tree
└── skills/food_1.json  one file per skill: words, material, sentences
```

Four levels of nested directories become four files. The structure lives in exactly one
place (`layout.json`) and the content in another (`skills/`), so a skill file never
needs to know where it sits.

### `words.json`

```jsonc
{
    "kind": "word_list",
    "words": [
        {
            "id": "comer",
            "word": "comer",
            "forms":   ["comer", "como", "comes", "come", "comemos", "coméis",
                        "comen", "comí", "comió", "comieron", "comido"],
            "glosses": ["eat", "eats", "ate", "eaten", "eating"]
        }
    ]
}
```

`forms` is every target-language form that can be **unambiguously** attributed to this
word; a form shared with another word is simply omitted (see §5). `glosses` is the same
list on the source-language side, and exists so that `es->en` answers can be graded
per-word too rather than all-or-nothing.

### `layout.json`

```jsonc
{
    "kind": "layout",
    "castles": [
        { "castle": 0, "rows": [["greetings_1", "numbers_1"],
                                ["family_1", "verbs_1"],
                                ["colours_1"]] },
        { "castle": 1, "rows": [["food_1", "body_1"], ["past_1"]] }
    ]
}
```

Rows are nested inside castles rather than numbered flat, so a castle boundary is
structurally incapable of falling mid-row. A skill's **row index** is its position in
the flattened list, and that index is the course's only ordinal (§6).

### `skills/food_1.json`

```jsonc
{
    "kind": "skill",
    "id": "food_1",
    "name": "Food 1",
    "focus": "Simple present-tense sentences about eating and drinking, using
              definite articles with the food nouns.",
    "lessons": 4,
    "words": ["pan", "agua", "comer", "beber", "leche", "manzana", "queso"],
    "material": [
        { "lesson": 1, "text": "Spanish nouns carry a gender..." },
        { "lesson": 3, "text": "**el agua**, not *la agua* — but it's still feminine." }
    ],
    "sentences": [
        { "direction": "es->en", "words": ["pan"],
          "prompt": "El pan.", "answer": "The bread." },
        { "direction": "en->es", "words": ["comer", "pan"],
          "prompt": "I eat the bread.", "answer": "[Yo como/Como] el pan." }
    ]
}
```

- **`lessons`** is a fixed count. Skills do not expire, degrade, or need re-clearing:
  once a skill is done it stays done, and keeping its words alive is the spaced
  repetition system's job, not the learner's. Mastery is checked at castles, not by
  quietly rotting the tree.
- **`material`** is plain markdown attached to a lesson number, and it teaches
  **nothing** in the technical sense — it introduces no words and creates no cards. It
  is a tip, shown up front. (Audio and character tags are dropped for now; a later
  revision can add an optional audio field.)
- **`sentences`** are authored per direction. An `es->en` sentence is not reversible
  into an `en->es` one — "Comió la naranja" → "He ate the orange" is correct, but the
  reverse admits "Él comió la naranja" too — which is exactly why both lists exist and
  each carries its own alternatives.

## 4. The load-time pipeline

Everything expensive happens once, in `loader::load`, and the runtime sees only finished
`Exercise` values. Four stages, in order:

### 4.1 Expand brackets

`[Yo como/Como] el pan` yields `Yo como el pan` and `Como el pan`. Brackets appear
**only in the answer** — the prompt is fixed — so this is a compact spelling of the
existing `accepted` list and involves no correlation with the prompt at all.

- Multiple groups take the cartesian product: `[Yo como/Como] [el/un] pan` → 4.
- An **empty branch** is legal: `la naranja[s]` → `la naranja`, `la naranjas`.
- **Nesting is rejected**, as is a bracket in a prompt.
- The **first** expansion is canonical (the answer shown as "correct"); the rest are
  accepted.

Expansion runs first because the word forms differ between variants, and the matcher in
§4.2 must see final text.

### 4.2 Locate each tagged word

For each sentence and each word it tags, find the word's span in the **answer** — the
side the learner produces, and therefore the side that is graded. `en->es` answers are
matched against `forms`, `es->en` answers against `glosses`.

Matching rules, all of which produce silent nonsense if left implicit:

- **Word boundaries, not substrings.** `es` must not match inside `estás`. Boundaries
  are Unicode-aware, since accented characters are letters.
- **Longest match wins, leftmost first, spans never overlap.** `del` beats `de`. Forms
  ambiguous *across the whole language* are already absent from `forms`, but two tagged
  words in one sentence can still offer overlapping candidates, and the resolution order
  has to be fixed rather than incidental.
- **Case-insensitive, punctuation-insensitive** at the edges; the original span is kept
  for display.

If every tagged word is located in every accepted variant, the exercise grades
**per word**, exactly as concept markers allow today. If any word is missing from any
variant — the sentence used a form the list doesn't cover — the exercise is marked
**all-or-nothing**: a fully correct answer marks every tagged word right, anything else
marks them all wrong.

All-or-nothing is set for the exercise, not per variant, so that grading doesn't depend
on which accepted answer the learner happened to hit. It is a graceful degradation: an
unanticipated form costs grading *precision*, never correctness.

### 4.3 Mint exercises

Each expanded sentence becomes **two** exercises in its own direction: a `translate` and
a `word_bank`. Ids are generated, never authored, following today's rule: `food_1:12:t`
and `food_1:12:b` for the twelfth sentence of `food_1`.

Both carry the sentence's `words`, its `row`, its `skill`, its accepted answers with
resolved spans, and the all-or-nothing flag.

### 4.4 Generate distractors

Word banks need wrong tiles. For each `word_bank`, sample surface tokens from the
answers of other sentences **in the same skill or an earlier row** — never a later one,
which would leak vocabulary the learner hasn't reached — excluding tokens already in the
answer. Sampling is **seeded by the exercise id**, so a bank looks the same every time
it is served and a re-taken lesson is genuinely the same lesson.

## 5. Grading without markers

The `[C_hola=Hola]` syntax, its parser (`exercise::markers`, `marked_concepts`,
`surface_tokens`), and every loader validation about it are deleted. §4.2 derives the
same information from data the course has to carry anyway.

Two properties are worth stating as invariants:

- **A word's card is updated by whatever form appeared.** One card per word, across all
  inflections, by design. The cost is that a hard inflection can demote a word the
  learner knows well in its base form; the benefit is that the model matches the actual
  goal, which is knowing the word.
- **The report shape is unchanged.** The client still returns
  `{exercise_id, correct, words}`, a missing response still counts as wrong, and a word
  missing from `words` still falls back to the overall verdict. All-or-nothing exercises
  are simply ones where the client always uses the fallback.

## 6. Progress, unlocking, and eligibility

`Position` shrinks from a four-level coordinate to `{ skill: SkillId, lesson: u8 }`, and
stops being a user's identity — it is only an address for a lesson request. A user is:

```
words:    HashMap<WordId, WordState>     // unchanged but for the rename
progress: HashMap<SkillId, u8>           // lessons completed, per skill
castles:  u8                             // how many castles are passed
```

Current row, available skills and the next lesson are all **derived**:

- A skill is **complete** when `progress[skill] == skill.lessons`.
- A row is **unlocked** when every skill in every earlier row is complete *and* every
  castle whose stretch ends before it is passed.
- A castle is **available** when every skill in its rows is complete.

### The eligibility rule

The single most important consequence of a branching tree is that progress is a *set*,
not a point, so `exercises_up_to(position)` cannot survive as written. It is replaced by
two clauses, each doing exactly one job:

```
a sentence is eligible for a lesson in skill S, for user U, iff
    row(sentence.skill) <= row(S)         ← the retake rule
  AND every word it tags is one U has met ← siblings and intra-skill order
```

The first clause is a **prefix slice on row**: the pool is sorted by row at load, so
`partition_point` still applies and `Course::exercises_up_to` survives with `row` as its
key. This is what stops a learner retaking `Greetings` from being shown row-9 material.

The second clause covers everything the first can't see. A learner retaking `Food 1` who
has never touched its row-mate `Body 1` is excluded from `Body 1`'s sentences; a learner
retaking lesson 2 of a 4-lesson skill is excluded from the words that skill introduces
in lessons 3 and 4. There is therefore **no lesson-level era rule at all** — being met
is the whole of it.

This clause needs to become a real gate. Today it is only *nearly* true: `User::allows`
treats an unmet word as Scaffolding-legal ([user.rs:70](src/user.rs:70)) and it is
`probability` returning 0.0 that keeps it out of lessons in practice. Under this model
that is load-bearing and must be enforced explicitly.

## 7. What a lesson is now

There is no `pattern`, no `content`, no `new_exercises`, and no scripted/pool
distinction. Lesson *n* of a skill is:

1. **Material** for lesson *n*, if any, served up front.
2. **One introducing word bank per new word.** A skill's `words` list is its
   introduction queue, split as evenly as possible across its `lessons` (7 words over 4
   lessons → 2, 2, 2, 1), remainder to the earlier lessons. The split is deterministic,
   so a retake introduces exactly what it did the first time.
3. **The rest filled by the existing builder** — `take_urgent`, then `top_up`, 85%
   targeting, easiest-first / second-easiest-last ordering, no repeated words. Unchanged.

The introducing exercise is a `word_bank` in the `es->en` direction over a sentence
tagging **only that word** — the gentlest possible first contact. This is a generation
requirement: **every word needs at least one `es->en` sentence tagging it alone.**

Three consequences worth being explicit about:

- **`User::learn` and `WordState::taught` are deleted.** Material no longer teaches, so
  the only way a word enters memory is a verdict on a real answer. That was already the
  preferred path (`AGENTS.md`: "prefer putting `introduces` on the exercise"); now it is
  the only one.
- **The scripted/pool split disappears.** Introducing exercises came out of the pool and
  go back into it. The old rule barring them existed because a scripted exercise sat
  directly after the material that taught it, so answering it proved nothing — with no
  teaching material, it is a genuine first attempt and there is no reason to bar it from
  later review. The "skip scripted exercises in a re-taken lesson" rule goes too.
- **The client must show the gloss on first contact.** A learner assembling "bread" from
  a word bank for a word they have never seen is otherwise guessing blind. The task
  carries a flag saying which words it introduces; the client does the rest.

## 8. Castles

A castle test is a **separate builder**, not a mode of `Lesson::build`. An
85%-targeted test is by construction a test everyone passes at 85%.

```
sample evenly across every word in the castle's rows
  → for each, pick a sentence tagging it; modes are Recognition and Production only
  → no urgency phase, no 85% targeting, no introductions, no word banks
  → CASTLE_SIZE (20) questions; CASTLE_PASS (0.8) to pass
  → a failure re-rolls a fresh sample
```

**Legality is bypassed on the way in.** A castle asks what it asks; the ladder's job is
to prepare the learner for it, and a test restricted to what the ladder already permits
would not be testing the thing worth testing. Every word in the stretch is guaranteed to
have been *met*, because reaching the castle requires completing every skill in its rows
and completing a skill introduces all of its words — so there is no unmet-word case to
handle.

**Verdicts at an illegal mode are asymmetric**, and this needs a code change:

| | word allows the mode | word does not |
|---|---|---|
| **correct** | normal `record` | credit it: card + streak, as `mode >= top` already implies |
| **wrong** | normal `record` | **ignore entirely** — no card, no counter |

The castle can help a learner and can never hurt one for material they weren't expected
to know. Someone who can produce a word they were only ever scaffolded on has
demonstrated something real, and the existing "a success at a harder mode counts at
least as much" rule is exactly right for it.

**This does not fall out of the current code.** [concept.rs:199](src/concept.rs:199)
updates the card *unconditionally*, before any legality check; only the ladder counters
are gated, and only on the failure branch ([concept.rs:225](src/concept.rs:225)). As
written, a failed production question on a Scaffolding word mints a fresh production
card in the `again` state. It would never be served — `due()` only yields legal modes —
but it sits there, and when the word finally graduates to Production weeks later it
inherits a damaged card instead of FSRS's first-review initialization. That is precisely
the punishment this rule exists to prevent, merely deferred. `ConceptState::record`
needs the legality check moved above `set_card`.

Failing a castle costs nothing but a retry, and the pressure to review comes from not
passing: a learner who has forgotten too much cannot pass a fresh sample either, and the
only way through is more lessons. That is the intended effect — capping how fast new
material piles onto a shaky foundation — and it needs no punishment mechanic to work.

## 9. What the engine keeps

Unchanged in substance, renamed in the obvious way (`concept` → `word`,
`ConceptState` → `WordState`, `Exercise.concepts` → `Exercise.words`, `by_concept` →
`by_word`):

- Three modes, three cards per word, one verdict updates one card, never cross-updated.
- The whole ladder: stages, streaks, lapses, demotion, re-promotion, the decay sweep.
- Both builder phases, `needed_average`, `MIN_URGENT`/`MAX_NEEDED`, the ordering rule.
- The no-repeated-words rule, which remains load-bearing for the independence assumption.
- The activity table, the derived profile, and the seeder's approach of *playing* the
  course rather than writing totals down.

`Course::glossary` is deleted. Display glosses come from the course's Wiktionary-derived
dictionary, which covers tagged vocabulary and untagged sentence words alike; `forms`
and `glosses` remain the separate, deliberately curated inputs to credit assignment.

The score becomes `400 + words×1.6 + skills×40 + lessons×0.5` — a straight substitution
of skills for units.

## 10. HTTP API

- **`GET /users/{u}/course`** returns castles → rows → skills, each skill marked
  `completed` / `available` / `locked` with `lessons_done`, plus each castle marked
  `passed` / `available` / `locked`.
- **`POST /users/{u}/lessons`** takes `{"skill": "food_1", "lesson": 2}`, or no body for
  the next lesson due. 404 unknown skill/lesson, 403 locked. Response shape unchanged.
- **`POST /users/{u}/castles`** builds the currently available castle test and returns
  the same `{lesson_id, tasks}` shape.
- **`POST /lessons/{id}/submit`** is unchanged in shape; the response gains `passed` when
  the submission was a castle.
- **`GET /users/{u}`** reports per-word ladder state, as today.

## 11. What gets deleted

Worth listing, because shrinking the system is the point:

- The four-level directory tree and `Position`'s `section`/`unit`/`node` fields.
- `pattern`, `task_count`, `content` letters, `new_exercises`, and every validation
  about them.
- The entire `[C_x=...]` marker syntax and its parser.
- `script.rs` in full — `Script`, `Slot`, `Material`, `ScriptedExercise`, `introduces`.
- `User::learn`, `WordState::taught`, and the scripted-exercise carve-outs in
  `submit_lesson`.
- `Course::glossary` and the marker-scraping it does.
- Roughly two-thirds of `loader.rs`'s validations, replaced by: every skill in
  `layout.json` exists and appears exactly once, every word belongs to exactly one
  skill, every sentence tags only its own skill's words, every word has an `es->en`
  solo sentence, `material` lesson numbers are in range, and brackets are well-formed.

## 12. Deliberate non-goals and known approximations

- **No migration.** `mimi.db` is deleted, per existing convention. More significantly,
  **`courses/spanish` cannot be converted mechanically** — there is no word list, no
  forms, and the markers are hand-written. This proposal implies regenerating the course
  content, and the loader tests will need a small hand-written fixture course in the new
  shape.
- **Grammar is never tracked.** A learner weak on plurals gets no targeted remediation.
  Accepted deliberately: making the focus a trackable concept would put one shared
  concept in every sentence of a skill, and the no-repeat rule would then permit exactly
  one of them per lesson. The cure is worse than the disease.
- **One card per word across all inflections**, as §5 notes.
- **Untagged filler words are never graded**, so an all-or-nothing failure may be caused
  by a word the exercise isn't testing. Minor noise; the alternative is tagging
  everything, which defeats §2.
- **Castle sampling is uniform over words**, not weighted by how shaky each one is. A
  test that targeted the learner's weak spots would be a better diagnostic and a worse
  test.
- Not in scope: audio in material, placement tests, per-skill "practice" sessions
  outside the lesson structure, and the offline generator itself.

## 13. Implementation plan

1. **`words.rs` (new) + `position.rs`.** The word list, forms, glosses; `Position`
   becomes `{skill, lesson}`; row as the ordinal.
2. **`sentence.rs` (new).** Bracket expansion and the form matcher, with their own
   tests — these are pure functions over strings and the easiest thing to get right
   first.
3. **`loader.rs`.** The four new file shapes, the §4 pipeline, distractor generation,
   the reduced validation set. Delete `script.rs`.
4. **`course.rs`.** Pool sorted by row, `by_word`, skill/layout lookups,
   `exercises_up_to(row)`. Delete `Glossary` scraping.
5. **`concept.rs`.** Rename to word vocabulary; move the legality check above
   `set_card`; add the castle-mode entry point.
6. **`user.rs`.** `progress`/`castles` fields, the derived unlock rules, the explicit
   met-word gate in `allows`, deletion of `learn`.
7. **`lesson.rs`.** The new lesson plan (material → introductions → holes) and the
   separate castle builder.
8. **`server.rs` / `api.rs`.** The course map, the skill-addressed lesson request, the
   castle endpoints.
9. **`seed.rs`.** Walk skills and castles instead of positions.
10. **Content.** Generate `courses/spanish` in the new shape; write the fixture course
    the loader tests use.

Steps 1–2 are self-contained and testable in isolation; nothing else compiles until 3–4
land, so those three should be one change.
