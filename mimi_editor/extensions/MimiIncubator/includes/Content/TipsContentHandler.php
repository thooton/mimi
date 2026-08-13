<?php

namespace MediaWiki\Extension\MimiIncubator\Content;

use MediaWiki\Content\Content;
use MediaWiki\Content\Renderer\ContentParseParams;
use MediaWiki\Extension\MimiIncubator\CourseName;
use MediaWiki\Extension\MimiIncubator\Icon;
use MediaWiki\Extension\MimiIncubator\Markdown;
use MediaWiki\Html\Html;
use MediaWiki\Page\PageReference;
use MediaWiki\Parser\ParserOutput;

/**
 * Tips are the short notes a skill shows before it is practised: what "buenos
 * días" literally means, when to stop saying it. They sit in their own
 * namespace rather than inside the skill because they are optional, are written
 * by different people at a different time, and would otherwise push the skill
 * schema into carrying prose it has no other use for.
 */
final class TipsContentHandler extends StructuredContentHandler {
	public function __construct() {
		parent::__construct( 'mimi-tips' );
	}
	protected function getContentClass() { return TipsContent::class; }
	protected function getSchemaFile(): string { return 'tips.schema.json'; }
	protected function getEditorKind(): string { return 'tips'; }
	public function makeEmptyContent() {
		return new TipsContent( '{"schemaVersion":1,"tips":[]}' );
	}

	protected function validateSemantics( object $data, string $courseName ): array {
		if ( !isset( $data->tips ) || !is_array( $data->tips ) ) {
			return [];
		}
		$titles = array_map(
			static fn ( $tip ) => is_object( $tip ) ? mb_strtolower( trim( $tip->title ?? '' ) ) : '',
			$data->tips );
		return count( $titles ) === count( array_unique( $titles ) )
			? []
			: [ 'tip titles must be unique within a skill' ];
	}

	/**
	 * Where a tip is shown. A tip with a lesson is put in front of the learner
	 * once, before that lesson begins. A tip without one is shown in no lesson
	 * at all: it waits behind the skill's tips button for a learner who goes
	 * looking. Both are worth stating on the page, because the difference is
	 * not visible in the tip itself.
	 */
	private function renderLessonBadge( object $tip ): string {
		$lesson = $tip->lesson ?? null;
		return Html::rawElement( 'span', [
			'class' => 'inline-flex shrink-0 items-center gap-1 rounded-sm border border-solid px-2 py-0.5 text-xs ' .
				( $lesson === null
					? 'border-[#c8ccd1] bg-[#f8f9fa] text-[#54595d]'
					: 'border-[#c8ccd1] bg-white font-semibold text-[#3366cc]' ),
		],
			Icon::codex( $lesson === null ? 'cdxIconLightbulb' : 'cdxIconClock', 'h-3.5 w-3.5 shrink-0' ) .
			Html::element( 'span', [], $lesson === null
				? 'Tips button only'
				: 'Before lesson ' . $lesson ) );
	}

	protected function renderStructuredView( object $data, string $courseName, PageReference $page ): string {
		$tips = $data->tips ?? [];
		$documents = '';
		foreach ( $tips as $tip ) {
			$body = Markdown::toHtml( $tip->body ?? '' );
			$documents .= Html::rawElement( 'article', [
				'class' => 'border border-solid border-[#a2a9b1] bg-white px-5 py-4',
			],
				Html::rawElement( 'div', [ 'class' => 'mb-2 flex flex-wrap items-baseline justify-between gap-3' ],
					Html::element( 'h2', [
						'class' => 'm-0 border-0 p-0 text-lg font-semibold leading-snug',
					], $tip->title ?? '' ) .
					$this->renderLessonBadge( $tip ) ) .
				Html::rawElement( 'div', [ 'class' => Markdown::PROSE_CLASSES ],
					$body !== '' ? $body
						: Html::element( 'p', [ 'class' => 'm-0 text-sm text-[#72777d]' ], 'This tip has not been written yet.' ) ) );
		}
		if ( $documents === '' ) {
			$documents = Html::element( 'p', [
				'class' => 'm-0 border border-dashed border-[#a2a9b1] bg-[#f8f9fa] px-6 py-10 text-center text-sm text-[#72777d]',
			], 'This skill has no tips yet. Edit this page to write the first one.' );
		}
		// A tips page named outside the course convention still needs its headers,
		// so the rows below are written whether or not the siblings resolve.
		$skillTitle = CourseName::sibling( NS_MIMI_SKILL, $page );
		$courseTitle = 'Course:' . $courseName;
		$row = static fn ( string $label, string $value ): string =>
			Html::element( 'dt', [ 'class' => 'text-sm font-semibold text-[#54595d]' ], $label ) .
			Html::rawElement( 'dd', [ 'class' => 'm-0 text-sm text-[#202122]' ], $value );
		$link = static fn ( string $target ): string => Html::element( 'a', [
			'href' => wfScript( 'index' ) . '?title=' . rawurlencode( $target ),
		], $target );
		return Html::rawElement( 'div', [
			'class' => 'my-6 max-w-4xl font-sans text-[#202122] [&_a]:cursor-pointer [&_h2]:font-sans',
		],
			Html::rawElement( 'dl', [
				'class' => 'mb-6 grid gap-1 border-l-4 border-[#a2a9b1] bg-[#f8f9fa] px-4 py-3 sm:grid-cols-[8rem_minmax(0,1fr)] sm:gap-4',
			],
				$row( 'Tips', count( $tips ) . ' ' . ( count( $tips ) === 1 ? 'document' : 'documents' ) ) .
				( $skillTitle ? $row( 'Skill', $link( $skillTitle->getPrefixedText() ) ) : '' ) .
				$row( 'Course', $link( $courseTitle ) ) ) .
			Html::rawElement( 'div', [ 'class' => 'grid gap-4' ], $documents ) );
	}

	protected function fillParserOutput(
		Content $content,
		ContentParseParams $cpoParams,
		ParserOutput &$parserOutput
	) {
		parent::fillParserOutput( $content, $cpoParams, $parserOutput );
		$skillTitle = CourseName::sibling( NS_MIMI_SKILL, $cpoParams->getPage() );
		if ( $cpoParams->getGenerateHtml() && $skillTitle ) {
			$parserOutput->addLink( $skillTitle );
		}
	}
}
