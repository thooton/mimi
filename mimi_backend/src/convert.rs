// Turn a wiki snapshot into course definitions.
//
// A pure function of the snapshot: no network, no clock, no filesystem. That is
// deliberate, and it is what makes an incremental poll trustworthy — the course
// built after patching three pages is the course a full refetch would have
// produced, because both run this same transform over the same cache.
//
// The two formats disagree in ways worth naming, because most of this file is
// those disagreements:
//
// * **The wiki names things; the course identifies them.** A course is
//   `Course:Spanish for English speakers` and a skill is a page title. The
//   loader wants `spanish`, `es`/`en`, and `family_and_people`. Everything is
//   slugged here, once.
// * **A wiki sentence belongs to one word; a course sentence tags a list.** An
//   author files a sentence under the one word it illustrates, but the sentence
//   they wrote usually *uses* other words of the same skill — that is what a
//   skill is, a batch taught together — so it is tagged with every one of them
//   it actually contains (see `tag_words_used`), and nothing is held back to
//   keep a sentence down to one word.
// * **Forms and glosses live in the glossary, not the skill.** A skill says
//   which words it teaches; the glossary says how each is spelt and what it
//   means.
// * **The loader parses brackets.** `[Hello/Hi]` is two wordings to it, and the
//   wiki has no such convention, so a sentence containing a literal bracket
//   cannot be represented and is dropped rather than silently reinterpreted.
//
// Everything `loader::assemble` rejects, this refuses to emit: a word with no
// forms, a word in two skills, a skill teaching nothing, a word with nothing to
// introduce it, an empty row, a castle with no rows. Where the wiki holds work
// in progress — a red-linked skill, a word nobody has written a sentence for —
// the page is left out and a warning is reported, because half-written content
// is the normal state of a wiki and not an error.
//
// # The glossary is read twice, for two different jobs
//
// It is the one page that says how a word is spelt and what it means, and the
// course needs that fact in two shapes:
//
// * grouped **lemma -> forms**, it is the *vocabulary*: the concepts a skill
//   teaches, each with the spellings that let `sentence::locate` find it in a
//   sentence and grade it word by word;
// * flattened **form -> translations**, it is the *dictionary*: what any word
//   of a sentence means when a learner taps it, including scenery the course
//   never teaches.
//
// So a lemma no skill teaches is not an error and not waste — it glosses. Only
// the taught lemmas become vocabulary, which is what keeps `assemble`'s rule
// that every word belong to exactly one skill satisfiable while the glossary
// grows to cover the whole language.

use std::collections::{BTreeMap, HashMap, HashSet};

use serde_json::Value;
use unicode_normalization::UnicodeNormalization;
use unicode_normalization::char::canonical_combining_class;

use crate::loader::{
    CastleDef, CourseIndex, Layout, MaterialDef, SentenceDef, SkillDef, WordDef, WordList,
};
use crate::sentence::locate;
use crate::skill::SKILL_LESSONS;
use crate::snapshot::{Snapshot, course_of};

// Language names as a course title writes them, to the codes the course uses.
// The wiki deliberately stores no code — a course page is named after its
// language pair — so the mapping has to live somewhere, and a table that can be
// overridden beats guessing from the first two letters.
const LANGUAGE_CODES: [(&str, &str); 32] = [
    ("arabic", "ar"),
    ("bengali", "bn"),
    ("chinese", "zh"),
    ("czech", "cs"),
    ("danish", "da"),
    ("dutch", "nl"),
    ("english", "en"),
    ("esperanto", "eo"),
    ("finnish", "fi"),
    ("french", "fr"),
    ("german", "de"),
    ("greek", "el"),
    ("hebrew", "he"),
    ("hindi", "hi"),
    ("hungarian", "hu"),
    ("indonesian", "id"),
    ("irish", "ga"),
    ("italian", "it"),
    ("japanese", "ja"),
    ("korean", "ko"),
    ("latin", "la"),
    ("norwegian", "no"),
    ("polish", "pl"),
    ("portuguese", "pt"),
    ("romanian", "ro"),
    ("russian", "ru"),
    ("spanish", "es"),
    ("swedish", "sv"),
    ("turkish", "tr"),
    ("ukrainian", "uk"),
    ("vietnamese", "vi"),
    ("welsh", "cy"),
];

// One course, as the definitions `loader::assemble` takes, plus the flattened
// glossary the dictionary is built from.
pub struct Converted {
    pub id: String,
    pub index: CourseIndex,
    pub words: WordList,
    pub layout: Layout,
    pub skills: Vec<(String, SkillDef)>,
    // form -> what it means, for glossing any word of a sentence
    pub glossary: Vec<(String, Vec<String>)>,
}

pub struct Conversion {
    pub courses: Vec<Converted>,
    pub warnings: Vec<String>,
}

// An ascii identifier: `Family and people` -> `family_and_people`.
//
// Accents are folded rather than dropped, so `adiós` becomes `adios` and not
// `adis`. Ids are only ever generated, never authored — but they are also the
// keys a learner's progress is stored under, so stability across a rebuild is
// the property that matters most.
pub fn slug(text: &str) -> String {
    let folded: String = text
        .nfkd()
        .filter(|c| canonical_combining_class(*c) == 0)
        .collect();
    let mut out = String::with_capacity(folded.len());
    let mut pending = false;
    for c in folded.to_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            if pending && !out.is_empty() {
                out.push('_');
            }
            pending = false;
            out.push(c);
        } else {
            pending = true;
        }
    }
    if out.is_empty() { "x".to_string() } else { out }
}

// `identifier`, or the first numbered variant of it that is free.
fn unique(identifier: &str, taken: &mut HashSet<String>) -> String {
    if taken.insert(identifier.to_string()) {
        return identifier.to_string();
    }
    let mut n = 2;
    while !taken.insert(format!("{identifier}_{n}")) {
        n += 1;
    }
    format!("{identifier}_{n}")
}

// The pair a course name states, as `CourseName::languages` reads it.
//
// Split at the *last* " for ", because a language whose name contains one
// ("Middle English for English speakers" is fine, but consider a hypothetical
// "X for Y for Z speakers") must not be cut in the wrong place — the same thing
// the wiki's own greedy `^(.+) for (.+) speakers$` does.
fn languages(course_name: &str) -> Option<(String, String)> {
    let rest = course_name.strip_suffix(" speakers")?;
    let (target, source) = rest.rsplit_once(" for ")?;
    let (target, source) = (target.trim(), source.trim());
    if target.is_empty() || source.is_empty() {
        return None;
    }
    Some((target.to_string(), source.to_string()))
}

fn has_brackets(text: &str) -> bool {
    text.contains('[') || text.contains(']')
}

// A list of non-empty strings, whatever the field actually holds.
fn clean(values: Option<&Value>) -> Vec<String> {
    values
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

// A trimmed string field, or "" if it is absent or not a string.
fn field(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string()
}

fn deduped(values: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    values
        .into_iter()
        .filter(|v| seen.insert(v.clone()))
        .collect()
}

pub fn convert(snapshot: &Snapshot, language_codes: &HashMap<String, String>) -> Conversion {
    let mut codes: HashMap<String, String> = LANGUAGE_CODES
        .iter()
        .map(|(name, code)| ((*name).to_string(), (*code).to_string()))
        .collect();
    codes.extend(
        language_codes
            .iter()
            .map(|(name, code)| (name.to_lowercase(), code.clone())),
    );

    let mut warnings = Vec::new();
    let mut courses = Vec::new();
    let mut course_ids = HashSet::new();

    let mut titles: Vec<&String> = snapshot
        .pages
        .keys()
        .filter(|title| title.starts_with("Course:"))
        .collect();
    titles.sort();

    for title in titles {
        if let Some(course) =
            convert_course(snapshot, title, &codes, &mut course_ids, &mut warnings)
        {
            courses.push(course);
        }
    }
    Conversion { courses, warnings }
}

fn convert_course(
    snapshot: &Snapshot,
    title: &str,
    codes: &HashMap<String, String>,
    course_ids: &mut HashSet<String>,
    warnings: &mut Vec<String>,
) -> Option<Converted> {
    let name = course_of(title);
    let Some((target_name, source_name)) = languages(&name) else {
        warnings.push(format!(
            "{title}: not named '<target> for <source> speakers', so its languages are \
             unknown; skipped"
        ));
        return None;
    };
    let target_lang = codes.get(&target_name.to_lowercase());
    let source_lang = codes.get(&source_name.to_lowercase());
    let missing: Vec<&str> = [(&target_name, target_lang), (&source_name, source_lang)]
        .iter()
        .filter(|(_, code)| code.is_none())
        .map(|(n, _)| n.as_str())
        .collect();
    if !missing.is_empty() {
        warnings.push(format!(
            "{title}: no language code known for {}; skipped",
            missing.join(" and ")
        ));
        return None;
    }
    let (target_lang, source_lang) = (target_lang?.clone(), source_lang?.clone());

    let glossary = glossary_index(snapshot, &name);
    let layout = &snapshot.pages.get(title)?.content;

    // --- the skills, in the order the rows place them ---

    let mut skill_ids: HashSet<String> = HashSet::new();
    let mut word_ids: HashSet<String> = HashSet::new();
    // The course needs the vocabulary to be partitioned: a word belongs to
    // exactly one skill. The wiki only enforces uniqueness within a skill, so
    // the first skill to claim a word (in row order) keeps it.
    let mut claimed: HashMap<String, String> = HashMap::new();
    let mut words: Vec<WordDef> = Vec::new();
    let mut skills: Vec<(String, SkillDef)> = Vec::new();
    let mut kept_rows: Vec<(usize, Vec<String>)> = Vec::new();

    let rows = layout.get("rows").and_then(Value::as_array);
    for (row_index, row) in rows.into_iter().flatten().enumerate() {
        let mut placed: Vec<String> = Vec::new();
        let listed = row.as_array().map(Vec::as_slice).unwrap_or(&[]);
        for skill_title in listed.iter().filter_map(Value::as_str) {
            let Some(page) = snapshot.pages.get(skill_title) else {
                warnings.push(format!(
                    "{title}: row {} places {skill_title}, which does not exist; left out",
                    row_index + 1
                ));
                continue;
            };
            let Some((name, mut def)) = convert_skill(
                snapshot,
                skill_title,
                &page.content,
                &glossary,
                &mut claimed,
                &mut word_ids,
                &mut words,
                warnings,
            ) else {
                continue;
            };
            let skill_id = unique(&slug(&name), &mut skill_ids);
            def.id = skill_id.clone();
            placed.push(skill_id);
            skills.push((skill_title.to_string(), def));
        }
        if !placed.is_empty() {
            kept_rows.push((row_index, placed));
        } else if !listed.is_empty() {
            warnings.push(format!(
                "{title}: row {} has nothing left to place; left out",
                row_index + 1
            ));
        }
    }

    if kept_rows.is_empty() {
        warnings.push(format!("{title}: no usable skills; skipped"));
        return None;
    }

    // --- the castles ---

    // A wiki castle is a boundary — `afterRow: 2` seals everything up to and
    // including the second row. A course castle owns the rows it seals. Rows
    // that were dropped above must not shift the boundaries, so the grouping is
    // done on each row's original position.
    let mut boundaries: Vec<i64> = layout
        .get("castles")
        .and_then(Value::as_array)
        .map(|castles| {
            castles
                .iter()
                .filter_map(|c| c.get("afterRow").and_then(Value::as_i64))
                .collect()
        })
        .unwrap_or_default();
    boundaries.sort_unstable();

    let mut grouped: BTreeMap<usize, Vec<Vec<String>>> = BTreeMap::new();
    for (original_index, placed) in kept_rows {
        let group = boundaries
            .iter()
            .filter(|b| **b <= original_index as i64)
            .count();
        grouped.entry(group).or_default().push(placed);
    }
    // Renumbered consecutively: `assemble` requires castle n to be the nth, and
    // a boundary whose rows all disappeared would otherwise leave a hole.
    let castles: Vec<CastleDef> = grouped
        .into_values()
        .enumerate()
        .map(|(castle, rows)| CastleDef { castle, rows })
        .collect();

    let course_id = unique(&slug(&target_name), course_ids);
    Some(Converted {
        id: course_id.clone(),
        index: CourseIndex {
            id: course_id,
            source_lang,
            target_lang,
        },
        words: WordList { words },
        layout: Layout { castles },
        skills,
        glossary: flatten_glossary(snapshot, &name),
    })
}

#[allow(clippy::too_many_arguments)]
fn convert_skill(
    snapshot: &Snapshot,
    skill_title: &str,
    content: &Value,
    glossary: &HashMap<String, &Value>,
    claimed: &mut HashMap<String, String>,
    word_ids: &mut HashSet<String>,
    words: &mut Vec<WordDef>,
    warnings: &mut Vec<String>,
) -> Option<(String, SkillDef)> {
    let name = skill_title
        .split_once('/')
        .map(|(_, rest)| rest.trim().to_string())
        .unwrap_or_else(|| course_of(skill_title));

    // The words this skill actually exports, each with its spellings. The forms
    // are kept because `tag_words_used` needs the whole skill's vocabulary at
    // once, and it cannot have it until this loop has decided which words the
    // skill exports at all.
    let mut taught: Vec<(String, Vec<String>)> = Vec::new();
    let mut sentences: Vec<SentenceDef> = Vec::new();

    let entries = content.get("words").and_then(Value::as_array);
    for entry in entries.into_iter().flatten() {
        let word = field(entry, "word");
        if word.is_empty() {
            continue;
        }
        let key = word.to_lowercase();
        if let Some(owner) = claimed.get(&key) {
            warnings.push(format!(
                "{skill_title}: '{word}' is already taught by '{owner}'; a word may only \
                 belong to one skill, so it is left out here"
            ));
            continue;
        }

        let written = sentences_of(entry, skill_title, &word, warnings);
        if written.is_empty() {
            // Nothing could introduce this word, and `assemble` rejects that
            // outright, so it cannot be exported however complete the rest is.
            warnings.push(format!(
                "{skill_title}: '{word}' has no usable sentence, so nothing could introduce \
                 it; left out"
            ));
            continue;
        }

        let word_id = unique(&slug(&word), word_ids);
        claimed.insert(key, name.clone());

        let definition = word_entry(&word_id, &word, glossary);
        if definition.glosses.is_empty() {
            // Not fatal — `assemble` only insists on `forms` — but a word with
            // no glosses cannot be found in the source-language side of its own
            // sentences, so every question asking it that way is graded
            // all-or-nothing instead of word by word.
            warnings.push(format!(
                "{skill_title}: '{word}' is not in the glossary, so it has no glosses and \
                 cannot be graded word by word in the source language"
            ));
        }
        taught.push((word_id.clone(), definition.forms.clone()));
        words.push(definition);

        for sentence in written {
            sentences.push(SentenceDef {
                words: vec![word_id.clone()],
                preferred_source: sentence.translation,
                alternative_sources: sentence.alternative_sources,
                preferred_target: sentence.text,
                alternative_targets: sentence.alternative_targets,
            });
        }
    }

    if taught.is_empty() {
        warnings.push(format!("{skill_title}: teaches nothing usable; left out"));
        return None;
    }

    tag_words_used(&mut sentences, &taught);

    Some((
        name.clone(),
        SkillDef {
            // filled in by the caller, which owns the pool ids are unique in
            id: String::new(),
            name,
            focus: field(content, "grammarFocus"),
            words: taught.into_iter().map(|(id, _)| id).collect(),
            material: material(snapshot, skill_title, warnings),
            sentences,
        },
    ))
}

// Tag each sentence with the other words of its own skill that it uses.
//
// An author writes a sentence under one word, so that is the word the wiki says
// it is *for* — but "Yo como pan" filed under `comer` exercises `pan` just as
// much, and a skill is precisely a batch of words taught together, so its
// sentences reuse each other's vocabulary constantly. Tagging only the filed-
// under word means the learner is graded on `pan` and the scheduler never hears
// about it: a review that happened and left no trace, which is exactly the input
// FSRS needs and the one thing it cannot infer.
//
// **Only the skill's own words are looked for.** Everything else in the sentence
// is scenery, `loader::assemble` rejects a sentence tagging a word from another
// skill, and a word tagged before its skill is reached would be graded by a
// learner who has never met it.
//
// The matcher is `sentence::locate`, the same one the loader will use to mark
// the spans, rather than a plain form -> word map. That buys three things a map
// would have to reinvent: word boundaries (`es` is not inside `estás`), forms
// that are several words long (`buenos días`), and — because the search set here
// is the whole skill — a form two of its words share is dropped rather than
// guessed at, since crediting the wrong card is worse than crediting neither. A
// tag it agrees with is also a tag the loader can actually mark, so the sentence
// grades word by word instead of falling back to all-or-nothing.
//
// **Nothing is held back to keep a sentence solo.** A word whose every sentence
// uses another of the skill's words is introduced by one of them, several words
// at a time (see `Course::introduction_for`); tagging one word and staying quiet
// about the other would be the transform lying about what the learner is being
// asked, which costs the scheduler exactly the evidence this function exists to
// give it.
fn tag_words_used(sentences: &mut [SentenceDef], taught: &[(String, Vec<String>)]) {
    let vocabulary: Vec<(&str, &[String])> = taught
        .iter()
        .map(|(id, forms)| (id.as_str(), forms.as_slice()))
        .collect();

    for sentence in sentences {
        // The word it was filed under is already tagged, and is searched for
        // anyway so that it can win a span another word also claims.
        let own = sentence.words[0].as_str();
        let mut found: Vec<String> = locate(&sentence.preferred_target, &vocabulary)
            .into_keys()
            .filter(|word| *word != own)
            .map(str::to_string)
            .collect();
        // `locate` answers in a HashMap, and this transform must be a pure
        // function of the snapshot rather than of an iteration order.
        found.sort();
        sentence.words.extend(found);
    }
}

struct Written {
    text: String,
    translation: String,
    alternative_targets: Vec<String>,
    alternative_sources: Vec<String>,
}

// The sentences of one word that can be exported at all.
//
// A wiki sentence is the target language with its translation beneath; the
// course calls those `preferred_target` and `preferred_source`.
fn sentences_of(
    entry: &Value,
    skill_title: &str,
    word: &str,
    warnings: &mut Vec<String>,
) -> Vec<Written> {
    let mut written = Vec::new();
    let sentences = entry.get("sentences").and_then(Value::as_array);
    for sentence in sentences.into_iter().flatten() {
        if sentence
            .get("disabled")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            continue;
        }
        let text = field(sentence, "text");
        let translation = field(sentence, "translation");
        if text.is_empty() || translation.is_empty() {
            // The read view holds a half-written sentence back too: it is draft
            // material, not teaching material.
            continue;
        }
        let alternative_targets = clean(sentence.get("alternativeSentences"));
        let alternative_sources = clean(sentence.get("alternativeTranslations"));
        let bracketed = [&text, &translation]
            .into_iter()
            .chain(&alternative_targets)
            .chain(&alternative_sources)
            .any(|t| has_brackets(t));
        if bracketed {
            warnings.push(format!(
                "{skill_title}: the sentence {text:?} for '{word}' contains a bracket, which \
                 the course reads as a group of alternatives and the wiki has no way to \
                 escape; left out"
            ));
            continue;
        }
        written.push(Written {
            text,
            translation,
            alternative_targets,
            alternative_sources,
        });
    }
    written
}

// One entry of the vocabulary, spellings and all.
//
// `forms` is every target-language spelling and `glosses` the first `GLOSSES`
// source-side ones; grading word by word works by finding them in a sentence.
// Forms are not pruned for ambiguity on purpose — `sentence::locate` drops one
// only where two words of the *same sentence* both claim it.
fn word_entry(word_id: &str, word: &str, glossary: &HashMap<String, &Value>) -> WordDef {
    let mut forms = vec![word.to_string()];
    let mut glosses: Vec<String> = Vec::new();
    if let Some(entry) = glossary.get(&word.to_lowercase()) {
        for form in entry
            .get("forms")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let spelling = field(form, "form");
            if !spelling.is_empty() {
                forms.push(spelling);
            }
            glosses.extend(clean(form.get("translations")));
        }
    }
    let mut glosses = deduped(glosses);
    glosses.truncate(GLOSSES);
    WordDef {
        id: word_id.to_string(),
        word: word.to_string(),
        // A word with no forms could never be located in a sentence, and
        // `assemble` refuses one, so the word itself is always the first form.
        forms: deduped(forms),
        glosses,
    }
}

// A skill's tips, as the material blocks its lessons show.
//
// Tips are the same page name in the `Tips:` namespace — that is the whole of
// the link between a skill and them.
fn material(
    snapshot: &Snapshot,
    skill_title: &str,
    warnings: &mut Vec<String>,
) -> Vec<MaterialDef> {
    let Some((_, rest)) = skill_title.split_once(':') else {
        return Vec::new();
    };
    let Some(page) = snapshot.pages.get(&format!("Tips:{rest}")) else {
        return Vec::new();
    };

    let mut material = Vec::new();
    let tips = page.content.get("tips").and_then(Value::as_array);
    for tip in tips.into_iter().flatten() {
        let title = field(tip, "title");
        let Some(lesson) = tip.get("lesson").and_then(Value::as_i64) else {
            // On the wiki a tip with no lesson waits behind the skill's tips
            // button and is shown in no lesson. The course has nowhere to put
            // that, and inventing a lesson for it would put it in front of a
            // learner its author kept it away from.
            warnings.push(format!(
                "{}: the tip {title:?} is pinned to no lesson, which the course cannot \
                 express; left out",
                page.title
            ));
            continue;
        };
        if lesson < 1 || lesson > SKILL_LESSONS as i64 {
            warnings.push(format!(
                "{}: the tip {title:?} is pinned to lesson {lesson}, but a skill has {}; \
                 left out",
                page.title, SKILL_LESSONS
            ));
            continue;
        }
        let body = field(tip, "body");
        material.push(MaterialDef {
            lesson: lesson as u8,
            text: if body.is_empty() {
                format!("## {title}")
            } else {
                format!("## {title}\n\n{body}")
            },
        });
    }
    material
}

// How many meanings of a word the course takes from the glossary.
//
// A wiki glossary is written for a reader with time: `caballero` may carry a
// dozen shades of gentleman, and an imported one usually does. A learner
// tapping a word in a sentence wants to know what it means, not to read a
// dictionary column, and a word bank distractor drawn from the twelfth shade
// teaches nothing. So the first three are taken and the rest left on the wiki,
// where they belong — the glossary is not pruned, only what the course carries
// away from it.
//
// The cost is that `sentence::locate` can no longer find a word in a source
// wording that used the fourth meaning and only that one. The glossary lists
// its meanings best first, so that is a rare sentence, and an author who hits
// it can move the meaning up.
pub const GLOSSES: usize = 3;

// Every entry of a course's glossary, in the order the wiki files them.
//
// A glossary outgrows a page long before a language runs out of words, so the
// wiki lets one spread over `Glossary:<course>/<letter>` subpages of the page it
// is filed under. The whole glossary is that page *together with* its segments,
// and this is the only place that knows it: read it once here and both the
// vocabulary and the dictionary see one glossary, whether it is written on one
// page or twenty-six.
//
// Pages are visited in title order so that the result is a function of the
// snapshot alone — a `HashMap` would let two lemmas spelt the same win by turns
// between rebuilds, which is exactly the kind of drift a pure transform is for.
fn glossary_entries<'a>(snapshot: &'a Snapshot, course_name: &str) -> Vec<&'a Value> {
    let root = format!("Glossary:{course_name}");
    let prefix = format!("{root}/");
    let mut titles: Vec<&String> = snapshot
        .pages
        .keys()
        .filter(|title| **title == root || title.starts_with(&prefix))
        .collect();
    titles.sort();
    titles
        .into_iter()
        .filter_map(|title| snapshot.pages.get(title))
        .filter_map(|page| page.content.get("entries").and_then(Value::as_array))
        .flatten()
        .collect()
}

// The course's glossary, keyed by lemma for looking a word up.
fn glossary_index<'a>(snapshot: &'a Snapshot, course_name: &str) -> HashMap<String, &'a Value> {
    let mut index = HashMap::new();
    for entry in glossary_entries(snapshot, course_name) {
        let lemma = field(entry, "lemma");
        if !lemma.is_empty() {
            index.insert(lemma.to_lowercase(), entry);
        }
    }
    index
}

// The glossary flattened to `form -> what it means`: the dictionary a learner
// taps a word of a sentence to see.
//
// **Every entry, not just the taught ones.** A sentence is mostly scenery the
// course never teaches, and scenery is exactly what a learner needs looked up.
//
// Keys are lowercased to match `Dictionary::annotate`, which lowercases what it
// looks up. A form carrying no translations of its own falls back to its
// lemma's, so a spelling recorded without a meaning still resolves to the word
// it belongs to rather than to nothing. If that spelling is also a lemma, its
// lemma entry owns the gloss completely: definitions inherited from another
// lemma's form would otherwise mix a word's dictionary meaning with an
// unrelated inflectional reading. Each keeps its first `GLOSSES` meanings — a
// tap is answered in a line, not in a column.
fn flatten_glossary(snapshot: &Snapshot, course_name: &str) -> Vec<(String, Vec<String>)> {
    let entries = glossary_entries(snapshot, course_name);
    // Resolve precedence from the complete glossary before flattening it, so
    // the result does not depend on whether the lemma or the colliding form is
    // encountered first (including when they live on different segments).
    let lemmas: HashSet<String> = entries
        .iter()
        .map(|entry| field(entry, "lemma").to_lowercase())
        .filter(|lemma| !lemma.is_empty())
        .collect();
    let mut flat: Vec<(String, Vec<String>)> = Vec::new();
    for entry in entries {
        let lemma = field(entry, "lemma");
        if lemma.is_empty() {
            continue;
        }
        let forms: Vec<&Value> = entry
            .get("forms")
            .and_then(Value::as_array)
            .map(|f| f.iter().collect())
            .unwrap_or_default();
        let mut all: Vec<String> = deduped(
            forms
                .iter()
                .flat_map(|form| clean(form.get("translations")))
                .collect(),
        );
        all.truncate(GLOSSES);
        flat.push((lemma.to_lowercase(), all.clone()));
        for form in forms {
            let spelling = field(form, "form");
            if spelling.is_empty() {
                continue;
            }
            let spelling = spelling.to_lowercase();
            if lemmas.contains(&spelling) {
                continue;
            }
            let mut own = deduped(clean(form.get("translations")));
            own.truncate(GLOSSES);
            flat.push((spelling, if own.is_empty() { all.clone() } else { own }));
        }
    }
    flat
}

// The rules being checked here are `loader::assemble`'s: it refuses a word with
// no forms, a word in two skills, a skill teaching nothing, a word nothing can
// introduce, an empty row and a castle with no rows. A snapshot that produced
// any of those would only fail later, when the poll tried to swap the course
// in, so the transform has to be the thing that guarantees it.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::wiki::Revision;
    use serde_json::json;

    const COURSE: &str = "Course:Spanish for English speakers";

    fn page(title: &str, content: Value) -> Revision {
        Revision {
            title: title.to_string(),
            revid: 1,
            timestamp: "2026-01-01T00:00:00Z".to_string(),
            model: String::new(),
            content,
        }
    }

    fn prefixed(name: &str) -> String {
        format!("Skill:Spanish for English speakers/{name}")
    }

    fn sentence(text: &str, translation: &str) -> Value {
        json!({"text": text, "translation": translation, "disabled": false})
    }

    fn skill_page(name: &str, words: Vec<(&str, Vec<Value>)>) -> Revision {
        let words: Vec<Value> = words
            .into_iter()
            .map(|(word, sentences)| json!({"word": word, "sentences": sentences}))
            .collect();
        page(
            &prefixed(name),
            json!({"schemaVersion": 5, "grammarFocus": "A focus.", "words": words}),
        )
    }

    struct Wiki {
        pages: Vec<Revision>,
        rows: Vec<Vec<String>>,
        castles: Vec<Value>,
        glossary: Option<Vec<Value>>,
    }

    impl Wiki {
        fn new(pages: Vec<Revision>, rows: Vec<Vec<&str>>) -> Self {
            Wiki {
                pages,
                rows: rows
                    .into_iter()
                    .map(|row| row.into_iter().map(prefixed).collect())
                    .collect(),
                castles: Vec::new(),
                glossary: None,
            }
        }
        fn castles(mut self, castles: Vec<Value>) -> Self {
            self.castles = castles;
            self
        }
        fn glossary(mut self, entries: Vec<Value>) -> Self {
            self.glossary = Some(entries);
            self
        }
        fn build(self) -> Snapshot {
            let skills: Vec<&String> = self.rows.iter().flatten().collect();
            let mut pages = vec![page(
                COURSE,
                json!({"schemaVersion": 5, "skills": skills,
                       "rows": self.rows, "castles": self.castles}),
            )];
            pages.extend(self.pages);
            if let Some(entries) = self.glossary {
                pages.push(page(
                    "Glossary:Spanish for English speakers",
                    json!({"schemaVersion": 3, "entries": entries}),
                ));
            }
            Snapshot {
                timestamp: "2026-01-01T00:00:00Z".to_string(),
                pages: pages.into_iter().map(|p| (p.title.clone(), p)).collect(),
            }
        }
    }

    fn convert_of(snapshot: &Snapshot) -> Conversion {
        convert(snapshot, &HashMap::new())
    }

    fn only(conversion: &Conversion) -> &Converted {
        assert_eq!(conversion.courses.len(), 1, "expected exactly one course");
        &conversion.courses[0]
    }

    fn skill<'a>(course: &'a Converted, id: &str) -> &'a SkillDef {
        course
            .skills
            .iter()
            .map(|(_, def)| def)
            .find(|def| def.id == id)
            .unwrap_or_else(|| panic!("no skill '{id}'"))
    }

    fn warned(conversion: &Conversion, needle: &str) -> bool {
        conversion.warnings.iter().any(|w| w.contains(needle))
    }

    fn rows_of(course: &Converted) -> Vec<Vec<Vec<String>>> {
        course
            .layout
            .castles
            .iter()
            .map(|c| c.rows.clone())
            .collect()
    }

    fn one_skill(name: &str, word: &str, text: &str, translation: &str) -> Wiki {
        Wiki::new(
            vec![skill_page(
                name,
                vec![(word, vec![sentence(text, translation)])],
            )],
            vec![vec![name]],
        )
    }

    // --- naming ---

    // `adiós` must not become `adis`: an id is generated, so the only thing
    // that matters is that it is stable and readable.
    #[test]
    fn accents_fold_rather_than_vanish() {
        assert_eq!(slug("adiós"), "adios");
        assert_eq!(slug("Family and people"), "family_and_people");
        assert_eq!(slug("¡Hola!"), "hola");
        assert_eq!(slug("  spaced  out  "), "spaced_out");
        // nothing usable at all still has to yield an identifier
        assert_eq!(slug("日本語"), "x");
    }

    #[test]
    fn a_course_takes_its_languages_from_its_name() {
        let conversion = convert_of(&one_skill("A", "hola", "Hola.", "Hello.").build());
        let course = only(&conversion);
        assert_eq!(course.index.source_lang, "en");
        assert_eq!(course.index.target_lang, "es");
        assert_eq!(course.id, "spanish");
    }

    #[test]
    fn an_unknown_language_is_reported_not_guessed() {
        let mut snapshot = one_skill("A", "x", "X.", "X.").build();
        snapshot.pages.insert(
            "Course:Klingon for English speakers".to_string(),
            page(
                "Course:Klingon for English speakers",
                json!({"schemaVersion": 5, "skills": [], "rows": [], "castles": []}),
            ),
        );
        assert!(warned(&convert_of(&snapshot), "no language code known"));
    }

    #[test]
    fn a_language_code_can_be_supplied() {
        let snapshot = Snapshot {
            timestamp: "t".to_string(),
            pages: [
                page(
                    "Course:Klingon for English speakers",
                    json!({"schemaVersion": 5, "rows": [["Skill:Klingon for English speakers/A"]],
                           "castles": []}),
                ),
                page(
                    "Skill:Klingon for English speakers/A",
                    json!({"schemaVersion": 5, "grammarFocus": "f", "words": [
                        {"word": "nuqneH", "sentences": [sentence("nuqneH.", "Hello.")]}]}),
                ),
            ]
            .into_iter()
            .map(|p| (p.title.clone(), p))
            .collect(),
        };
        let codes = HashMap::from([("Klingon".to_string(), "tlh".to_string())]);
        let conversion = convert(&snapshot, &codes);
        assert_eq!(only(&conversion).index.target_lang, "tlh");
    }

    // --- what the loader refuses ---

    // Words the author did write apart stay apart: a sentence tags several
    // words only where it really uses several (see the tagging tests below), so
    // the ordinary skill still introduces one word at a time.
    #[test]
    fn a_word_the_rest_of_the_skill_never_uses_keeps_a_sentence_to_itself() {
        let wiki = Wiki::new(
            vec![skill_page(
                "A",
                vec![
                    ("hola", vec![sentence("Hola.", "Hello.")]),
                    ("adiós", vec![sentence("Adiós.", "Bye.")]),
                ],
            )],
            vec![vec!["A"]],
        );
        let conversion = convert_of(&wiki.build());
        let a = skill(only(&conversion), "a");
        let introduced: HashSet<&str> = a
            .sentences
            .iter()
            .filter(|s| s.words.len() == 1)
            .map(|s| s.words[0].as_str())
            .collect();
        assert_eq!(introduced, a.words.iter().map(String::as_str).collect());
    }

    #[test]
    fn a_word_with_no_usable_sentence_is_left_out() {
        let wiki = Wiki::new(
            vec![skill_page(
                "A",
                vec![
                    ("hola", vec![sentence("Hola.", "Hello.")]),
                    ("adiós", vec![]),
                ],
            )],
            vec![vec!["A"]],
        );
        let conversion = convert_of(&wiki.build());
        assert_eq!(skill(only(&conversion), "a").words, ["hola"]);
        assert!(warned(&conversion, "nothing could introduce it"));
    }

    #[test]
    fn a_disabled_or_half_written_sentence_is_not_teaching_material() {
        let wiki = Wiki::new(
            vec![skill_page(
                "A",
                vec![(
                    "hola",
                    vec![
                        sentence("Hola.", "Hello."),
                        json!({"text": "Off.", "translation": "Off.", "disabled": true}),
                        sentence("Half.", ""),
                    ],
                )],
            )],
            vec![vec!["A"]],
        );
        let conversion = convert_of(&wiki.build());
        let texts: Vec<&str> = skill(only(&conversion), "a")
            .sentences
            .iter()
            .map(|s| s.preferred_target.as_str())
            .collect();
        assert_eq!(texts, ["Hola."]);
    }

    // The wiki only enforces uniqueness within a skill; the course needs the
    // vocabulary partitioned, so the first skill to claim it keeps it.
    #[test]
    fn a_word_belongs_to_exactly_one_skill() {
        let wiki = Wiki::new(
            vec![
                skill_page("A", vec![("hola", vec![sentence("Hola.", "Hello.")])]),
                skill_page(
                    "B",
                    vec![
                        ("hola", vec![sentence("Hola again.", "Hello again.")]),
                        ("adiós", vec![sentence("Adiós.", "Bye.")]),
                    ],
                ),
            ],
            vec![vec!["A"], vec!["B"]],
        );
        let conversion = convert_of(&wiki.build());
        let course = only(&conversion);
        assert_eq!(skill(course, "a").words, ["hola"]);
        assert_eq!(skill(course, "b").words, ["adios"]);
        let ids: Vec<&str> = course.words.words.iter().map(|w| w.id.as_str()).collect();
        assert_eq!(ids, ["hola", "adios"]);
        assert!(warned(&conversion, "may only belong to one skill"));
    }

    #[test]
    fn a_skill_that_teaches_nothing_is_left_out() {
        let wiki = Wiki::new(
            vec![
                skill_page("A", vec![("hola", vec![sentence("Hola.", "Hello.")])]),
                skill_page("Empty", vec![("adiós", vec![])]),
            ],
            vec![vec!["A"], vec!["Empty"]],
        );
        let conversion = convert_of(&wiki.build());
        let course = only(&conversion);
        assert!(!course.skills.iter().any(|(_, def)| def.id == "empty"));
        // Its row went with it rather than being kept empty.
        assert_eq!(rows_of(course), [[["a"]]]);
    }

    #[test]
    fn a_red_linked_skill_is_left_out() {
        let wiki = Wiki::new(
            vec![skill_page(
                "A",
                vec![("hola", vec![sentence("Hola.", "Hello.")])],
            )],
            vec![vec!["A", "Missing"]],
        );
        let conversion = convert_of(&wiki.build());
        assert_eq!(rows_of(only(&conversion)), [[["a"]]]);
        assert!(warned(&conversion, "does not exist"));
    }

    #[test]
    fn every_word_has_at_least_one_form() {
        let conversion = convert_of(&one_skill("A", "hola", "Hola.", "Hello.").build());
        for word in &only(&conversion).words.words {
            assert!(!word.forms.is_empty(), "{} has no forms", word.id);
        }
    }

    // --- brackets ---

    // The loader reads `[a/b]` as a group of alternatives and the wiki has no
    // way to escape one, so such a sentence cannot be represented.
    #[test]
    fn a_literal_bracket_is_dropped_rather_than_reinterpreted() {
        let wiki = Wiki::new(
            vec![skill_page(
                "A",
                vec![(
                    "hola",
                    vec![
                        sentence("Hola.", "Hello."),
                        sentence("Hola [amigo].", "Hello [friend]."),
                    ],
                )],
            )],
            vec![vec!["A"]],
        );
        let conversion = convert_of(&wiki.build());
        let texts: Vec<&str> = skill(only(&conversion), "a")
            .sentences
            .iter()
            .map(|s| s.preferred_target.as_str())
            .collect();
        assert_eq!(texts, ["Hola."]);
        assert!(warned(&conversion, "bracket"));
    }

    #[test]
    fn a_bracket_in_an_alternative_disqualifies_the_sentence_too() {
        let wiki = Wiki::new(
            vec![skill_page(
                "A",
                vec![(
                    "hola",
                    vec![
                        sentence("Hola.", "Hello."),
                        json!({"text": "Buenas.", "translation": "Hi.", "disabled": false,
                               "alternativeTranslations": ["Hi [there]"]}),
                    ],
                )],
            )],
            vec![vec!["A"]],
        );
        let conversion = convert_of(&wiki.build());
        let texts: Vec<&str> = skill(only(&conversion), "a")
            .sentences
            .iter()
            .map(|s| s.preferred_target.as_str())
            .collect();
        assert_eq!(texts, ["Hola."]);
    }

    // --- castles ---

    fn abc() -> Vec<Revision> {
        ["A", "B", "C"]
            .iter()
            .map(|n| {
                let word = n.to_lowercase();
                skill_page(
                    n,
                    vec![(&word, vec![sentence(&format!("{n}."), &format!("{n}."))])],
                )
            })
            .collect()
    }

    // `afterRow: 1` seals the first row; everything later belongs to the castle
    // after it.
    #[test]
    fn a_boundary_splits_the_rows_it_seals() {
        let wiki = Wiki::new(abc(), vec![vec!["A"], vec!["B"], vec!["C"]])
            .castles(vec![json!({"afterRow": 1})]);
        let conversion = convert_of(&wiki.build());
        assert_eq!(
            rows_of(only(&conversion)),
            [vec![vec!["a"]], vec![vec!["b"], vec!["c"]]]
        );
    }

    #[test]
    fn no_boundary_means_one_castle_over_everything() {
        let wiki = Wiki::new(
            abc().into_iter().take(2).collect(),
            vec![vec!["A"], vec!["B"]],
        );
        let conversion = convert_of(&wiki.build());
        assert_eq!(rows_of(only(&conversion)), [[["a"], ["b"]]]);
    }

    // B's row disappears, which would leave castle 1 with no rows; the loader
    // requires castles numbered 0..n with rows in each.
    #[test]
    fn castles_are_renumbered_when_a_stretch_empties() {
        let wiki = Wiki::new(
            vec![
                skill_page("A", vec![("a", vec![sentence("A.", "A.")])]),
                skill_page("B", vec![("b", vec![])]),
                skill_page("C", vec![("c", vec![sentence("C.", "C.")])]),
            ],
            vec![vec!["A"], vec!["B"], vec!["C"]],
        )
        .castles(vec![json!({"afterRow": 1}), json!({"afterRow": 2})]);
        let conversion = convert_of(&wiki.build());
        let course = only(&conversion);
        let numbers: Vec<usize> = course.layout.castles.iter().map(|c| c.castle).collect();
        assert_eq!(numbers, [0, 1]);
        assert_eq!(rows_of(course), [[["a"]], [["c"]]]);
        for castle in &course.layout.castles {
            assert!(!castle.rows.is_empty());
            assert!(castle.rows.iter().all(|row| !row.is_empty()));
        }
    }

    // --- tagging the words a sentence uses ---

    // The words one sentence of a skill grades, own word first.
    fn tags<'a>(skill: &'a SkillDef, target: &str) -> &'a [String] {
        skill
            .sentences
            .iter()
            .find(|s| s.preferred_target == target)
            .unwrap_or_else(|| panic!("no sentence {target:?}"))
            .words
            .as_slice()
    }

    // The point of the whole exercise: an author files "Como pan." under
    // `comer`, but it exercises `pan` just as much, and a verdict on `pan` that
    // reaches no card is a review FSRS never hears about.
    #[test]
    fn a_sentence_is_tagged_with_every_word_of_its_skill_it_uses() {
        let wiki = Wiki::new(
            vec![skill_page(
                "A",
                vec![
                    (
                        "comer",
                        vec![
                            sentence("Como pan.", "I eat bread."),
                            sentence("Comer.", "To eat."),
                        ],
                    ),
                    ("pan", vec![sentence("El pan.", "The bread.")]),
                ],
            )],
            vec![vec!["A"]],
        )
        .glossary(vec![json!({
            "lemma": "comer",
            "forms": [
                {"form": "", "translations": ["to eat"]},
                {"form": "como", "translations": ["I eat"]},
            ]
        })]);
        let conversion = convert_of(&wiki.build());
        let a = skill(only(&conversion), "a");
        // found by an inflected form, not just the dictionary one
        assert_eq!(tags(a, "Como pan."), ["comer", "pan"]);
        // and a sentence that uses nothing else still tags exactly its own word
        assert_eq!(tags(a, "El pan."), ["pan"]);
    }

    // Only the skill's own words: `assemble` rejects a sentence tagging a word
    // from elsewhere, and a word tagged before its own skill is reached would be
    // graded on a learner who has never met it.
    #[test]
    fn a_word_from_another_skill_stays_scenery() {
        let wiki = Wiki::new(
            vec![
                skill_page("A", vec![("pan", vec![sentence("El pan.", "The bread.")])]),
                skill_page(
                    "B",
                    vec![("comer", vec![sentence("Comer pan.", "To eat bread.")])],
                ),
            ],
            vec![vec!["A"], vec!["B"]],
        );
        let conversion = convert_of(&wiki.build());
        assert_eq!(tags(skill(only(&conversion), "b"), "Comer pan."), ["comer"]);
    }

    // A form two of the skill's words share is dropped rather than guessed at:
    // crediting the wrong card half the time is worse than crediting neither.
    #[test]
    fn a_form_two_of_the_skills_words_share_is_not_tagged() {
        let wiki = Wiki::new(
            vec![skill_page(
                "A",
                vec![
                    ("ser", vec![sentence("Soy alto.", "I am tall.")]),
                    ("estar", vec![sentence("Está aquí.", "It is here.")]),
                ],
            )],
            vec![vec!["A"]],
        )
        .glossary(vec![
            json!({"lemma": "ser", "forms": [{"form": "soy", "translations": ["am"]}]}),
            json!({"lemma": "estar", "forms": [
                {"form": "soy", "translations": ["am"]},
                {"form": "está", "translations": ["is"]},
            ]}),
        ]);
        let conversion = convert_of(&wiki.build());
        // "soy" is claimed by both, so "Soy alto." tags only the word it was
        // filed under — `estar` is not guessed onto it
        assert_eq!(tags(skill(only(&conversion), "a"), "Soy alto."), ["ser"]);
    }

    // Nothing is held back to keep a sentence down to one word. A skill whose
    // author never writes a word by itself gets sentences tagged with
    // everything they use, and `Course::introduction_for` meets those words
    // together rather than the transform pretending they are met apart.
    #[test]
    fn a_word_never_written_alone_is_still_tagged_everywhere_it_appears() {
        let wiki = Wiki::new(
            vec![skill_page(
                "A",
                vec![
                    (
                        "comer",
                        vec![
                            sentence("Comer pan y agua.", "To eat bread and water."),
                            sentence("Comer pan.", "To eat bread."),
                        ],
                    ),
                    ("pan", vec![sentence("Comer pan.", "To eat bread.")]),
                    ("agua", vec![sentence("Comer agua.", "To drink water.")]),
                ],
            )],
            vec![vec!["A"]],
        );
        let conversion = convert_of(&wiki.build());
        let a = skill(only(&conversion), "a");
        // no sentence here uses one word alone, and every one says so
        assert_eq!(tags(a, "Comer pan y agua."), ["comer", "agua", "pan"]);
        assert_eq!(tags(a, "Comer agua."), ["agua", "comer"]);
        assert!(a.sentences.iter().all(|s| s.words.len() > 1));
        // and the loader takes it: a solo sentence is no longer required
        let course = conversion.courses.into_iter().next().unwrap();
        crate::loader::assemble(course.index, course.words, course.layout, course.skills)
            .expect("a course that never uses a word alone is still a course");
    }

    // End to end, because tagging is only worth anything if the loader can then
    // mark both words: a tag it cannot locate grades all-or-nothing.
    #[test]
    fn a_tagged_word_is_marked_up_by_the_loader() {
        let wiki = Wiki::new(
            vec![skill_page(
                "A",
                vec![
                    (
                        "comer",
                        vec![
                            sentence("Como pan.", "I eat bread."),
                            sentence("Comer.", "To eat."),
                        ],
                    ),
                    ("pan", vec![sentence("El pan.", "The bread.")]),
                ],
            )],
            vec![vec!["A"]],
        )
        .glossary(vec![
            json!({"lemma": "comer", "forms": [
                {"form": "", "translations": ["to eat"]},
                {"form": "como", "translations": ["eat"]},
            ]}),
            json!({"lemma": "pan", "forms": [{"form": "", "translations": ["bread"]}]}),
        ]);
        let conversion = convert_of(&wiki.build());
        let course = conversion.courses.into_iter().next().unwrap();
        let assembled =
            crate::loader::assemble(course.index, course.words, course.layout, course.skills)
                .expect("the transform must not emit anything the loader refuses");
        let sentence = assembled
            .sentences
            .iter()
            .find(|s| s.words.len() == 2)
            .expect("the two-word sentence survived assembly");
        assert_eq!(sentence.words, ["comer", "pan"]);
        let spanish = sentence.target.preferred();
        let marked: Vec<&str> = spanish.words.iter().map(|m| m.word.as_str()).collect();
        assert_eq!(marked, ["comer", "pan"]);
        // and each span really is the stretch of text that word owns
        let at = |mark: &crate::sentence::Mark| {
            spanish
                .text
                .chars()
                .skip(mark.start)
                .take(mark.end - mark.start)
                .collect::<String>()
        };
        assert_eq!(at(&spanish.words[0]), "Como");
        assert_eq!(at(&spanish.words[1]), "pan");
    }

    // The whole path in one test, because every step of it is somebody's
    // assumption about the next: a wiki page, converted, assembled, asked as a
    // question, and serialized into the bytes a browser receives. The sentence
    // is accented on purpose — the client indexes in UTF-16 code units and the
    // matcher works in UTF-8 bytes, so "niña" is where an off-by-one would
    // first show itself.
    //
    // It also pins the two grading regimes side by side: the two-word sentence
    // carries spans so its words can be scored apart, and the one-word one
    // carries none, because a sentence testing one word is all or nothing.
    #[test]
    fn a_served_answer_carries_spans_only_where_credit_can_be_divided() {
        let wiki = Wiki::new(
            vec![skill_page(
                "A",
                vec![
                    (
                        "niña",
                        vec![sentence("La niña y la mujer.", "The girl and the woman.")],
                    ),
                    ("mujer", vec![sentence("La mujer.", "The woman.")]),
                ],
            )],
            vec![vec!["A"]],
        )
        .glossary(vec![
            json!({"lemma": "niña", "forms": [{"form": "", "translations": ["girl"]}]}),
            json!({"lemma": "mujer", "forms": [{"form": "", "translations": ["woman"]}]}),
        ]);
        let conversion = convert_of(&wiki.build());
        let converted = conversion.courses.into_iter().next().unwrap();
        let course = crate::loader::assemble(
            converted.index,
            converted.words,
            converted.layout,
            converted.skills,
        )
        .unwrap();

        let dictionary = crate::dictionary::Dictionary::from_entries(Vec::new());
        let served = |index: usize| {
            let task = crate::lesson::Task::Exercise {
                sentence: index,
                ask: crate::exercise::Ask::WriteTarget, // answered in Spanish
                introduces: Vec::new(),
            };
            serde_json::to_value(crate::api::TaskView::of(&course, &dictionary, &task).unwrap())
                .unwrap()
        };

        let both = served(0);
        assert_eq!(both.pointer("/task/words"), Some(&json!(["nina", "mujer"])));
        assert_eq!(
            both.pointer("/task/answers/0"),
            Some(&json!({
                "text": "La niña y la mujer.",
                // "niña" is bytes 3..8 and code units 3..7; the client is told
                // the second pair, because `text.slice(3, 7)` is what it runs
                "words": [
                    {"word": "nina", "start": 3, "end": 7},
                    {"word": "mujer", "start": 13, "end": 18},
                ],
            }))
        );
        // shown the side it does not ask for
        assert_eq!(
            both.pointer("/task/prompt"),
            Some(&json!("The girl and the woman."))
        );

        // ...and the sentence that tests `mujer` alone divides nothing
        let alone = served(1);
        assert_eq!(alone.pointer("/task/words"), Some(&json!(["mujer"])));
        assert_eq!(
            alone.pointer("/task/answers/0"),
            Some(&json!({"text": "La mujer.", "words": []}))
        );
    }

    // --- the glossary and tips ---

    #[test]
    fn forms_and_glosses_come_from_the_glossary() {
        let wiki = one_skill("A", "comer", "Yo como.", "I eat.").glossary(vec![json!({
            "lemma": "comer",
            "forms": [
                {"form": "", "translations": ["to eat"]},
                {"form": "como", "translations": ["eat"]},
            ]
        })]);
        let conversion = convert_of(&wiki.build());
        let word = &only(&conversion).words.words[0];
        assert_eq!(word.forms, ["comer", "como"]);
        assert_eq!(word.glosses, ["to eat", "eat"]);
    }

    // A glossary too large for one page is spread over subpages of it, and the
    // course is not supposed to be able to tell: the vocabulary a skill teaches
    // and the dictionary a learner taps both read the segments as one glossary.
    #[test]
    fn a_glossary_spread_over_segments_reads_as_one() {
        let mut snapshot = one_skill("A", "comer", "Yo como pan.", "I eat bread.")
            .glossary(vec![])
            .build();
        snapshot.pages.insert(
            "Glossary:Spanish for English speakers/C".to_string(),
            page(
                "Glossary:Spanish for English speakers/C",
                json!({"schemaVersion": 3, "entries": [
                    {"lemma": "comer", "forms": [
                        {"form": "", "translations": ["to eat"]},
                        {"form": "como", "translations": ["I eat"]},
                    ]},
                ]}),
            ),
        );
        snapshot.pages.insert(
            "Glossary:Spanish for English speakers/P".to_string(),
            page(
                "Glossary:Spanish for English speakers/P",
                json!({"schemaVersion": 3, "entries": [
                    {"lemma": "pan", "forms": [{"form": "", "translations": ["bread"]}]},
                ]}),
            ),
        );
        let conversion = convert_of(&snapshot);
        let course = only(&conversion);
        let word = &course.words.words[0];
        assert_eq!(word.forms, ["comer", "como"]);
        assert_eq!(word.glosses, ["to eat", "I eat"]);
        // The segment nothing teaches from still glosses, as any entry does.
        let flat: HashMap<&str, &Vec<String>> = course
            .glossary
            .iter()
            .map(|(form, meanings)| (form.as_str(), meanings))
            .collect();
        assert_eq!(flat["pan"], &["bread".to_string()]);
        assert!(!warned(&conversion, "not in the glossary"));
    }

    #[test]
    fn a_word_outside_the_glossary_still_exports_with_a_warning() {
        let wiki = one_skill("A", "hola", "Hola.", "Hello.").glossary(vec![]);
        let conversion = convert_of(&wiki.build());
        let word = &only(&conversion).words.words[0];
        assert_eq!(word.forms, ["hola"]);
        assert!(word.glosses.is_empty());
        assert!(warned(&conversion, "not in the glossary"));
    }

    #[test]
    fn tips_become_the_material_of_the_lesson_they_are_pinned_to() {
        let mut snapshot = one_skill("A", "hola", "Hola.", "Hello.").build();
        snapshot.pages.insert(
            "Tips:Spanish for English speakers/A".to_string(),
            page(
                "Tips:Spanish for English speakers/A",
                json!({"schemaVersion": 1, "tips": [
                    {"title": "Greeting", "body": "Use **hola**.", "lesson": 2},
                    {"title": "Unpinned", "body": "Behind the button."},
                    {"title": "Too late", "body": "x", "lesson": 99},
                ]}),
            ),
        );
        let conversion = convert_of(&snapshot);
        let material = &skill(only(&conversion), "a").material;
        assert_eq!(material.len(), 1);
        assert_eq!(material[0].lesson, 2);
        assert_eq!(material[0].text, "## Greeting\n\nUse **hola**.");
        assert!(warned(&conversion, "pinned to no lesson"));
        assert!(warned(&conversion, "lesson 99"));
    }

    // --- the glossary's other projection: the dictionary ---

    // Every entry is flattened, taught or not, because a sentence is mostly
    // scenery and scenery is what a learner taps.
    #[test]
    fn the_dictionary_covers_words_no_skill_teaches() {
        let wiki = one_skill("A", "hola", "Hola amigo.", "Hello friend.").glossary(vec![
            json!({"lemma": "hola", "forms": [{"form": "", "translations": ["hello"]}]}),
            json!({"lemma": "amigo", "forms": [{"form": "", "translations": ["friend"]}]}),
        ]);
        let conversion = convert_of(&wiki.build());
        let course = only(&conversion);
        let flat: HashMap<&str, &Vec<String>> = course
            .glossary
            .iter()
            .map(|(form, meanings)| (form.as_str(), meanings))
            .collect();
        assert_eq!(flat["amigo"], &["friend".to_string()]);
        // ... while only the taught lemma became vocabulary
        let taught: Vec<&str> = course.words.words.iter().map(|w| w.id.as_str()).collect();
        assert_eq!(taught, ["hola"]);
    }

    #[test]
    fn a_form_without_its_own_meaning_falls_back_to_its_lemmas() {
        let wiki = one_skill("A", "adiós", "Adiós.", "Bye.").glossary(vec![json!({
            "lemma": "adiós",
            "forms": [
                {"form": "", "translations": ["goodbye", "bye"]},
                {"form": "Adioses", "translations": []},
            ]
        })]);
        let conversion = convert_of(&wiki.build());
        let flat: HashMap<&str, &Vec<String>> = only(&conversion)
            .glossary
            .iter()
            .map(|(form, meanings)| (form.as_str(), meanings))
            .collect();
        assert_eq!(flat["adiós"], &["goodbye".to_string(), "bye".to_string()]);
        // lowercased, because `Dictionary::annotate` lowercases what it looks up
        assert_eq!(flat["adioses"], &["goodbye".to_string(), "bye".to_string()]);
    }

    // A surface spelling can be an inflection of one word and a dictionary
    // word in its own right. Tapping it should explain the latter, not merge
    // both just because the flat dictionary encountered both projections.
    #[test]
    fn a_lemma_takes_precedence_over_an_identically_spelled_form() {
        let wiki = one_skill("A", "como", "Como quieras.", "As you wish.").glossary(vec![
            json!({
                "lemma": "comer",
                "forms": [{"form": "Como", "translations": ["I eat"]}]
            }),
            json!({
                "lemma": "como",
                "forms": [{"form": "", "translations": ["as"]}]
            }),
        ]);
        let conversion = convert_of(&wiki.build());
        let entries: Vec<&Vec<String>> = only(&conversion)
            .glossary
            .iter()
            .filter(|(form, _)| form == "como")
            .map(|(_, meanings)| meanings)
            .collect();

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0], &["as".to_string()]);
        let dictionary =
            crate::dictionary::Dictionary::from_entries(only(&conversion).glossary.clone());
        assert_eq!(dictionary.annotate("Como")[0].meanings, ["As"]);
    }

    // The whole point of the transform: what it produces must survive the
    // loader's validation, since that is what runs at poll time.
    #[test]
    fn the_result_assembles_into_a_course() {
        let wiki = Wiki::new(
            vec![
                skill_page("A", vec![("hola", vec![sentence("Hola.", "Hello.")])]),
                skill_page("B", vec![("adiós", vec![sentence("Adiós.", "Bye.")])]),
            ],
            vec![vec!["A"], vec!["B"]],
        )
        .castles(vec![json!({"afterRow": 1})])
        .glossary(vec![json!({
            "lemma": "hola", "forms": [{"form": "", "translations": ["hello"]}]
        })]);
        let conversion = convert_of(&wiki.build());
        let course = conversion.courses.into_iter().next().unwrap();
        let assembled =
            crate::loader::assemble(course.index, course.words, course.layout, course.skills)
                .expect("the transform must not emit anything the loader refuses");
        assert_eq!(assembled.skills().len(), 2);
        assert_eq!(assembled.vocab.words().len(), 2);
        assert_eq!(assembled.castles().len(), 2);
    }
}
