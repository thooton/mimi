use std::collections::HashMap;

use crate::convert::GLOSSES;
use crate::gloss::Gloss;

pub struct Dictionary {
    // target -> source[]
    map: HashMap<String, Vec<String>>,
}

impl Dictionary {
    // Build the dictionary from the glossary, flattened to `form -> meanings`
    // by `convert::flatten_glossary`.
    //
    // This is the glossary's second reading. Grouped by lemma it is the
    // vocabulary — the concepts a skill teaches; flattened like this it is what
    // any word of a sentence means when a learner taps it, which is why it
    // covers scenery the course never teaches.
    //
    // After conversion has given a lemma precedence over identically spelt
    // forms, a remaining form that two lemmas both claim keeps both meanings
    // rather than whichever came last: the reader is a person looking at one
    // word, and the honest answer is that it could be either.
    //
    // Both, but not all of both. The glossary hands over `GLOSSES` meanings per
    // entry, and merging is the one place that count can be exceeded — "niña"
    // read off `niña` and again off `niño` is five, which is the column a tap
    // was never meant to open. The first `GLOSSES` of the merge stand, and
    // since entries arrive in the glossary's order, best first, they are the
    // ones worth reading.
    //
    // A form with no meanings at all is dropped instead of stored empty. It
    // would otherwise match in `annotate` and win against a shorter phrase that
    // does have something to say — a silent way to gloss less than we know.
    pub fn from_entries(entries: Vec<(String, Vec<String>)>) -> Self {
        let mut map: HashMap<String, Vec<String>> = HashMap::new();
        for (form, meanings) in entries {
            if meanings.is_empty() {
                continue;
            }
            // `annotate` normalizes sentence text to lowercase; make the
            // constructor uphold the other half of that contract even when a
            // caller did not pre-normalize the glossary projection.
            let known = map.entry(form.to_lowercase()).or_default();
            for meaning in meanings {
                if known.len() >= GLOSSES {
                    break;
                }
                if !known.contains(&meaning) {
                    known.push(meaning);
                }
            }
        }
        Dictionary { map }
    }

    pub fn get(&self, target: &str) -> Option<&[String]> {
        self.map.get(target).map(Vec::as_slice)
    }

    // annotate
    pub fn annotate<'a>(&self, s: &'a str) -> Vec<Gloss<'a>> {
        // Each word with its byte offset in `s`.
        let base = s.as_ptr() as usize;
        let words: Vec<(usize, &str)> = s
            .split_whitespace()
            .map(|w| (w.as_ptr() as usize - base, w))
            .collect();

        // Normalized form of each word for lookups: punctuation-trimmed, lowercased.
        let keys: Vec<String> = words
            .iter()
            .map(|&(_, w)| {
                w.trim_matches(|c: char| !c.is_alphanumeric())
                    .to_lowercase()
            })
            .collect();

        let mut result = Vec::new();
        let mut i = 0;

        while i < words.len() {
            let max = 4.min(words.len() - i);

            // Try spans of 4, 3, 2, 1 words; fall through to n = 1 unmatched.
            let (n, meanings) = (1..=max)
                .rev()
                .find_map(|n| self.get(&keys[i..i + n].join(" ")).map(|m| (n, m.to_vec())))
                .unwrap_or((1, Vec::new()));

            let (first_start, first) = words[i];
            let (last_start, last) = words[i + n - 1];
            // Punctuation helps a token sit in a sentence; it is not part of
            // the word the dictionary explains. Lookup has always discarded
            // it through `keys` above, so the surface quote on the wire must
            // draw the same boundary. Otherwise a client faithfully hangs
            // its dotted gloss underline from `niño.` and makes the period
            // look interactive too.
            //
            // Keep punctuation *between* the words of a multi-word entry — a
            // span has to remain contiguous — and trim only the two outside
            // edges. `split_whitespace` gives us slices of `s`, so the byte
            // arithmetic still points into the original UTF-8 string.
            let first_word = first.trim_start_matches(|c: char| !c.is_alphanumeric());
            let last_word = last.trim_end_matches(|c: char| !c.is_alphanumeric());
            let start = first_start + (first.len() - first_word.len());
            let end = last_start + last_word.len();
            // A punctuation-only unknown token has no lexical interior. It
            // carries no meanings and the client will not make it a hint, but
            // keep the original quote rather than constructing an inverted
            // range while walking past it.
            let text = if start <= end {
                &s[start..end]
            } else {
                &s[first_start..first_start + first.len()]
            };
            // The dictionary deduplicates authored meanings before this
            // point, but matching the word's case can make two different
            // spellings converge ("the" and "The" both become "The" under
            // "El"). The client only sees these final strings, so make that
            // final representation unique while preserving glossary order.
            let mut rendered = Vec::new();
            for meaning in meanings {
                let meaning = matching_case(text, meaning);
                if !rendered.contains(&meaning) {
                    rendered.push(meaning);
                }
            }
            result.push(Gloss {
                text,
                meanings: rendered,
            });
            i += n;
        }

        result
    }
}

// A meaning written to sit beside the word it translates.
//
// The glossary files its words in lower case, because that is how a dictionary
// lists them, but a gloss is not read in a dictionary — it is read against a
// sentence. "El niño" glossed as "the" reads like a correction of the capital;
// glossed as "The" it reads like a translation, which is what it is.
//
// Only a capital is copied across, never a lower case: a meaning that begins
// with a capital of its own — "I", "Marco", "Monday" — is spelt that way
// wherever it appears, and taking it away would be a spelling mistake rather
// than a matter of position. The first *letter* is what decides, so "¿Cómo" is
// read past its punctuation.
fn matching_case(text: &str, meaning: String) -> String {
    let capitalised = text
        .chars()
        .find(|c| c.is_alphabetic())
        .is_some_and(char::is_uppercase);
    if !capitalised {
        return meaning;
    }
    let mut characters = meaning.chars();
    match characters.next() {
        Some(first) if first.is_lowercase() => {
            first.to_uppercase().collect::<String>() + characters.as_str()
        }
        _ => meaning,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn annotation_prefers_phrases_and_keeps_unknown_words() {
        let dictionary = Dictionary {
            map: HashMap::from([
                ("buenos días".to_string(), vec!["good morning".to_string()]),
                ("hola".to_string(), vec!["hello".to_string()]),
            ]),
        };
        let glosses = dictionary.annotate("¡Hola, buenos días amigo!");
        assert_eq!(glosses[0].text, "Hola");
        // Capitalised in the sentence, so capitalised in the gloss.
        assert_eq!(glosses[0].meanings, ["Hello"]);
        assert_eq!(glosses[1].text, "buenos días");
        assert_eq!(glosses[1].meanings, ["good morning"]);
        assert_eq!(glosses[2].text, "amigo");
        assert!(glosses[2].meanings.is_empty());
    }

    // The glossary is the source now, so the shape it arrives in has to work
    // with the phrase matching above: multi-word lemmas included.
    #[test]
    fn a_dictionary_built_from_the_glossary_annotates() {
        let dictionary = Dictionary::from_entries(vec![
            ("buenos días".into(), vec!["good morning".into()]),
            ("Hola".into(), vec!["hello".into()]),
        ]);
        let glosses = dictionary.annotate("¡Hola, buenos días amigo!");
        assert_eq!(glosses[0].meanings, ["Hello"]);
        assert_eq!(glosses[1].text, "buenos días");
        assert!(glosses[2].meanings.is_empty());
    }

    // The quote is also the exact run the client makes interactive, so
    // sentence punctuation must stay outside it. Lookup already ignores the
    // punctuation; this verifies the served annotation agrees.
    #[test]
    fn a_gloss_quotes_the_word_without_its_sentence_punctuation() {
        let dictionary = Dictionary::from_entries(vec![
            ("niño".into(), vec!["boy".into()]),
            ("buenos días".into(), vec!["good morning".into()]),
        ]);

        let glosses = dictionary.annotate("¿El niño. ¡Buenos días!");
        assert_eq!(glosses[0].text, "El");
        assert_eq!(glosses[1].text, "niño");
        assert_eq!(glosses[2].text, "Buenos días");
    }

    #[test]
    fn punctuation_by_itself_remains_a_harmless_unknown_annotation() {
        let dictionary = Dictionary::from_entries(Vec::new());
        let glosses = dictionary.annotate("hola ... adiós");
        assert_eq!(glosses[1].text, "...");
        assert!(glosses[1].meanings.is_empty());
    }

    // A gloss is read against the sentence, so it is capitalised as the word it
    // translates is — but a meaning spelt with a capital of its own keeps it.
    #[test]
    fn a_gloss_is_capitalised_as_the_word_it_sits_under() {
        let dictionary = Dictionary::from_entries(vec![
            ("el".into(), vec!["the".into()]),
            ("niño".into(), vec!["boy".into(), "child".into()]),
            ("yo".into(), vec!["I".into()]),
        ]);
        let glosses = dictionary.annotate("El niño");
        assert_eq!(glosses[0].meanings, ["The"]);
        assert_eq!(glosses[1].meanings, ["boy", "child"]);
        // Mid-sentence the dictionary's own spelling stands.
        assert_eq!(dictionary.annotate("un niño")[1].meanings, ["boy", "child"]);
        // And a meaning that is always capitalised is not lowercased to match.
        assert_eq!(dictionary.annotate("yo")[0].meanings, ["I"]);
    }

    // Deduplicating glossary input is not enough: case matching happens after
    // lookup, and can turn two distinct authored strings into one wire value.
    #[test]
    fn a_gloss_deduplicates_its_final_strings() {
        let dictionary = Dictionary::from_entries(vec![(
            "el".into(),
            vec!["the".into(), "The".into(), "it".into()],
        )]);

        assert_eq!(dictionary.annotate("El")[0].meanings, ["The", "It"]);
    }

    #[test]
    fn a_form_two_lemmas_claim_keeps_both_meanings() {
        let dictionary = Dictionary::from_entries(vec![
            ("como".into(), vec!["I eat".into()]),
            ("como".into(), vec!["as".into(), "I eat".into()]),
        ]);
        assert_eq!(dictionary.get("como").unwrap(), ["I eat", "as"]);
    }

    // Each entry arrives cut to `GLOSSES`, so only the merge can overshoot it.
    // A tap is answered in a line either way.
    #[test]
    fn a_form_two_lemmas_claim_still_stops_at_three_meanings() {
        let dictionary = Dictionary::from_entries(vec![
            (
                "niña".into(),
                vec!["girl".into(), "little girl".into(), "(female) child".into()],
            ),
            ("niña".into(), vec!["child".into(), "kid".into()]),
        ]);
        assert_eq!(
            dictionary.get("niña").unwrap(),
            ["girl", "little girl", "(female) child"]
        );
    }

    // A form stored with no meanings would match in `annotate` and beat a
    // shorter phrase that does have one, so it must not be stored at all.
    #[test]
    fn a_form_with_nothing_to_say_is_not_stored() {
        let dictionary = Dictionary::from_entries(vec![
            ("buenos días".into(), vec![]),
            ("buenos".into(), vec!["good".into()]),
        ]);
        assert!(dictionary.get("buenos días").is_none());
        assert_eq!(dictionary.annotate("buenos días")[0].text, "buenos");
    }
}
