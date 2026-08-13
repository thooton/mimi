// Turns the course definitions projected by `convert` into the one `Course`
// the server serves. Keeping assembly separate from conversion makes the
// definitions the interface: every source-side shape is gone by the time this
// module validates content and builds the runtime course.
//
// Assembly is also where **everything expensive about a sentence happens**,
// once, so that the runtime only ever reads finished, marked-up prose:
//
//   1. **expand** each authored wording's brackets into the variants it
//      stands for,
//   2. **locate** each tagged word in each variant, by its forms — but only
//      for a sentence that tags two or more, since one word alone is graded
//      all or nothing (see `wording`),
//   3. **record** the located spans on the phrasing, so the client can grade
//      word by word without looking for anything itself,
//   4. **locate again for presentation** in the preferred target wording,
//      including one-word sentences, so every first contact can highlight the
//      word it announces without pretending that highlight is grading or a
//      dictionary gloss.
//
// What it deliberately does *not* do is build exercises. A sentence has no
// direction and no mode: it becomes a question only once a lesson has chosen
// one of the four ways to ask it, which is a decision about a *learner* and so
// cannot be made here (see course.rs).
//
// And it is where the content is validated. Bad data should stop the server at
// boot with a clear message, not turn into a strange lesson hours later.

use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::error::Error;

use crate::course::Course;
use crate::exercise::Side;
use crate::sentence::{Phrasing, Sentence, Span, Wording, expand, locate};
use crate::skill::{Castle, MaterialBlock, SKILL_LESSONS, Skill};
use crate::vocab::{Vocab, Word};

#[derive(Deserialize)]
pub struct CourseIndex {
    pub id: String,
    pub source_lang: String,
    pub target_lang: String,
}

#[derive(Deserialize)]
pub struct WordList {
    pub words: Vec<WordDef>,
}

#[derive(Deserialize)]
pub struct WordDef {
    pub id: String,
    pub word: String,
    // every target-language form. Forms shared with another word are kept:
    // `sentence::locate` drops the ones that clash with the other words a
    // given sentence tags, which is the only place a clash can mislead.
    #[serde(default)]
    pub forms: Vec<String>,
    // the same on the source-language side, so es->en answers can be graded
    // word by word too
    #[serde(default)]
    pub glosses: Vec<String>,
}

#[derive(Deserialize)]
pub struct Layout {
    pub castles: Vec<CastleDef>,
}

#[derive(Deserialize)]
pub struct CastleDef {
    pub castle: usize,
    // the rows of this castle's stretch, each a list of skill ids
    pub rows: Vec<Vec<String>>,
}

#[derive(Deserialize)]
pub struct SkillDef {
    pub id: String,
    pub name: String,
    pub focus: String,
    pub words: Vec<String>,
    #[serde(default)]
    pub material: Vec<MaterialDef>,
    #[serde(default)]
    pub sentences: Vec<SentenceDef>,
}

#[derive(Deserialize)]
pub struct MaterialDef {
    pub lesson: u8,
    pub text: String,
}

// One authored sentence: the same thing said in both languages, with no
// direction at all. Every question the course can ask about it — tiles either
// way round, typed either way round — is made from these two sides.
//
// **Preferred and alternative are named outright** rather than being the first
// and the rest of one list, because they are used for genuinely different
// things: a preferred wording is shown (as a prompt, as the answer, as a
// bank's correct tiles) and an alternative is only ever accepted.
#[derive(Deserialize)]
pub struct SentenceDef {
    // the words this sentence exercises. Only words of its own skill — the
    // rest of the sentence is scenery, and isn't graded.
    pub words: Vec<String>,
    // how the course says it in the source language, and any other phrasings
    // it will accept. Each may use bracket groups, which are a compact way of
    // writing several wordings at once — `[Hello/Hi]!` is two.
    pub preferred_source: String,
    #[serde(default)]
    pub alternative_sources: Vec<String>,
    // and the same in the target language
    pub preferred_target: String,
    #[serde(default)]
    pub alternative_targets: Vec<String>,
}

// Build the `Course` from the definitions projected from a wiki snapshot. This
// is where content is validated and where a sentence's expansion, location and
// markup happen.
//
// The skills arrive paired with where they came from, because a complaint about
// a skill is only useful if it says which wiki page needs attention. The origin
// is just that label; assembly does not otherwise depend on MediaWiki.
pub fn assemble(
    index: CourseIndex,
    word_list: WordList,
    layout: Layout,
    skill_defs: Vec<(String, SkillDef)>,
) -> Result<Course, Box<dyn Error>> {
    let vocab = build_vocab(word_list)?;
    let (skills, rows, castles, written) = build_tree(&layout, skill_defs, &vocab)?;

    let mut sentences = Vec::new();
    for (skill, written) in skills.iter().zip(&written) {
        build_sentences(skill, written, &vocab, &mut sentences)?;
    }
    check_every_word_can_be_introduced(&skills, &sentences)?;

    Ok(Course::new(
        index.id,
        index.source_lang,
        index.target_lang,
        vocab,
        sentences,
        skills,
        rows,
        castles,
    ))
}

// --- the vocabulary ---

fn build_vocab(list: WordList) -> Result<Vocab, Box<dyn Error>> {
    let mut seen: HashSet<&str> = HashSet::new();
    for word in &list.words {
        if !seen.insert(&word.id) {
            return Err(format!("the word '{}' is defined twice", word.id).into());
        }
        // a word with no forms could never be located in a sentence, so every
        // exercise using it would silently fall back to all-or-nothing
        if word.forms.is_empty() {
            return Err(format!("the word '{}' has no forms", word.id).into());
        }
    }
    Ok(Vocab::new(
        list.words
            .into_iter()
            .map(|w| Word {
                id: w.id,
                word: w.word,
                forms: w.forms,
                glosses: w.glosses,
            })
            .collect(),
    ))
}

// --- the tree ---

// Match the converted skill definitions up with the layout, which is the only
// thing that says where a skill sits. A skill never has to know its own
// coordinates, and the layout remains the single place to read the shape of
// the course off.
// the skills, the rows they sit in, the castles that seal them, and each
// skill's sentences — kept alongside rather than on the `Skill`, because they
// are raw material for `mint` and nothing at runtime wants them again
type Tree = (
    Vec<Skill>,
    Vec<Vec<usize>>,
    Vec<Castle>,
    Vec<Vec<SentenceDef>>,
);

fn build_tree(
    layout: &Layout,
    mut defs: Vec<(String, SkillDef)>,
    vocab: &Vocab,
) -> Result<Tree, Box<dyn Error>> {
    let mut by_id: HashMap<String, SkillDef> = HashMap::new();
    for (origin, def) in defs.drain(..) {
        if by_id.contains_key(&def.id) {
            return Err(format!("{origin}: skill '{}' is defined twice", def.id).into());
        }
        by_id.insert(def.id.clone(), def);
    }

    let mut skills: Vec<Skill> = Vec::new();
    let mut sentences: Vec<Vec<SentenceDef>> = Vec::new();
    let mut rows: Vec<Vec<usize>> = Vec::new();
    let mut castles: Vec<Castle> = Vec::new();
    // every word must belong to exactly one skill, so the skills partition
    // the vocabulary — this is what makes "the words of the rows behind this
    // castle" a well-defined set
    let mut owner: HashMap<&str, String> = HashMap::new();

    for (expected, castle) in layout.castles.iter().enumerate() {
        if castle.castle != expected {
            return Err(format!(
                "the layout's castles are out of order: expected castle {expected}, found {}",
                castle.castle
            )
            .into());
        }
        let first_row = rows.len();
        for row in &castle.rows {
            if row.is_empty() {
                return Err(format!("castle {expected} has an empty row").into());
            }
            let mut in_row = Vec::new();
            for id in row {
                let def = by_id.remove(id).ok_or_else(|| {
                    format!(
                        "the layout names skill '{id}', which has no definition (or is listed twice)"
                    )
                })?;
                let (skill, written) = build_skill(def, vocab, rows.len(), expected, &mut owner)?;
                in_row.push(skills.len());
                skills.push(skill);
                sentences.push(written);
            }
            rows.push(in_row);
        }
        if rows.len() == first_row {
            return Err(format!("castle {expected} has no rows").into());
        }
        castles.push(Castle {
            castle: expected,
            rows: first_row..rows.len(),
        });
    }

    if let Some(orphan) = by_id.keys().next() {
        return Err(format!("skill '{orphan}' has a definition but no place in the layout").into());
    }
    // a word nothing teaches can never be learnt, and is almost always a
    // forgotten skill rather than a deliberate omission
    if let Some(word) = vocab
        .words()
        .iter()
        .find(|w| !owner.contains_key(w.id.as_str()))
    {
        return Err(format!("the word '{}' belongs to no skill", word.id).into());
    }
    Ok((skills, rows, castles, sentences))
}

fn build_skill<'a>(
    def: SkillDef,
    vocab: &'a Vocab,
    row: usize,
    castle: usize,
    owner: &mut HashMap<&'a str, String>,
) -> Result<(Skill, Vec<SentenceDef>), Box<dyn Error>> {
    let id = def.id;
    if def.words.is_empty() {
        return Err(format!("skill '{id}' teaches no words").into());
    }
    for word in &def.words {
        let known = vocab.get(word).ok_or_else(|| {
            format!("skill '{id}' teaches '{word}', which is not in the word list")
        })?;
        if let Some(other) = owner.insert(known.id.as_str(), id.clone()) {
            return Err(format!("'{word}' belongs to both '{other}' and '{id}'").into());
        }
    }
    for block in &def.material {
        if block.lesson == 0 || block.lesson > SKILL_LESSONS {
            return Err(format!(
                "skill '{id}' has material for lesson {}, but only {} lessons",
                block.lesson, SKILL_LESSONS
            )
            .into());
        }
    }
    let words: HashSet<&str> = def.words.iter().map(String::as_str).collect();
    for sentence in &def.sentences {
        let name = &sentence.preferred_target;
        if sentence.words.is_empty() {
            return Err(format!("skill '{id}': sentence {name:?} tags no words").into());
        }
        // a sentence tags only its own skill's words. This is what makes a
        // word's review pool knowable by counting rather than a matter of
        // luck, and what keeps generation well-posed.
        for word in &sentence.words {
            if !words.contains(word.as_str()) {
                return Err(format!(
                    "skill '{id}': sentence {name:?} tags '{word}', which is not one of its words"
                )
                .into());
            }
        }
        // A sentence with an empty side can't be asked in either direction:
        // one way there is nothing to show, the other nothing to answer.
        // Missing fields are serde's business; blank ones are ours.
        for (side, text) in [
            ("preferred_source", &sentence.preferred_source),
            ("preferred_target", name),
        ] {
            if text.trim().is_empty() {
                return Err(format!("skill '{id}': a sentence has an empty {side}").into());
            }
        }
    }
    Ok((
        Skill {
            id,
            name: def.name,
            focus: def.focus,
            words: def.words,
            material: def
                .material
                .into_iter()
                .map(|m| MaterialBlock {
                    lesson: m.lesson,
                    text: m.text,
                })
                .collect(),
            row,
            castle,
        },
        def.sentences,
    ))
}

// --- marking up sentences ---

// Turn a skill's authored sentences into `Sentence`s: expand both sides,
// locate every tagged word in every wording, and mark the spans.
//
// Both sides get the same treatment, and that symmetry is the point — the old
// format made an author write "es->en" and "en->es" as two unrelated
// sentences, each with its own alternatives, and then minted two exercises
// from each. One bidirectional sentence says the same thing once and answers
// four questions, because the objection that used to force the split
// ("Comió la naranja" -> "He ate the orange", but the reverse also admits
// "Él comió la naranja") is exactly what an *alternative* is for.
fn build_sentences(
    skill: &Skill,
    written: &[SentenceDef],
    vocab: &Vocab,
    out: &mut Vec<Sentence>,
) -> Result<(), Box<dyn Error>> {
    for (n, sentence) in written.iter().enumerate() {
        let side = |side: Side| -> Result<Wording, Box<dyn Error>> {
            let (preferred, alternatives) = match side {
                Side::Source => (&sentence.preferred_source, &sentence.alternative_sources),
                Side::Target => (&sentence.preferred_target, &sentence.alternative_targets),
            };
            wording(skill, sentence, vocab, side, preferred, alternatives)
        };
        let source = side(Side::Source)?;
        let target = side(Side::Target)?;
        let target_marks = locations(
            target.preferred().text.as_str(),
            &sentence.words,
            vocab,
            Side::Target,
        );
        out.push(Sentence {
            id: format!("{}:{}", skill.id, n + 1),
            words: sentence.words.clone(),
            row: skill.row,
            skill: skill.id.clone(),
            source,
            target,
            target_marks,
        });
    }
    Ok(())
}

// Locate tagged concepts in one finished wording and convert the matcher's
// UTF-8 byte spans to the UTF-16 offsets the browser consumes. Unlike
// `wording`, this never suppresses a solo word: these marks point out what is
// new in a prompt and have no role in partial-credit grading.
fn locations(
    text: &str,
    words: &[String],
    vocab: &Vocab,
    side: Side,
) -> Vec<crate::sentence::Mark> {
    let forms: Vec<(&str, &[String])> = words
        .iter()
        .filter_map(|word| {
            let entry = vocab.get(word)?;
            let spellings = match side {
                Side::Source => &entry.glosses,
                Side::Target => &entry.forms,
            };
            Some((word.as_str(), spellings.as_slice()))
        })
        .collect();
    let found = locate(text, &forms);
    let mut spans: Vec<(&str, Span)> = words
        .iter()
        .filter_map(|word| found.get(word.as_str()).map(|&span| (word.as_str(), span)))
        .collect();
    spans.sort_by_key(|(_, span)| span.start);
    Phrasing::of(text, &spans).words
}

// One side of one sentence, expanded and marked up.
//
// Expansion comes first and applies to the preferred wording too: `[Hello/Hi]!`
// is a compact way of writing two wordings, and the **first branch of every
// group is the preferred one**, so what an author reads off the page is what a
// learner is shown. Everything the preferred wording expands into beyond that
// first branch is an alternative like any other.
fn wording(
    skill: &Skill,
    sentence: &SentenceDef,
    vocab: &Vocab,
    side: Side,
    preferred: &str,
    alternatives: &[String],
) -> Result<Wording, Box<dyn Error>> {
    let fail = |e| format!("skill '{}': {e}", skill.id);
    let mut variants = expand(preferred).map_err(fail)?;
    for alternative in alternatives {
        variants.extend(expand(alternative).map_err(fail)?);
    }

    // **A sentence testing one word is not located at all**, and is therefore
    // graded all or nothing.
    //
    // Spans exist to divide credit, and a sentence with one tagged word has
    // nothing to divide it between: the whole sentence is that word's
    // question, and everything else in it is scenery the learner still had to
    // produce. Marking the word anyway means "Juan girl" scores `nina` right
    // for "The girl." — a right word inside a wrong sentence, reported to the
    // ladder as a clean success. Partial credit is for answers with something
    // to be partial about.
    //
    // With nothing marked, the client falls back to the exercise's overall
    // verdict for every word it was told to grade, which is exactly the rule
    // we want; nothing had to be added at either end to say so. It is also the
    // cheaper path, and the common one — most sentences test one word.
    let located: Vec<HashMap<&str, Span>> = if sentence.words.len() < 2 {
        variants.iter().map(|_| HashMap::new()).collect()
    } else {
        // Which side we are marking up decides which list of spellings to
        // match against: the target side is the learner's target language, so
        // it uses `forms`; the source side uses `glosses`, which is the same
        // list for the word's translation.
        //
        // Only the tagged words are looked for, and that set is also the scope
        // ambiguity is judged in: `locate` drops a form two of *these* words
        // share, and leaves alone the ones they merely share with the rest of
        // the course.
        let forms: Vec<(&str, &[String])> = sentence
            .words
            .iter()
            .filter_map(|word| {
                let entry = vocab.get(word)?;
                let list = match side {
                    Side::Source => &entry.glosses,
                    Side::Target => &entry.forms,
                };
                Some((word.as_str(), list.as_slice()))
            })
            .collect();
        variants.iter().map(|v| locate(v, &forms)).collect()
    };

    // A word is marked in every wording of a side or in none of them, so that
    // grading can't depend on which one the learner happened to hit. A word
    // the sentence uses in a form its list doesn't cover — or in one another
    // tagged word claims too, or in a sentence that tests it alone — is marked
    // nowhere, and the client falls back to the overall verdict for it: the
    // all-or-nothing case, which costs precision and never correctness.
    //
    // The two sides are judged separately: a word may well be markable in the
    // Spanish and not in the English, and there is no reason to lose the half
    // that works.
    let marked: Vec<&str> = sentence
        .words
        .iter()
        .map(String::as_str)
        .filter(|word| located.iter().all(|found| found.contains_key(word)))
        .collect();
    let mut phrasings = variants.iter().zip(&located).map(|(text, found)| {
        let mut spans: Vec<(&str, Span)> = marked.iter().map(|&word| (word, found[word])).collect();
        spans.sort_by_key(|(_, span)| span.start);
        Phrasing::of(text, &spans)
    });
    // `expand` always yields at least one variant, so the preferred one is
    // always there
    let preferred = phrasings.next().unwrap_or_else(|| Phrasing::of("", &[]));
    Ok(Wording::new(preferred, phrasings.collect()))
}

// A word no suitable sentence uses can never be learnt: it has no first
// contact, so the lesson planner has nothing to introduce it with, and it
// would sit in its skill's word list forever. Suitable means every tagged word
// has a dedicated target-prompt span. That is stricter than review, which may
// fall back to whole-answer grading, because the new-word UI must point at
// every word it announces and must not derive that location from a gloss.
//
// **One sentence is enough — it does not have to use the word on its own.** It
// used to have to: an introduction was a sentence grading nothing else, and a
// word without one was rejected here. But a sentence tags every word of its
// skill it actually uses (see `convert::tag_words_used`), and demanding a solo
// one would mean either refusing courses whose author writes whole phrases, or
// tagging a sentence with one word while the learner is graded on three.
// `Course::introduction_for` takes the gentlest sentence there is instead, and
// a first contact that introduces two words at once introduces both.
fn check_every_word_can_be_introduced(
    skills: &[Skill],
    sentences: &[Sentence],
) -> Result<(), Box<dyn Error>> {
    let used: HashSet<&str> = sentences
        .iter()
        .filter(|sentence| sentence.can_introduce())
        .flat_map(|s| s.words.iter().map(String::as_str))
        .collect();
    for skill in skills {
        for word in &skill.words {
            if !used.contains(word.as_str()) {
                return Err(format!(
                    "skill '{}' teaches '{word}', but no sentence both uses it and \
                     locates every new word in its target prompt, so nothing can introduce it",
                    skill.id
                )
                .into());
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::course::tests::{sentence, skill};

    // Review may still use an unlocatable form and fall back to whole-answer
    // grading, but a first contact has a stronger UI contract: if the lesson
    // calls a word new, it must also say exactly where that word is in the
    // prompt. A sentence without that presentation span therefore cannot be
    // the only route into memory.
    #[test]
    fn a_word_without_a_target_prompt_span_cannot_be_introduced() {
        let skills = vec![skill("intro", 0, 0, &["hola"], 1)];
        let mut sentences = vec![sentence("intro:1", &["hola"], 0)];
        sentences[0].target_marks.clear();

        let error = check_every_word_can_be_introduced(&skills, &sentences)
            .unwrap_err()
            .to_string();
        assert!(error.contains("locates every new word"), "{error}");
    }

    #[test]
    fn a_located_target_prompt_is_a_valid_first_contact() {
        let skills = vec![skill("intro", 0, 0, &["hola"], 1)];
        let sentences = vec![sentence("intro:1", &["hola"], 0)];
        assert!(check_every_word_can_be_introduced(&skills, &sentences).is_ok());
    }
}
