<?php

namespace MediaWiki\Extension\MimiIncubator;

use MediaWiki\Title\Title;

/** One published course, reduced to what a catalogue entry has to show. */
final class CourseSummary {
	public function __construct(
		public readonly Title $title,
		public readonly string $name,
		public readonly string $targetLanguage,
		public readonly string $sourceLanguage,
		public readonly int $skills,
		public readonly int $rows,
		public readonly int $sentences,
		public readonly int $completeSentences,
		public readonly int $glossaryTerms,
	) {
	}

	/** Share of the course's sentences that are ready to teach. */
	public function completion(): int {
		return $this->sentences === 0
			? 0
			: (int)round( $this->completeSentences / $this->sentences * 100 );
	}

	/** "Spanish, taught in English" — or the page name when it is not a pair. */
	public function subtitle(): string {
		return $this->targetLanguage === ''
			? $this->name
			: $this->targetLanguage . ', taught in ' . $this->sourceLanguage;
	}
}
