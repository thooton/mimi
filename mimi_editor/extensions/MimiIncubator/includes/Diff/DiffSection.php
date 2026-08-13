<?php

namespace MediaWiki\Extension\MimiIncubator\Diff;

/**
 * One thing an author would recognise inside a structured page, a word, a
 * glossary entry, a tip, reduced to the text worth diffing.
 *
 * A section is keyed by something the author wrote rather than by its position,
 * so inserting a word above another one does not report both as changed, and a
 * dragged word reports as moved rather than as two rewrites. Renaming the thing
 * a section is keyed by is therefore a removal and an addition, which is what
 * replacing a word or a lemma actually is.
 *
 * Its fields are grouped because a word owns several sentences and each
 * sentence owns a handful of fields; flattening that into
 * "sentence 3 alternative translations" keys is what made the old diff
 * unreadable. Groups, and the fields within them, keep the order the page
 * stores them in, that order is itself authored.
 *
 * A field whose value is the empty string is absent, not blank: a sentence
 * with no notes has nothing to say about notes. Absent fields are left out of
 * an added or removed section entirely, and drawn as a placeholder opposite a
 * side that has something, so that gaining a translation cannot be misread as
 * losing one.
 */
final class DiffSection {
	/**
	 * @param string $kind The noun for this section, "Word", "Tip", shown
	 *   before its label. Empty for the one-of-a-kind sections a page always
	 *   has, such as a skill's grammar focus, which name themselves.
	 * @param string $label What the author calls it: the word, the lemma, the
	 *   tip's title.
	 * @param array<string,array<string,string>> $groups Group label => field
	 *   label => value. The empty string keys the group of fields the section
	 *   owns directly, which is drawn without a heading.
	 */
	public function __construct(
		public readonly string $kind,
		public readonly string $label,
		public readonly array $groups,
	) {
	}
}
