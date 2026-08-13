<?php

namespace MediaWiki\Extension\MimiIncubator\Content;

use HtmlArmor;
use MediaWiki\Content\Content;
use MediaWiki\Content\Renderer\ContentParseParams;
use MediaWiki\Extension\MimiIncubator\CourseName;
use MediaWiki\Extension\MimiIncubator\Icon;
use MediaWiki\Html\Html;
use MediaWiki\MediaWikiServices;
use MediaWiki\Page\PageReference;
use MediaWiki\Parser\ParserOutput;

final class SkillContentHandler extends StructuredContentHandler {
	public function __construct() {
		parent::__construct( 'mimi-skill' );
	}
	protected function getContentClass() { return SkillContent::class; }
	protected function getSchemaFile(): string { return 'skill.schema.json'; }
	protected function getEditorKind(): string { return 'skill'; }
	public function makeEmptyContent() {
		return new SkillContent( '{"schemaVersion":5,"grammarFocus":"Describe the sentence pattern.","words":[{"word":"example","sentences":[]}]}' );
	}
	protected function validateSemantics( object $data, string $courseName ): array {
		if ( !isset( $data->words ) || !is_array( $data->words ) ) { return []; }
		$words = array_map( static fn ( $w ) => isset( $w->word ) ? mb_strtolower( trim( $w->word ) ) : '', $data->words );
		$errors = [];
		if ( count( $words ) !== count( array_unique( $words ) ) ) { $errors[] = 'words must be unique within a skill'; }
		return $errors;
	}

	/**
	 * A sentence is complete once it has text and a translation. The editor
	 * trims every answer and drops the blank ones before saving, and the schema
	 * rejects an empty one, so no reader here has to tidy a stored answer up.
	 */
	public static function isSentenceComplete( object $sentence ): bool {
		return trim( $sentence->text ?? '' ) !== '' && trim( $sentence->translation ?? '' ) !== '';
	}

	/**
	 * One sentence with its translation. Only a sentence that has alternatives
	 * opens, so the ones without stay a plain row: an arrow onto an empty panel
	 * is an invitation to a click that shows nothing. The arrow that remains
	 * sits at the trailing edge, clear of the text, so every sentence starts on
	 * the same line down the left whether or not it expands.
	 */
	private function renderSentence( object $sentence ): string {
		$disabled = !empty( $sentence->disabled );
		$translation = $sentence->translation ?? '';
		$chevron = 'shrink-0 text-sm leading-snug text-[#72777d]';
		$alternatives =
			$this->renderAnswerGroup( 'Alternative sentences', $sentence->alternativeSentences ?? [] ) .
			$this->renderAnswerGroup( 'Alternative translations', $sentence->alternativeTranslations ?? [] );
		$row = static fn ( string $marker, string $extraClass ) => Html::rawElement( 'div', [ 'class' => 'flex items-start gap-3 px-4 py-3 md:pl-8' . $extraClass ],
			Html::rawElement( 'div', [ 'class' => 'min-w-0 flex-1' ],
				Html::element( 'span', [ 'class' => 'text-sm leading-snug' . ( $disabled ? ' line-through' : '' ) ], $sentence->text ?? '' ) .
				( $translation !== '' ? Html::element( 'small', [ 'class' => 'mt-1 block text-sm leading-snug text-[#54595d]' ], $translation ) : '' ) ) .
			( $disabled ? Html::element( 'span', [ 'class' => 'shrink-0 rounded-sm border border-[#c8ccd1] px-1.5 py-0.5 text-xs text-[#54595d]' ], 'Disabled' ) : '' ) .
			$marker );
		$border = 'border-b border-[#eaecf0]' . ( $disabled ? ' opacity-60' : '' );
		if ( $alternatives === '' ) {
			return Html::rawElement( 'li', [ 'class' => $border ], $row( '', '' ) );
		}
		return Html::rawElement( 'li', [ 'class' => $border ],
			Html::rawElement( 'details', [ 'class' => 'group' ],
				Html::rawElement( 'summary', [ 'class' => 'list-none [&::-webkit-details-marker]:hidden' ],
					// The extension ships no base layer, so no transform variables: swap glyphs instead of rotating one.
					$row( Html::element( 'span', [ 'class' => $chevron . ' group-open:hidden', 'aria-hidden' => 'true' ], '▼' ) .
						Html::element( 'span', [ 'class' => $chevron . ' hidden group-open:block', 'aria-hidden' => 'true' ], '▲' ),
						' cursor-pointer hover:bg-[#f8f9fa]' ) ) .
				Html::rawElement( 'div', [ 'class' => 'space-y-3 border-t border-dashed border-[#c8ccd1] bg-[#f8f9fa] px-4 py-3 md:pl-8' ],
					$alternatives ) ) );
	}

	/** @param string[] $texts */
	private function renderAnswerGroup( string $label, array $texts ): string {
		if ( !$texts ) {
			return '';
		}
		return Html::rawElement( 'div', [],
			Html::element( 'p', [ 'class' => 'm-0 mb-1 text-xs font-semibold uppercase tracking-wide text-[#54595d]' ], $label ) .
			Html::rawElement( 'ul', [ 'class' => 'm-0 list-none p-0' ], implode( '', array_map(
				static fn ( string $text ) => Html::element( 'li', [ 'class' => 'text-sm leading-snug' ], $text ),
				$texts ) ) ) );
	}

	/**
	 * The link to this skill's tips. Tips are optional, so the link is left red
	 * when there are none: on this wiki a red link is the invitation to write
	 * what is missing, and the tips editor opens straight from it.
	 */
	private function renderTipsLink( PageReference $page ): string {
		$tipsTitle = CourseName::sibling( NS_MIMI_TIPS, $page );
		if ( !$tipsTitle ) {
			return '';
		}
		$linkContents = Html::rawElement( 'span', [
			'class' => 'shrink-0 [&_svg]:h-4 [&_svg]:w-4',
			'aria-hidden' => 'true',
		], Icon::codex( 'cdxIconLightbulb' ) ) .
			Html::element( 'span', [], 'Tips for this skill' );
		return Html::element( 'dt', [ 'class' => 'text-sm font-semibold text-[#202122]' ], 'Tips' ) .
			Html::rawElement( 'dd', [ 'class' => 'm-0 text-sm' ],
				MediaWikiServices::getInstance()->getLinkRenderer()->makeLink(
					$tipsTitle,
					new HtmlArmor( $linkContents ),
					[ 'class' => 'inline-flex items-center gap-1' ] ) );
	}

	/**
	 * The link back to the course this skill belongs to, written whether or not
	 * the page is named by the course convention: a skill filed outside it still
	 * needs the row, the same way the glossary and tips pages carry one.
	 */
	private function renderCourseLink( string $courseName ): string {
		$courseTitle = 'Course:' . $courseName;
		return Html::element( 'dt', [ 'class' => 'text-sm font-semibold text-[#202122]' ], 'Course' ) .
			Html::rawElement( 'dd', [ 'class' => 'm-0 text-sm' ],
				Html::element( 'a', [
					'href' => wfScript( 'index' ) . '?title=' . rawurlencode( $courseTitle ),
				], $courseTitle ) );
	}

	protected function renderStructuredView( object $data, string $courseName, PageReference $page ): string {
		$words = '';
		$sentenceGroups = '';
		foreach ( $data->words ?? [] as $index => $word ) {
			// A half-written sentence is draft material: the editor lists it
			// without its checkmark, but the viewer presents only finished
			// teaching material, so it is left out of the count and the list.
			$shown = array_filter( $word->sentences ?? [],
				static fn ( $sentence ) => self::isSentenceComplete( $sentence ) );
			$words .= Html::rawElement( 'button', [
				'type' => 'button',
				'class' => 'flex w-full items-center gap-3 border-0 border-b border-l-4 border-[#eaecf0] px-4 py-3 text-left text-sm ' .
					( $index === 0 ? 'border-l-[#36c] bg-[#eaecf0]' : 'border-l-transparent bg-white hover:bg-[#f8f9fa]' ),
				'data-mimi-word' => '',
				'data-word-index' => $index,
			],
				Html::element( 'strong', [ 'class' => 'min-w-0 flex-1 truncate font-semibold' ], $word->word ) .
				Html::element( 'span', [ 'class' => 'ml-auto min-w-6 rounded-full bg-[#eaecf0] px-1.5 py-0.5 text-center text-xs text-[#54595d]' ], count( $shown ) ) );
			$sentences = '';
			foreach ( $shown as $sentence ) {
				$sentences .= $this->renderSentence( $sentence );
			}
			if ( $sentences === '' ) {
				$sentences = Html::element( 'li', [ 'class' => 'px-4 py-8 text-center text-sm text-[#72777d] md:pl-8' ], 'No sentences yet.' );
			}
			$sentenceGroups .= Html::rawElement( 'ol', [
				'class' => 'm-0 list-none p-0 ' . ( $index === 0 ? 'block' : 'hidden' ),
				'data-mimi-sentences' => '',
				'data-word-index' => $index,
			], $sentences );
		}
		return Html::rawElement( 'div', [
			'class' => 'my-6 max-w-5xl font-sans text-[#202122] [&_button:not(:disabled)]:cursor-pointer [&_h2]:font-sans',
			'data-mimi-skill-view' => '',
		],
			Html::rawElement( 'dl', [ 'class' => 'mb-6 grid gap-1 border-l-4 border-[#a2a9b1] bg-[#f8f9fa] px-4 py-3 sm:grid-cols-[8rem_minmax(0,1fr)] sm:gap-4' ],
				Html::element( 'dt', [ 'class' => 'text-sm font-semibold text-[#202122]' ], 'Grammar focus' ) .
				Html::element( 'dd', [ 'class' => 'm-0 text-sm leading-relaxed text-[#202122]' ], $data->grammarFocus ?? '' ) .
				$this->renderTipsLink( $page ) .
				$this->renderCourseLink( $courseName ) ) .
			Html::rawElement( 'div', [ 'class' => 'grid overflow-hidden rounded-sm border border-[#a2a9b1] bg-white md:grid-cols-[minmax(220px,0.8fr)_minmax(0,1.7fr)]' ],
				Html::rawElement( 'section', [],
					Html::element( 'h2', [ 'class' => 'm-0 border-0 border-b border-[#c8ccd1] bg-[#f8f9fa] px-4 py-3 text-sm font-semibold' ], 'Words taught' ) .
					Html::rawElement( 'div', [], $words ) ) .
				Html::rawElement( 'section', [ 'class' => 'border-t border-[#c8ccd1] md:border-l md:border-t-0' ],
					Html::element( 'h2', [ 'class' => 'm-0 border-0 border-b border-[#c8ccd1] bg-[#f8f9fa] px-4 py-3 text-sm font-semibold md:pl-8' ], 'Sentences' ) .
					$sentenceGroups )
			)
		);
	}

	protected function fillParserOutput(
		Content $content,
		ContentParseParams $cpoParams,
		ParserOutput &$parserOutput
	) {
		parent::fillParserOutput( $content, $cpoParams, $parserOutput );
		// Registered so that the tips page knows what points at it, and so the
		// link above is coloured from the same cache the rest of the wiki uses.
		$tipsTitle = CourseName::sibling( NS_MIMI_TIPS, $cpoParams->getPage() );
		if ( $cpoParams->getGenerateHtml() && $tipsTitle ) {
			$parserOutput->addLink( $tipsTitle );
		}
	}
}
