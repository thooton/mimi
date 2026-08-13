<?php

namespace MediaWiki\Extension\MimiIncubator\Content;

use HtmlArmor;
use MediaWiki\Content\Content;
use MediaWiki\Content\Renderer\ContentParseParams;
use MediaWiki\Extension\MimiIncubator\CourseCatalogue;
use MediaWiki\Extension\MimiIncubator\Icon;
use MediaWiki\Html\Html;
use MediaWiki\MediaWikiServices;
use MediaWiki\Page\PageReference;
use MediaWiki\Parser\ParserOutput;
use MediaWiki\Title\Title;

final class CourseLayoutContentHandler extends StructuredContentHandler {
	public function __construct() { parent::__construct( 'mimi-course-layout' ); }
	protected function getContentClass() { return CourseLayoutContent::class; }
	protected function getSchemaFile(): string { return 'course-layout.schema.json'; }
	protected function getEditorKind(): string { return 'course'; }
	public function makeEmptyContent() {
		return new CourseLayoutContent( '{"schemaVersion":5,"skills":[],"rows":[],"castles":[]}' );
	}
	protected function validateSemantics( object $data, string $courseName ): array {
		if ( !isset( $data->skills, $data->rows, $data->castles ) || !is_array( $data->skills ) || !is_array( $data->rows ) || !is_array( $data->castles ) ) { return []; }
		$placed = [];
		foreach ( $data->rows as $row ) { if ( is_array( $row ) ) { $placed = array_merge( $placed, $row ); } }
		$errors = [];
		$prefix = 'Skill:' . $courseName . '/';
		foreach ( $data->skills as $skill ) {
			if ( !str_starts_with( $skill, $prefix ) || trim( substr( $skill, strlen( $prefix ) ) ) === '' ) {
				$errors[] = 'skills must be namespaced as ' . $prefix . 'Skill name';
				break;
			}
		}
		if ( count( $placed ) !== count( array_unique( $placed ) ) ) { $errors[] = 'each skill may appear in only one row'; }
		$missing = array_diff( $data->skills, $placed );
		$unknown = array_diff( $placed, $data->skills );
		if ( $missing ) { $errors[] = 'every listed skill must appear in a row'; }
		if ( $unknown ) { $errors[] = 'rows may only contain listed skills'; }
		$last = 0;
		foreach ( $data->castles as $castle ) {
			if ( isset( $castle->afterRow ) && ( $castle->afterRow <= $last || $castle->afterRow > count( $data->rows ) ) ) { $errors[] = 'castle boundaries must be increasing valid row numbers'; break; }
			$last = $castle->afterRow ?? $last;
		}
		return $errors;
	}

	/**
	 * Percentage of a skill's sentences that are complete (text plus a
	 * translation). Returns 0 when the skill page does not exist yet.
	 */
	public static function skillCompletion( string $skillTitle ): int {
		[ $total, $complete ] = CourseCatalogue::skillSentenceCounts( $skillTitle );
		return $total === 0 ? 0 : (int)round( $complete / $total * 100 );
	}

	/** Select an icon name using the shared course/editor keyword rules. */
	private static function skillIconName( string $skillTitle ): string {
		static $rules = null;
		if ( $rules === null ) {
			$path = dirname( __DIR__, 2 ) . '/resources/skill-icons.json';
			$rules = json_decode( file_get_contents( $path ), true );
		}
		$label = mb_strtolower( preg_replace( '#^Skill:(?:.*/)?#', '', $skillTitle ) );
		foreach ( $rules as $rule ) {
			foreach ( $rule['terms'] as $term ) {
				if ( str_contains( $label, $term ) ) {
					return $rule['icon'];
				}
			}
		}
		return 'cdxIconPuzzle';
	}

	public static function skillIcon( string $skillTitle ): string {
		return Icon::codex( self::skillIconName( $skillTitle ) );
	}

	/** Castles are unnamed; they are numbered by their order down the tree. */
	private static function castleNumbers( object $data ): array {
		$numbers = [];
		foreach ( $data->castles ?? [] as $index => $castle ) { $numbers[$castle->afterRow] = $index + 1; }
		return $numbers;
	}

	protected function renderStructuredView( object $data, string $courseName, PageReference $page ): string {
		$castles = self::castleNumbers( $data );
		$linkRenderer = MediaWikiServices::getInstance()->getLinkRenderer();
		$tree = '';
		$rowIndexWithinCastle = 0;
		foreach ( $data->rows ?? [] as $index => $row ) {
			$skills = '';
			foreach ( $row as $skill ) {
				$label = preg_replace( '#^Skill:(?:.*/)?#', '', $skill );
				$title = Title::newFromText( $skill );
				$linkContents =
					Html::rawElement( 'span', [
						'class' => 'shrink-0 text-[#54595d] [&_svg]:h-5 [&_svg]:w-5',
						'aria-hidden' => 'true',
					], self::skillIcon( $skill ) ) .
					Html::element( 'strong', [ 'class' => 'min-w-0 flex-1 truncate text-sm font-semibold leading-tight group-hover:underline group-focus:underline' ], $label ) .
					Html::rawElement( 'span', [
						'class' => 'shrink-0 text-[#72777d] [&_svg]:h-4 [&_svg]:w-4',
						'aria-hidden' => 'true',
					], Icon::codex( 'cdxIconNext' ) );
				$linkAttributes = [
					'class' => 'group flex w-64 max-w-full items-center gap-3 border border-solid border-[#a2a9b1] bg-white px-4 py-3 no-underline transition-colors duration-100 hover:border-[#72777d] hover:bg-[#f8f9fa] focus:border-[#36c] focus:shadow-[inset_0_0_0_1px_#36c] focus:outline focus:outline-1 focus:outline-transparent active:border-[#202122] active:bg-[#eaecf0]',
				];
				$skills .= $title
					? $linkRenderer->makeLink( $title, new HtmlArmor( $linkContents ), $linkAttributes )
					: Html::rawElement( 'span', $linkAttributes, $linkContents );
			}
			$rowBackground = $rowIndexWithinCastle % 2 === 0 ? 'bg-white' : 'bg-[#f8f9fa]';
			$tree .= Html::rawElement( 'section', [ 'class' => 'py-3 ' . $rowBackground ],
				Html::rawElement( 'div', [ 'class' => 'flex flex-wrap items-stretch justify-center gap-3' ], $skills ) );
			$rowIndexWithinCastle++;
			$rowNo = $index + 1;
			if ( isset( $castles[$rowNo] ) ) {
				$tree .= Html::rawElement( 'div', [ 'class' => 'my-3 flex items-center gap-3' ],
					Html::rawElement( 'span', [ 'class' => 'h-px flex-1 bg-[#c8ccd1]' ], '' ) .
					Html::rawElement( 'span', [ 'class' => 'flex shrink-0 items-center gap-2 text-sm font-semibold uppercase tracking-wide text-[#54595d]' ],
						Icon::codex( 'cdxIconFlag', 'h-4 w-4 shrink-0' ) .
						Html::element( 'span', [], 'Castle ' . $castles[$rowNo] ) ) .
					Html::rawElement( 'span', [ 'class' => 'h-px flex-1 bg-[#c8ccd1]' ], '' ) );
				$rowIndexWithinCastle = 0;
			}
		}
		if ( $tree === '' ) {
			$tree = Html::element( 'p', [ 'class' => 'border border-dashed border-[#a2a9b1] bg-[#f8f9fa] p-8 text-center text-[#54595d]' ], 'No skills have been added to this course yet.' );
		}
		$glossaryTitle = 'Glossary:' . $courseName;
		$stat = static fn ( string $value, string $label ): string => Html::rawElement( 'div', [ 'class' => 'flex flex-col' ],
			Html::element( 'dd', [ 'class' => 'm-0 text-2xl font-semibold leading-tight' ], $value ) .
			Html::element( 'dt', [ 'class' => 'text-sm text-[#54595d]' ], $label ) );
		return Html::rawElement( 'div', [ 'class' => 'my-6 max-w-6xl font-sans text-[#202122] [&_a]:cursor-pointer [&_h2]:font-sans' ],
			Html::rawElement( 'div', [ 'class' => 'grid items-start gap-8 lg:grid-cols-[minmax(0,1fr)_280px]' ],
				Html::rawElement( 'main', [],
					Html::element( 'h2', [ 'class' => 'mb-4 border-b border-[#a2a9b1] pb-2 text-xl font-semibold' ], 'Course skills' ) .
					Html::rawElement( 'div', [ 'class' => 'relative' ], $tree ) ) .
				Html::rawElement( 'aside', [ 'class' => 'border border-[#c8ccd1] bg-[#f8f9fa] p-5' ],
					Html::element( 'p', [ 'class' => 'mb-1 mt-0 text-xs font-semibold uppercase tracking-wide text-[#54595d]' ], 'Course' ) .
					Html::element( 'h2', [ 'class' => 'm-0 border-0 p-0 text-lg font-semibold leading-snug' ], $courseName ) .
					Html::rawElement( 'dl', [ 'class' => 'my-5 grid grid-cols-2 gap-4 border-y border-[#c8ccd1] py-4' ],
						$stat( (string)count( $data->skills ?? [] ), 'Skills' ) .
						$stat( (string)count( $data->rows ?? [] ), 'Rows' ) ) .
					Html::rawElement( 'a', [
						'class' => 'inline-flex items-center gap-1 text-sm',
						'href' => wfScript( 'index' ) . '?title=' . rawurlencode( $glossaryTitle ),
					], Icon::codex( 'cdxIconBook' ) . Html::element( 'span', [], $glossaryTitle ) ) ) ) );
	}

	protected function fillParserOutput(
		Content $content,
		ContentParseParams $cpoParams,
		ParserOutput &$parserOutput
	) {
		parent::fillParserOutput( $content, $cpoParams, $parserOutput );
		if ( !$cpoParams->getGenerateHtml() || !$content->isValid() ) {
			return;
		}
		foreach ( $content->getData()->getValue()->skills ?? [] as $skill ) {
			$title = Title::newFromText( $skill );
			if ( $title ) {
				$parserOutput->addLink( $title );
			}
		}
	}
}
