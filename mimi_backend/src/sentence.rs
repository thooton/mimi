// A sentence: the same thing said in both of the course's languages, and the
// two string operations that turn what an author wrote into something a client
// can grade.
//
// **A sentence has no direction.** It is a pair of sides, each one preferred
// wording plus any number of alternatives, and it is the *question* that points
// one way through it (see `Ask`). The same "¡Hola! / Hello!" is a word bank
// either way round, a recognition drill, and a production drill; which of those
// a learner meets is decided per learner, per lesson, by the ladder in word.rs.
//
// The course data has no hand-written concept markers. An author writes plain
// sentences and tags them with the words they exercise; everything the grader
// needs — which span of a wording belongs to which word — is worked out here,
// once, at load time:
//
//   1. **expand** each authored wording's brackets into its variants,
//   2. **locate** each tagged word in each variant, by its forms,
//   3. **mark** the located spans, so the client can grade word by word.
//
// Expansion runs first, because the forms of a word differ between the
// variants and the matcher must never see an unexpanded string.

use std::collections::HashMap;

use serde::Serialize;

use crate::exercise::{Ask, Exercise, Side, tile};

// A sentence that expands into more variants than this is almost certainly a
// mistake in the course data rather than an author being thorough.
const MAX_EXPANSIONS: usize = 64;

// --- the sentence ---

// Where one word sits in a finished phrasing: the pair that ties the surface
// text "Hola" back to the word `hola`, which is what makes per-word grading
// possible.
//
// **The offsets are UTF-16 code units**, not bytes and not characters, because
// the only thing that ever reads them is a browser and `text.slice(start, end)`
// in JavaScript means exactly this pair. `Span` below is the other convention —
// byte offsets into UTF-8, which is what Rust string indexing and `locate`
// speak — and `Phrasing::of` is the one place the two meet. Getting this wrong
// is silent and off by one per accent, so it is worth the paragraph: "¡Hola" is
// 6 bytes and 5 code units, and every Spanish sentence in the course starts
// with a character where the two disagree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Mark {
    pub word: String,
    pub start: usize,
    pub end: usize,
}

// One accepted phrasing, ready to serve: the text itself, and where each word
// it grades sits in it.
//
// This is the shape that goes on the wire, and it is deliberately not a
// string with the markers written into it. That format ("¡[hola=Hola], Juan!")
// meant the server rendered a little language on every request and the client
// parsed it straight back out again — twice the code, one shared escaping
// problem, and an authored '[' that could never be represented. Locating is a
// property of the sentence, so it is settled once at load and then only
// cloned.
#[derive(Debug, Clone, Serialize)]
pub struct Phrasing {
    // what the learner should produce, exactly as it is shown
    pub text: String,
    // Sorted by `start` and non-overlapping, which is what `locate`
    // guarantees. A word the phrasing uses in a form its list doesn't cover is
    // absent, and so is *every* word of a sentence that tests only one — see
    // `loader::wording`. The client falls back to the exercise's overall
    // verdict for anything not here, which is what makes an empty list mean
    // "graded all or nothing" rather than "graded on nothing".
    pub words: Vec<Mark>,
}

impl Phrasing {
    // A phrasing from its text and the byte spans `locate` found in it,
    // converting those spans to the code units the client indexes with.
    //
    // The spans must be sorted by start and must not overlap.
    pub fn of(text: &str, spans: &[(&str, Span)]) -> Phrasing {
        Phrasing {
            text: text.to_string(),
            words: spans
                .iter()
                .map(|&(word, span)| {
                    let start = utf16_len(&text[..span.start]);
                    Mark {
                        word: word.to_string(),
                        start,
                        end: start + utf16_len(&text[span.start..span.end]),
                    }
                })
                .collect(),
        }
    }

    pub fn grades(&self, word: &str) -> bool {
        self.words.iter().any(|mark| mark.word == word)
    }
}

// How many UTF-16 code units a string occupies — a JavaScript `.length`.
fn utf16_len(text: &str) -> usize {
    text.chars().map(char::len_utf16).sum()
}

// One side of a sentence: how to say it in one of the two languages, once
// well and then however else the author will accept.
//
// The distinction is not decoration. **A prompt shows the preferred wording
// and nothing else** — a question has to ask one thing — and it is what a
// client displays as "the" answer and what a word bank's correct tiles are cut
// from. The alternatives exist only to be *accepted*: they are never shown,
// never prompted with, and never turned into tiles. Which is why the author
// says outright which is which, rather than the first line of a list quietly
// meaning something different from the rest.
// The fields are private because `tiles` is *derived* from `preferred`, and
// the two drifting apart would hand a client a board that cannot spell the
// answer it is being graded against. `new` is the only way to build one.
pub struct Wording {
    // located and ready to grade
    preferred: Phrasing,
    // also located; may be empty
    alternatives: Vec<Phrasing>,
    // The preferred phrasing cut into a word bank's correct tiles, cut here
    // because it is a property of the sentence: nothing about it changes per
    // learner or per request, and a lesson would otherwise re-cut it every
    // time it served the question. Only the preferred phrasing is ever cut up
    // — the alternatives are accepted, never shown — which is why this sits on
    // the side rather than on `Phrasing`.
    tiles: Vec<String>,
}

impl Wording {
    pub fn new(preferred: Phrasing, alternatives: Vec<Phrasing>) -> Wording {
        let tiles = preferred.text.split_whitespace().filter_map(tile).collect();
        Wording {
            preferred,
            alternatives,
            tiles,
        }
    }

    pub fn preferred(&self) -> &Phrasing {
        &self.preferred
    }

    pub fn tiles(&self) -> &[String] {
        &self.tiles
    }

    // every phrasing this side accepts, preferred first — an exercise's
    // `answers` when this is the side being asked for
    pub fn accepted(&self) -> Vec<Phrasing> {
        std::iter::once(&self.preferred)
            .chain(&self.alternatives)
            .cloned()
            .collect()
    }
}

// One authored sentence, ready to be asked any of four ways. Both sides are
// already expanded and marked up.
pub struct Sentence {
    // where it came from: "greetings:1" is the first sentence of the
    // greetings skill. An exercise's id extends this with the way it is
    // being asked.
    pub id: String,
    // the words this sentence exercises — no duplicates, and only words of
    // its own skill. Anything else it contains is scenery, and isn't graded.
    pub words: Vec<String>,
    // which row of the tree its skill sits in. The course's only ordinal.
    pub row: usize,
    // the skill it was written for
    pub skill: String,
    // how to say it in the language the learner already has...
    pub source: Wording,
    // ...and in the one they are learning
    pub target: Wording,
    // Where every tagged word that can be found in the preferred target
    // wording sits. These are **presentation spans for first contacts**, not
    // grading spans: unlike answer marks they are retained for a one-word
    // sentence, because the client still has to colour the new word in the
    // prompt even when the answer is graded all or nothing. They are also not
    // dictionary glosses; a word needs this location whether or not the
    // glossary has anything useful to say about it.
    pub(crate) target_marks: Vec<Mark>,
}

impl Sentence {
    pub fn side(&self, side: Side) -> &Wording {
        match side {
            Side::Source => &self.source,
            Side::Target => &self.target,
        }
    }

    // The prompt locations for exactly the words this task introduces. None
    // means the content cannot uphold the lesson API's promise that every
    // announced new word has a span. Assembly prevents that for wiki-built
    // courses; keeping the result explicit also protects hand-built fixtures
    // and any future constructor.
    pub fn new_word_marks(&self, words: &[String]) -> Option<Vec<Mark>> {
        let mut marks = Vec::with_capacity(words.len());
        for word in words {
            marks.push(
                self.target_marks
                    .iter()
                    .find(|mark| mark.word == *word)?
                    .clone(),
            );
        }
        marks.sort_by_key(|mark| mark.start);
        Some(marks)
    }

    // A sentence may be a first contact only when *all* words it grades can
    // be pointed out in its target prompt. The learner may already know some
    // of them, but for a fresh learner every one is announced as new.
    pub fn can_introduce(&self) -> bool {
        self.words.len() == self.target_marks.len()
            && self
                .words
                .iter()
                .all(|word| self.target_marks.iter().any(|mark| mark.word == *word))
    }

    // The exercise this sentence becomes when asked this way — everything but
    // a word bank's wrong tiles, which are drawn from the course as a whole
    // (see `Course::exercise`, which is what callers should use).
    //
    // Preferred wording in, everything accepted out: the prompt is the shown
    // side's preferred text, and the answers are every phrasing the produced
    // side accepts, grading marks and all. Prompt presentation marks stay on
    // the sentence for the enclosing task to select only when it introduces
    // words; they are not part of an ordinary `Exercise`.
    pub fn ask(&self, ask: Ask) -> Exercise {
        let produced = self.side(ask.produces());
        Exercise {
            id: format!("{}:{}", self.id, ask.tag()),
            ask,
            prompt: self.side(ask.shows()).preferred().text.clone(),
            words: self.words.clone(),
            row: self.row,
            skill: self.skill.clone(),
            answers: produced.accepted(),
            tiles: produced.tiles().to_vec(),
            bank: Vec::new(),
        }
    }
}

// --- bracket expansion ---

// Every wording a bracketed one stands for, in order: the first is preferred
// and the rest are merely accepted. Both sides of a sentence may use brackets
// — a side is a list of wordings, and a bracket group is the compact way to
// write several at once.
//
// A group is `[a/b]`, and the alternatives are separated by `/`. A branch may
// be empty, which is how an optional ending is written: `la naranja[/s]`
// gives `la naranja` and `la naranjas`. Several groups take the cartesian
// product, first branches first, so the canonical sentence is the one an
// author reads off the page.
//
// A group with no `/` at all is rejected rather than guessed at: `[s]` is
// ambiguous between "always s" and "optionally s", and the fix (`[/s]`) is
// one character away.
pub fn expand(text: &str) -> Result<Vec<String>, String> {
    let mut out = vec![String::new()];
    let mut rest = text;
    loop {
        let Some(open) = rest.find('[') else {
            for variant in &mut out {
                variant.push_str(rest);
            }
            return Ok(out);
        };
        let head = &rest[..open];
        let after = &rest[open + 1..];
        let Some(close) = after.find(']') else {
            return Err(format!("unclosed '[' in {text:?}"));
        };
        let body = &after[..close];
        if body.contains('[') {
            return Err(format!("nested brackets in {text:?}"));
        }
        if !body.contains('/') {
            return Err(format!(
                "bracket group [{body}] in {text:?} has no alternatives \
                 (write [/{body}] if it is optional)"
            ));
        }
        let branches: Vec<&str> = body.split('/').collect();
        if out.len() * branches.len() > MAX_EXPANSIONS {
            return Err(format!(
                "{text:?} expands into more than {MAX_EXPANSIONS} sentences"
            ));
        }
        let mut next = Vec::with_capacity(out.len() * branches.len());
        for prefix in &out {
            for branch in &branches {
                let mut variant = prefix.clone();
                variant.push_str(head);
                variant.push_str(branch);
                next.push(variant);
            }
        }
        out = next;
        rest = &after[close + 1..];
    }
}

// --- locating a word in a sentence ---

// Where one word sits in one sentence, as byte offsets into it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    fn len(self) -> usize {
        self.end - self.start
    }

    fn overlaps(self, other: Span) -> bool {
        self.start < other.end && other.start < self.end
    }
}

// Which span of `text` demonstrates each of `words`, where `words` pairs a
// word id with the forms to look for.
//
// Four rules decide it, and all four matter:
//
//   - **Word boundaries, not substrings.** `es` must not match inside
//     `estás`. Boundaries are Unicode-aware, because an accented letter is a
//     letter.
//   - **A form two of these words share is dropped from both** before any of
//     them is looked for — `words` is the whole scope in which a form can be
//     ambiguous, and inside it a guess would be wrong half the time. See
//     `disambiguate`.
//   - **Longest match first, then leftmost.** `del` beats `de`. Two words can
//     still offer overlapping candidates of different lengths, and which one
//     wins has to be a rule rather than an accident of iteration order.
//   - **Spans never overlap.** Once a stretch of text is spoken for, no other
//     word may claim it.
//
// A word with no span in the result is one the sentence uses in a form its
// list doesn't cover, or in one another tagged word claims too. That is not an
// error: the caller simply doesn't mark it, and the client falls back to the
// exercise's overall verdict for it.
pub fn locate<'a>(text: &str, words: &[(&'a str, &[String])]) -> HashMap<&'a str, Span> {
    // every (word, span) the text offers at all...
    let usable = disambiguate(words);
    let mut candidates: Vec<(&str, Span)> = Vec::new();
    for (id, forms) in &usable {
        for span in occurrences(text, forms) {
            candidates.push((*id, span));
        }
    }
    // ...longest first, then leftmost, then by id so that two equally good
    // candidates always resolve the same way
    candidates.sort_by(|(a_id, a), (b_id, b)| {
        b.len()
            .cmp(&a.len())
            .then(a.start.cmp(&b.start))
            .then(a_id.cmp(b_id))
    });
    let mut taken: HashMap<&str, Span> = HashMap::new();
    for (id, span) in candidates {
        if taken.contains_key(id) || taken.values().any(|&other| span.overlaps(other)) {
            continue;
        }
        taken.insert(id, span);
    }
    taken
}

// The forms each word may actually be matched by *here*: its own list, less
// every form some other word of this set also claims.
//
// Ambiguity is a property of the set being searched, not of the language. A
// sentence is only ever searched for the handful of words it tags, so a form
// two words share matters exactly when both of them are in that handful — and
// then it must be dropped rather than guessed at, because awarding the span to
// one of them would credit the wrong card half the time. A form shared with
// some *other* word of the course is no problem at all: nothing here is
// looking for that word, and the learner is not being graded on it.
//
// Two forms are indistinguishable precisely when the matcher can't tell them
// apart, which (see `same_letter`) is when they are equal case-insensitively.
// A word listing the same form twice is not ambiguous with itself.
fn disambiguate<'a, 'b>(words: &[(&'a str, &'b [String])]) -> Vec<(&'a str, Vec<&'b str>)> {
    // form -> the one word that claims it, or None once a second one does
    let mut owner: HashMap<String, Option<&'a str>> = HashMap::new();
    for &(id, forms) in words {
        for form in forms {
            owner
                .entry(form.to_lowercase())
                .and_modify(|existing| {
                    if *existing != Some(id) {
                        *existing = None;
                    }
                })
                .or_insert(Some(id));
        }
    }
    words
        .iter()
        .map(|&(id, forms)| {
            let usable = forms
                .iter()
                .filter(|form| owner[&form.to_lowercase()] == Some(id))
                .map(String::as_str)
                .collect();
            (id, usable)
        })
        .collect()
}

// every place one of `forms` occurs in `text` at word boundaries
fn occurrences(text: &str, forms: &[&str]) -> Vec<Span> {
    let mut spans: Vec<Span> = Vec::new();
    for (start, _) in text.char_indices() {
        if !boundary_before(text, start) {
            continue;
        }
        for form in forms {
            let Some(len) = prefix_len(&text[start..], form) else {
                continue;
            };
            let span = Span {
                start,
                end: start + len,
            };
            if boundary_after(text, span.end) && !spans.contains(&span) {
                spans.push(span);
            }
        }
    }
    spans
}

// How many bytes of `haystack` match `needle`, comparing case-insensitively,
// or None if it doesn't start with it.
//
// Done character by character rather than by lowercasing both strings, so the
// byte offsets we hand back are offsets into the original text — case mapping
// is not guaranteed to preserve length.
fn prefix_len(haystack: &str, needle: &str) -> Option<usize> {
    let mut chars = haystack.char_indices();
    let mut consumed = 0;
    for wanted in needle.chars() {
        let (i, found) = chars.next()?;
        if !same_letter(found, wanted) {
            return None;
        }
        consumed = i + found.len_utf8();
    }
    Some(consumed)
}

fn same_letter(a: char, b: char) -> bool {
    a == b || a.to_lowercase().eq(b.to_lowercase())
}

// a match may not start in the middle of a word...
fn boundary_before(text: &str, at: usize) -> bool {
    text[..at]
        .chars()
        .next_back()
        .is_none_or(|c| !c.is_alphanumeric())
}

// ...nor end in the middle of one. Punctuation, spaces and the ends of the
// string are all fine.
fn boundary_after(text: &str, at: usize) -> bool {
    text[at..]
        .chars()
        .next()
        .is_none_or(|c| !c.is_alphanumeric())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn forms(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    // --- phrasings ---

    // What the browser will do with a mark: `text.slice(start, end)`. Written
    // out longhand over UTF-16 code units, because that is the whole point.
    fn sliced(phrasing: &Phrasing, mark: &Mark) -> String {
        let units: Vec<u16> = phrasing.text.encode_utf16().collect();
        String::from_utf16(&units[mark.start..mark.end]).unwrap()
    }

    #[test]
    fn a_phrasing_keeps_its_text_and_the_spans_it_was_given() {
        let text = "Como pan.";
        let phrasing = Phrasing::of(
            text,
            &[
                ("comer", Span { start: 0, end: 4 }),
                ("pan", Span { start: 5, end: 8 }),
            ],
        );
        assert_eq!(phrasing.text, text);
        assert_eq!(sliced(&phrasing, &phrasing.words[0]), "Como");
        assert_eq!(sliced(&phrasing, &phrasing.words[1]), "pan");
        assert!(phrasing.grades("comer"));
        assert!(!phrasing.grades("hola"));
    }

    // The conversion that has no second chance: `locate` counts bytes and the
    // client counts UTF-16 code units, and every Spanish sentence in the
    // course opens with a character where the two disagree. Off by one per
    // accent is exactly the bug that grades the wrong half of a word.
    #[test]
    fn spans_are_converted_from_bytes_to_the_units_a_browser_indexes() {
        let text = "¡Buenos días, Ana!";
        // "Buenos días" is bytes 2..14 — "¡" is two bytes and "í" is two more
        let phrasing = Phrasing::of(text, &[("buenos_dias", Span { start: 2, end: 14 })]);
        let mark = &phrasing.words[0];
        // ...but code units 1..12, which is what the client is told
        assert_eq!((mark.start, mark.end), (1, 12));
        assert_eq!(sliced(&phrasing, mark), "Buenos días");
    }

    // Characters outside the basic plane take *two* UTF-16 units, so a count
    // of `chars` would be wrong here where a count of bytes was merely
    // different. An emoji in a sentence is unlikely; being quietly wrong about
    // one is not worth the risk of assuming so.
    #[test]
    fn an_astral_character_counts_as_the_two_units_it_is() {
        let text = "🙂 hola";
        let phrasing = Phrasing::of(text, &[("hola", Span { start: 5, end: 9 })]);
        let mark = &phrasing.words[0];
        assert_eq!((mark.start, mark.end), (3, 7));
        assert_eq!(sliced(&phrasing, mark), "hola");
    }

    // a phrasing nothing could be located in still has its text: the client
    // grades it all-or-nothing, which costs precision and never correctness
    #[test]
    fn a_phrasing_with_no_spans_is_still_an_answer() {
        let phrasing = Phrasing::of("Buenos días.", &[]);
        assert_eq!(phrasing.text, "Buenos días.");
        assert!(phrasing.words.is_empty());
    }

    // the correct tiles are cut once, off the preferred phrasing alone — an
    // alternative is accepted, never shown, and never tapped
    #[test]
    fn a_wordings_tiles_come_from_its_preferred_phrasing() {
        let wording = Wording::new(
            Phrasing::of("¡Hola, Ana!", &[]),
            vec![Phrasing::of("Buenas, Ana!", &[])],
        );
        assert_eq!(wording.tiles(), ["Hola", "Ana"]);
        assert_eq!(wording.accepted().len(), 2);
    }

    // --- expansion ---

    #[test]
    fn a_sentence_without_brackets_is_itself() {
        assert_eq!(expand("Como el pan.").unwrap(), ["Como el pan."]);
    }

    #[test]
    fn a_group_becomes_one_sentence_per_branch() {
        assert_eq!(
            expand("[Yo como/Como] el pan.").unwrap(),
            ["Yo como el pan.", "Como el pan."]
        );
    }

    // an optional ending is an empty first branch, which is also why a group
    // must contain a '/' — `[s]` would be ambiguous
    #[test]
    fn an_empty_branch_makes_the_rest_optional() {
        assert_eq!(
            expand("la naranja[/s]").unwrap(),
            ["la naranja", "la naranjas"]
        );
    }

    // several groups take the cartesian product, and the canonical sentence
    // is the one made of every first branch
    #[test]
    fn groups_multiply_first_branches_first() {
        assert_eq!(
            expand("[Yo/Tú] [como/comes] pan").unwrap(),
            ["Yo como pan", "Yo comes pan", "Tú como pan", "Tú comes pan"]
        );
    }

    #[test]
    fn malformed_brackets_are_rejected() {
        assert!(expand("[a/b").is_err()); // unclosed
        assert!(expand("[a/[b/c]]").is_err()); // nested
        assert!(expand("naranja[s]").is_err()); // no alternatives
    }

    // a runaway expansion is a mistake in the data, not thoroughness
    #[test]
    fn an_enormous_expansion_is_rejected() {
        let text = "[a/b] [a/b] [a/b] [a/b] [a/b] [a/b] [a/b]"; // 2^7
        assert!(expand(text).is_err());
    }

    // --- locating ---

    #[test]
    fn a_word_is_found_by_any_of_its_forms() {
        let comer = forms(&["comer", "como", "comí"]);
        let located = locate("Yo comí el pan.", &[("comer", &comer)]);
        let span = located["comer"];
        assert_eq!(&"Yo comí el pan."[span.start..span.end], "comí");
    }

    #[test]
    fn matching_ignores_case() {
        let hola = forms(&["hola"]);
        let located = locate("Hola, Juan.", &[("hola", &hola)]);
        assert_eq!(located["hola"], Span { start: 0, end: 4 });
    }

    // the one rule that silently mismarks everything if it is missed: "es"
    // lives inside "estás", and a substring match would grade the wrong word
    #[test]
    fn matching_respects_word_boundaries() {
        let es = forms(&["es"]);
        assert!(locate("¿Cómo estás?", &[("es", &es)]).is_empty());
        assert!(!locate("Él es alto.", &[("es", &es)]).is_empty());
    }

    // punctuation and the ends of the string are boundaries, accented letters
    // are not
    #[test]
    fn punctuation_is_a_boundary_but_accents_are_not() {
        let si = forms(&["sí"]);
        assert!(!locate("¡Sí!", &[("si", &si)]).is_empty());
        // "sí" is not a word inside "sígueme"
        assert!(locate("Sígueme", &[("si", &si)]).is_empty());
    }

    // multi-word forms are ordinary forms; nothing about the matcher assumes a
    // word is one token
    #[test]
    fn a_form_may_be_several_words() {
        let bd = forms(&["buenos días"]);
        let located = locate("¡Buenos días, Ana!", &[("buenos_dias", &bd)]);
        let span = located["buenos_dias"];
        assert_eq!(&"¡Buenos días, Ana!"[span.start..span.end], "Buenos días");
    }

    // The rule that replaces a course-wide unambiguous-forms pass: two words
    // tagged in the *same* sentence that offer the same form lose it, both of
    // them, because awarding the span to either would credit the wrong card
    // half the time. Case doesn't hide the clash — the matcher ignores it too.
    #[test]
    fn a_form_two_tagged_words_share_is_dropped_from_both() {
        let ser = forms(&["es", "Soy"]);
        let estar = forms(&["está", "soy"]);
        let located = locate("Soy alto y está aquí.", &[("ser", &ser), ("estar", &estar)]);
        // "soy" is claimed by both, so neither is marked on it...
        assert!(!located.contains_key("ser"));
        // ...but the forms they don't share still work
        let span = located["estar"];
        assert_eq!(&"Soy alto y está aquí."[span.start..span.end], "está");
    }

    // ...and a form shared with a word this sentence *doesn't* tag is no
    // clash at all: nothing here is looking for that word. This is the whole
    // reason ambiguity is judged per sentence rather than course-wide.
    #[test]
    fn a_form_only_shared_outside_the_sentence_still_matches() {
        let comer = forms(&["como"]);
        // "como" is also the question word, but that word isn't tagged here
        let located = locate("Como pan.", &[("comer", &comer)]);
        assert_eq!(located["comer"], Span { start: 0, end: 4 });
    }

    // a word repeating a form in its own list is not ambiguous with itself
    #[test]
    fn a_repeated_form_within_one_word_is_kept() {
        let pan = forms(&["pan", "Pan"]);
        let located = locate("El pan.", &[("pan", &pan)]);
        assert!(located.contains_key("pan"));
    }

    #[test]
    fn the_longest_match_wins() {
        let de = forms(&["de"]);
        let del = forms(&["del"]);
        let located = locate("la casa del hombre", &[("de", &de), ("del", &del)]);
        // "del" is longer, so it claims the text and "de" is left with nothing
        // (there is no other "de" in the sentence to fall back to)
        assert!(located.contains_key("del"));
        assert!(!located.contains_key("de"));
    }

    // once a stretch of text is claimed, a second word may not also claim it —
    // otherwise one span would grade two cards
    #[test]
    fn spans_never_overlap() {
        let a = forms(&["comer pan"]);
        let b = forms(&["pan"]);
        let located = locate("comer pan", &[("a", &a), ("b", &b)]);
        assert!(located.contains_key("a"));
        assert!(!located.contains_key("b"));
    }

    // a word used in a form its list doesn't cover simply isn't located; the
    // caller leaves it unmarked and the client grades it on the whole answer
    #[test]
    fn an_unlisted_form_is_not_found() {
        let comer = forms(&["comer", "como"]);
        assert!(locate("Ellos comieron pan.", &[("comer", &comer)]).is_empty());
    }

    // two words in one sentence each get their own span
    #[test]
    fn several_words_are_located_independently() {
        let comer = forms(&["como"]);
        let pan = forms(&["pan"]);
        let located = locate("Como pan.", &[("comer", &comer), ("pan", &pan)]);
        assert_eq!(located.len(), 2);
        assert!(located["comer"].start < located["pan"].start);
    }
}
