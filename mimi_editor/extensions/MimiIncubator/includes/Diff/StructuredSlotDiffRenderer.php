<?php

namespace MediaWiki\Extension\MimiIncubator\Diff;

use MediaWiki\Content\Content;
use MediaWiki\Content\JsonContent;
use MediaWiki\Diff\SlotDiffRenderer;
use MediaWiki\Html\Html;
use MediaWiki\Output\OutputPage;
use Wikimedia\Diff\ComplexityException;
use Wikimedia\Diff\Diff;
use Wikimedia\Diff\WordLevelDiff;

/**
 * The diff view for all four structured models.
 *
 * Nobody edits the JSON, so nobody should have to read it: a revision is
 * summarised into the things an author works with, words, entries, tips, rows
 * of the tree, and only those that differ are drawn. Each one becomes a card
 * headed by its own name and told plainly what became of it, and inside the
 * card every changed field is one line with its label written once and the two
 * versions beside it.
 *
 * Two decisions carry most of the legibility:
 *
 * - Word-level highlighting comes from core. `WordLevelDiff` is what
 *   MediaWiki's own wikitext diffs use, so a corrected sentence marks the words
 *   that actually moved, in the colours a reader already knows, and the
 *   `diff-addedline` / `diff-deletedline` classes keep the cells looking native.
 * - Derived numbers are not diffed. A word count changing says nothing the
 *   added and removed words below it do not already say, so counts became the
 *   tally in the bar at the top instead of three rows of noise.
 *
 * The four columns MediaWiki gives a slot diff cannot hold a label column, so
 * every row here spans all four and lays itself out inside. That is also what
 * lets the whole thing stack into one column on a phone, which the fixed
 * four-column diff table cannot do.
 */
final class StructuredSlotDiffRenderer extends SlotDiffRenderer {
	/** The thing a page of this kind is mostly a list of, singular and plural. */
	private const NOUNS = [
		'skill' => [ 'word', 'words' ],
		'glossary' => [ 'entry', 'entries' ],
		'tips' => [ 'tip', 'tips' ],
		'course' => [ 'section', 'sections' ],
	];

	/** Status name => the chip's colours, in the WikimediaUI palette. */
	private const STATUS_COLOURS = [
		'added' => 'border-[#00af89] bg-[#d5fdf4] text-[#14866d]',
		'removed' => 'border-[#d73333] bg-[#fee7e6] text-[#b32424]',
		'moved' => 'border-[#a2a9b1] bg-[#eaecf0] text-[#54595d]',
		'changed' => 'border-[#3366cc] bg-[#eaf3ff] text-[#3366cc]',
	];

	private const CHIP_CLASSES =
		'inline-flex shrink-0 items-center rounded-sm border border-solid px-1.5 py-0.5 text-xs font-semibold leading-tight';

	/**
	 * Label, then the two versions. Below the breakpoint the three stack, which
	 * is the only way any of this fits a phone.
	 */
	private const FIELD_CLASSES =
		'grid gap-1 px-3 py-2 md:grid-cols-[9.5rem_minmax(0,1fr)_minmax(0,1fr)] md:items-start md:gap-2';

	private const FIELD_LABEL_CLASSES =
		'break-words pt-1 text-xs font-semibold uppercase tracking-wide text-[#54595d]';

	public function __construct( private readonly string $kind ) {
	}

	public function addModules( OutputPage $output ) {
		$output->addModuleStyles( [ 'ext.mimiIncubator.diff' ] );
	}

	public function getDiff( ?Content $oldContent = null, ?Content $newContent = null ) {
		$this->normalizeContents( $oldContent, $newContent, JsonContent::class );
		/** @var JsonContent $oldContent */
		/** @var JsonContent $newContent */
		$old = $this->sections( $oldContent );
		$new = $this->sections( $newContent );
		[ $order, $moved ] = self::align( array_keys( $old ), array_keys( $new ) );

		$tally = [ 'added' => 0, 'removed' => 0, 'moved' => 0, 'changes' => 0 ];
		$cards = '';
		foreach ( $order as $id ) {
			$before = $old[$id] ?? null;
			$after = $new[$id] ?? null;
			if ( $before === null || $after === null ) {
				$section = $after ?? $before;
				$status = $after === null ? 'removed' : 'added';
				$tally[$status]++;
				$cards .= $this->card( $section, [ $status ], $this->wholeSection( $section, $status ) );
				continue;
			}
			$body = $this->changedFields( $before->groups, $after->groups, $tally );
			$isMoved = isset( $moved[$id] );
			if ( $body === '' && !$isMoved ) {
				continue;
			}
			$statuses = $body === '' ? [ 'moved' ] : ( $isMoved ? [ 'changed', 'moved' ] : [ 'changed' ] );
			if ( $isMoved ) {
				$tally['moved']++;
			}
			$cards .= $this->card( $after, $statuses, $body );
		}
		return $cards === '' ? '' : $this->summaryBar( $tally ) . $cards;
	}

	/**
	 * The order to draw sections in, and which of them merely moved.
	 *
	 * Both fall out of one alignment of the two key lists. Everything core's
	 * differ calls a copy kept its place relative to the sections around it, so
	 * anything it does not, while still existing on both sides, was dragged
	 * somewhere. Asking instead whether a section's index changed would report
	 * the whole glossary as moved every time a lemma is added near the top.
	 *
	 * @param string[] $oldIds
	 * @param string[] $newIds
	 * @return array{0:string[],1:array<string,true>}
	 */
	private static function align( array $oldIds, array $newIds ): array {
		try {
			$edits = ( new Diff( $oldIds, $newIds ) )->getEdits();
		} catch ( ComplexityException ) {
			// Too many sections to align. The new revision's order with the
			// removals after it is still a diff, just one that calls every
			// reordering a rewrite.
			return [ array_keys( array_flip( array_merge( $newIds, $oldIds ) ) ), [] ];
		}
		$inBoth = array_flip( array_intersect( $oldIds, $newIds ) );
		$order = [];
		$moved = [];
		foreach ( $edits as $edit ) {
			// A change lists what it replaced and what replaced it; taking both
			// keeps a section that only moved from being drawn twice, since the
			// keys are what deduplicates. The side an operation does not touch
			// is false rather than an empty array, hence the type check.
			$ids = array_merge(
				is_array( $edit->orig ) ? $edit->orig : [],
				is_array( $edit->closing ) ? $edit->closing : [] );
			foreach ( $ids as $id ) {
				if ( $edit->type !== 'copy' && isset( $inBoth[$id] ) ) {
					$moved[$id] = true;
				}
				$order[$id] = true;
			}
		}
		return [ array_keys( $order ), $moved ];
	}

	/**
	 * Every field of a section that exists on only one side, drawn against the
	 * blank half of the table. Absent fields are left out rather than shown as
	 * empty: a new sentence with no notes has nothing to say about notes.
	 */
	private function wholeSection( DiffSection $section, string $status ): string {
		$blocks = '';
		foreach ( $section->groups as $groupLabel => $fields ) {
			$rows = '';
			foreach ( $fields as $label => $value ) {
				if ( $value !== '' ) {
					$rows .= $this->oneSidedRow( $status, $label, $value );
				}
			}
			$blocks .= self::group( $groupLabel, $rows );
		}
		return $blocks;
	}

	/**
	 * A field with nothing opposite it, given the whole width rather than half.
	 * Nothing is highlighted within it: every word of it was added or taken
	 * away, and marking them all only makes it harder to read.
	 */
	private function oneSidedRow( string $status, string $label, string $value ): string {
		return Html::rawElement( 'div', [ 'class' => self::FIELD_CLASSES ],
			Html::element( 'div', [ 'class' => self::FIELD_LABEL_CLASSES ], $label ) .
			$this->valueCell( $status === 'added' ? 'added' : 'deleted', htmlspecialchars( $value ), true ) );
	}

	/**
	 * The fields that differ between two versions of the same section, grouped
	 * as the page groups them. A group only one side has is not special-cased:
	 * its fields simply have nothing opposite them.
	 *
	 * @param array<string,array<string,string>> $before
	 * @param array<string,array<string,string>> $after
	 * @param array<string,int> &$tally
	 */
	private function changedFields( array $before, array $after, array &$tally ): string {
		$blocks = '';
		foreach ( self::mergedKeys( $before, $after ) as $groupLabel ) {
			$oldFields = $before[$groupLabel] ?? [];
			$newFields = $after[$groupLabel] ?? [];
			// A whole sentence added or taken away is one thing that happened,
			// not four fields that each became "not set". Saying so on the
			// heading is both shorter and what the author would say.
			$status = self::wholeGroupStatus( $oldFields, $newFields );
			$rows = '';
			foreach ( self::mergedKeys( $oldFields, $newFields ) as $label ) {
				$was = $oldFields[$label] ?? '';
				$now = $newFields[$label] ?? '';
				if ( $was === $now ) {
					continue;
				}
				if ( $status === '' ) {
					$tally['changes']++;
					$rows .= $this->fieldRow( $label, $was, $now );
				} else {
					$rows .= $this->oneSidedRow( $status, $label, $status === 'added' ? $now : $was );
				}
			}
			if ( $rows !== '' ) {
				$tally['changes'] += $status === '' ? 0 : 1;
				$blocks .= self::group( $groupLabel, $rows, $status );
			}
		}
		return $blocks;
	}

	/**
	 * Whether one side of a group holds nothing at all, which is what a sentence
	 * or a form having been added or removed looks like from here. The unnamed
	 * group a section owns directly has no heading to carry the answer, but it
	 * still gains the single full-width column, which is the point.
	 *
	 * @param array<string,string> $oldFields
	 * @param array<string,string> $newFields
	 */
	private static function wholeGroupStatus( array $oldFields, array $newFields ): string {
		$has = static fn ( array $fields ) => implode( '', $fields ) !== '';
		if ( $has( $oldFields ) === $has( $newFields ) ) {
			return '';
		}
		return $has( $newFields ) ? 'added' : 'removed';
	}

	/**
	 * The keys of two ordered maps, in the new revision's order, with whatever
	 * only the old one had after them.
	 *
	 * @return string[]
	 */
	private static function mergedKeys( array $old, array $new ): array {
		$keys = $new;
		foreach ( $old as $key => $unused ) {
			$keys[$key] ??= null;
		}
		return array_map( 'strval', array_keys( $keys ) );
	}

	/** One field: its label once, and the two versions beside it. */
	private function fieldRow( string $label, string $was, string $now ): string {
		if ( $was !== '' && $now !== '' ) {
			// WordLevelDiff escapes its own output and returns one string per
			// line, which rejoin into the pre-wrapped cell they came from.
			$diff = new WordLevelDiff( explode( "\n", $was ), explode( "\n", $now ) );
			$oldCell = $this->valueCell( 'deleted', implode( "\n", $diff->orig() ) );
			$newCell = $this->valueCell( 'added', implode( "\n", $diff->closing() ) );
		} else {
			$oldCell = $was === '' ? self::emptyCell() : $this->valueCell( 'deleted', htmlspecialchars( $was ) );
			$newCell = $now === '' ? self::emptyCell() : $this->valueCell( 'added', htmlspecialchars( $now ) );
		}
		return Html::rawElement( 'div', [ 'class' => self::FIELD_CLASSES ],
			Html::element( 'div', [ 'class' => self::FIELD_LABEL_CLASSES ], $label ) . $oldCell . $newCell );
	}

	/**
	 * One version of one field. The border, the highlight colours and the
	 * wrapping are core's, so these read as the diff cells they are; only the
	 * typeface is overridden, because the editfont preference is about wikitext
	 * and these hold sentences.
	 */
	private function valueCell( string $side, string $html, bool $wide = false ): string {
		$deleted = $side === 'deleted';
		return Html::rawElement( 'div', [
			'class' => ( $deleted ? 'diff-deletedline diff-side-deleted' : 'diff-addedline diff-side-added' ) .
				' flex min-w-0 gap-2 px-2 py-1 font-sans text-[13px] leading-normal' .
				( $wide ? ' md:col-span-2' : '' ),
		],
			Html::element( 'span', [
				'class' => 'shrink-0 select-none font-bold text-[#72777d]',
				'aria-hidden' => 'true',
			], $deleted ? '−' : '+' ) .
			Html::rawElement( 'span', [ 'class' => 'min-w-0 flex-1 whitespace-pre-wrap break-words' ], $html ) );
	}

	/**
	 * What stands opposite a field the other revision does not have. Left
	 * blank, a gained translation and a lost one look the same.
	 */
	private static function emptyCell(): string {
		return Html::element( 'div', [
			'class' => 'diff-context min-w-0 px-2 py-1 font-sans text-[13px] italic leading-normal text-[#72777d]',
		], 'not set' );
	}

	/** A group of fields under its heading, or without one when it has no name. */
	private static function group( string $label, string $rows, string $status = '' ): string {
		if ( $rows === '' ) {
			return '';
		}
		$heading = $label === '' ? '' : Html::rawElement( 'div', [
			'class' => 'flex flex-wrap items-center gap-2 bg-[#fcfcfc] px-3 pt-2 text-[13px] font-semibold text-[#202122]',
		],
			Html::element( 'span', [ 'class' => 'min-w-0 break-words' ], $label ) .
			( $status === '' ? '' : self::chip( $status ) ) );
		return Html::rawElement( 'div', [
			'class' => 'border-0 border-t border-solid border-[#eaecf0] first:border-t-0',
		], $heading . $rows );
	}

	/** The coloured word for what became of a section or a group. */
	private static function chip( string $status ): string {
		return Html::element( 'span', [
			'class' => self::CHIP_CLASSES . ' ' . self::STATUS_COLOURS[$status],
		], ucfirst( $status ) );
	}

	/** One section's card, as the one full-width row a slot diff may add. */
	private function card( DiffSection $section, array $statuses, string $body ): string {
		$chips = implode( '', array_map( self::chip( ... ), $statuses ) );
		return Html::rawElement( 'tr', [], Html::rawElement( 'td', [
			'colspan' => '4',
			'class' => 'p-0 align-top',
		],
			Html::rawElement( 'div', [
				'class' => 'overflow-hidden rounded-sm border border-solid border-[#c8ccd1] bg-white font-sans text-[#202122]',
			],
				Html::rawElement( 'div', [
					'class' => 'flex flex-wrap items-center gap-x-2 gap-y-1 border-0 border-b border-solid ' .
						'border-[#c8ccd1] bg-[#f8f9fa] px-3 py-2',
				],
					( $section->kind === '' ? '' : Html::element( 'span', [
						'class' => 'shrink-0 text-xs font-semibold uppercase tracking-wide text-[#72777d]',
					], $section->kind ) ) .
					Html::element( 'span', [ 'class' => 'min-w-0 break-words text-sm font-semibold' ], $section->label ) .
					Html::rawElement( 'span', [ 'class' => 'ml-auto flex shrink-0 flex-wrap gap-1' ], $chips ) ) .
				// The groups are wrapped so that the first of them is a first
				// child and can drop the rule it would otherwise draw against
				// the header's own.
				Html::rawElement( 'div', [], $body ) ) ) );
	}

	/**
	 * What the edit did, in one line above the cards. This is where the counts
	 * the old diff spent three rows on belong: they are worth a glance and never
	 * worth reading twice.
	 *
	 * @param array<string,int> $tally
	 */
	private function summaryBar( array $tally ): string {
		[ $singular, $plural ] = self::NOUNS[$this->kind] ?? self::NOUNS['course'];
		$noun = static fn ( int $n ) => $n === 1 ? $singular : $plural;
		$chips = '';
		$parts = [
			'added' => static fn ( int $n ) => '+' . $n . ' ' . $noun( $n ),
			'removed' => static fn ( int $n ) => '−' . $n . ' ' . $noun( $n ),
			'moved' => static fn ( int $n ) => $n . ' ' . $noun( $n ) . ' moved',
			'changes' => static fn ( int $n ) => $n . ( $n === 1 ? ' change' : ' changes' ),
		];
		foreach ( $parts as $status => $text ) {
			if ( $tally[$status] > 0 ) {
				$chips .= Html::element( 'span', [
					'class' => self::CHIP_CLASSES . ' ' . self::STATUS_COLOURS[$status === 'changes' ? 'changed' : $status],
				], $text( $tally[$status] ) );
			}
		}
		return Html::rawElement( 'tr', [], Html::rawElement( 'td', [
			'colspan' => '4',
			'class' => 'p-0 align-top',
		],
			Html::rawElement( 'div', [
				'class' => 'flex flex-wrap items-center gap-2 rounded-sm border border-solid border-[#c8ccd1] ' .
					'bg-[#f8f9fa] px-3 py-2 font-sans text-sm text-[#202122]',
			],
				Html::element( 'span', [ 'class' => 'font-semibold' ], 'This edit' ) . $chips ) ) );
	}

	/**
	 * A revision reduced to its sections.
	 *
	 * @return array<string,DiffSection>
	 */
	private function sections( JsonContent $content ): array {
		if ( !$content->isValid() ) {
			// Nothing here can summarise what never parsed, and saying so is
			// less use than showing it: as text, the two blobs still diff.
			return [ 'invalid' => new DiffSection( '', 'Unreadable JSON', [ '' => [
				'Page source' => $content->getText(),
			] ] ) ];
		}
		$data = $content->getData()->getValue();
		return match ( $this->kind ) {
			'skill' => self::skillSections( $data ),
			'glossary' => self::glossarySections( $data ),
			'tips' => self::tipsSections( $data ),
			// The language pair lives in the page name, so it cannot differ here.
			default => self::courseSections( $data ),
		};
	}

	/** @return array<string,DiffSection> */
	private static function skillSections( object $data ): array {
		$sections = [ 'grammarFocus' => new DiffSection( '', 'Skill', [ '' => [
			'Grammar focus' => $data->grammarFocus ?? '',
		] ] ) ];
		foreach ( $data->words ?? [] as $word ) {
			$name = $word->word ?? '?';
			$groups = [];
			foreach ( $word->sentences ?? [] as $index => $sentence ) {
				// Sentences are keyed by position because they have no name of
				// their own, so a sentence inserted above another renumbers it.
				// The word they belong to keeps that from spreading any further.
				$groups['Sentence ' . ( $index + 1 )] = [
					'Text' => $sentence->text ?? '',
					'Translation' => $sentence->translation ?? '',
					'Alternative sentences' => self::lines( $sentence->alternativeSentences ?? [] ),
					'Alternative translations' => self::lines( $sentence->alternativeTranslations ?? [] ),
					'Notes' => $sentence->notes ?? '',
					'Status' => empty( $sentence->disabled ) ? '' : 'Disabled',
				];
			}
			$sections['word:' . $name] = new DiffSection( 'Word', $name, $groups );
		}
		return $sections;
	}

	/** @return array<string,DiffSection> */
	private static function glossarySections( object $data ): array {
		$sections = [];
		foreach ( $data->entries ?? [] as $entry ) {
			$lemma = $entry->lemma ?? '?';
			$groups = [];
			foreach ( $entry->forms ?? [] as $index => $form ) {
				// Forms keep the order they were written in, so they are keyed
				// by position: a reordered paradigm is a change worth showing.
				// The spelling is a field rather than part of the key, so that
				// correcting one reads as a correction and not as a form
				// replaced by another.
				$groups['Form ' . ( $index + 1 )] = [
					// The first form is the lemma standing for itself and has
					// no spelling of its own; it is absent rather than blank.
					'Spelling' => $form->form ?? '',
					'Translations' => self::numbered( $form->translations ?? [] ),
				];
			}
			$sections['entry:' . $lemma] = new DiffSection( 'Entry', $lemma, $groups );
		}
		return $sections;
	}

	/** @return array<string,DiffSection> */
	private static function tipsSections( object $data ): array {
		$sections = [];
		foreach ( $data->tips ?? [] as $tip ) {
			$title = $tip->title ?? '?';
			$sections['tip:' . $title] = new DiffSection( 'Tip', $title, [ '' => [
				'Shown' => isset( $tip->lesson ) ? 'Before lesson ' . $tip->lesson : 'Tips button only',
				'Body' => $tip->body ?? '',
			] ] );
		}
		return $sections;
	}

	/** @return array<string,DiffSection> */
	private static function courseSections( object $data ): array {
		$rows = [];
		foreach ( $data->rows ?? [] as $index => $row ) {
			// One skill per line, under the name the tree shows: a skill moved
			// between rows then reads as a line leaving one row and joining
			// another, rather than as two rewritten sentences of page titles.
			$rows['Row ' . ( $index + 1 )] = self::lines( array_map(
				static fn ( string $skill ) => preg_replace( '#^Skill:(?:.*/)?#', '', $skill ),
				$row ) );
		}
		$castles = [];
		foreach ( $data->castles ?? [] as $index => $castle ) {
			$castles['Castle ' . ( $index + 1 )] = 'after row ' . ( $castle->afterRow ?? '?' );
		}
		return [
			'rows' => new DiffSection( '', 'Course tree', [ '' => $rows ] ),
			'castles' => new DiffSection( '', 'Castles', [ '' => $castles ] ),
		];
	}

	/** A list as one item per line, so adding one shows as one added line. */
	private static function lines( array $items ): string {
		return implode( "\n", array_map( 'strval', $items ) );
	}

	/** The same, numbered, where the positions are part of what is stored. */
	private static function numbered( array $items ): string {
		$lines = [];
		foreach ( array_values( $items ) as $index => $item ) {
			$lines[] = ( $index + 1 ) . '. ' . $item;
		}
		return implode( "\n", $lines );
	}
}
