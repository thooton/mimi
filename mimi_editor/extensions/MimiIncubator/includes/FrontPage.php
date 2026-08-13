<?php

namespace MediaWiki\Extension\MimiIncubator;

use MediaWiki\Extension\MimiIncubator\Special\NewCourse;
use MediaWiki\Html\Html;
use MediaWiki\Language\Language;
use MediaWiki\MediaWikiServices;
use MediaWiki\Parser\Hook\ParserFirstCallInitHook;
use MediaWiki\Parser\Parser;
use MediaWiki\Parser\PPFrame;
use MediaWiki\Title\Title;
use Wikimedia\HtmlArmor\HtmlArmor;
use Wikimedia\Rdbms\SelectQueryBuilder;

/**
 * The tags a main page is built from: `<mimilearn>`, `<mimistats>`,
 * `<mimicourses>`, `<miminewcourse>`, `<mimisentences>` and `<mimiactivity>`.
 *
 * Each one fills a box the wikitext lays out, the way a MediaWiki main page has
 * always been assembled. The wording around them stays editable wikitext; only
 * the parts that have to keep up with the wiki — what is published, what it
 * teaches, who edited it — are read at parse time. Styling comes from
 * `resources/frontpage.css`, not Tailwind, because wikitext shares it.
 */
final class FrontPage implements ParserFirstCallInitHook {
	/** The listings go stale as courses are edited; recheck a few times an hour. */
	private const CACHE_SECONDS = 600;

	private const NAMESPACE_ICONS = [
		NS_MIMI_COURSE => 'cdxIconMapTrail',
		NS_MIMI_SKILL => 'cdxIconPuzzle',
		NS_MIMI_GLOSSARY => 'cdxIconBook',
	];

	public function onParserFirstCallInit( $parser ) {
		$parser->setHook( 'mimilearn', [ self::class, 'renderLearnerLink' ] );
		$parser->setHook( 'mimistats', [ self::class, 'renderStats' ] );
		$parser->setHook( 'mimicourses', [ self::class, 'renderCourses' ] );
		$parser->setHook( 'miminewcourse', [ self::class, 'renderNewCourse' ] );
		$parser->setHook( 'mimisentences', [ self::class, 'renderSentences' ] );
		$parser->setHook( 'mimiactivity', [ self::class, 'renderActivity' ] );
	}

	/** Point visitors at the separate site where published courses are taken. */
	public static function renderLearnerLink( ?string $input, array $args, Parser $parser, PPFrame $frame ): string {
		self::prepare( $parser );
		$url = MediaWikiServices::getInstance()->getMainConfig()->get( 'MimiLearnerUrl' );
		return Html::rawElement( 'p', [],
			'This is the course editing site. To take a course, ' .
			Html::element( 'a', [ 'href' => $url ], 'visit the learning site' ) . '.' );
	}

	/** Everything the wiki holds, counted, for the line under the banner. */
	public static function renderStats( ?string $input, array $args, Parser $parser, PPFrame $frame ): string {
		self::prepare( $parser );
		$totals = CourseCatalogue::totals();
		$items = '';
		foreach ( [
			[ $totals['courses'], 'course', 'courses' ],
			[ $totals['skills'], 'skill', 'skills' ],
			[ $totals['sentences'], 'sentence', 'sentences' ],
			[ $totals['glossaryTerms'], 'glossary term', 'glossary terms' ],
		] as [ $count, $singular, $plural ] ) {
			$items .= Html::rawElement( 'li', [],
				htmlspecialchars( self::quantity( $count, $singular, $plural ) ) );
		}
		return Html::rawElement( 'ul', [ 'class' => 'mimi-stats' ], $items );
	}

	/** Every published course, each on a card of its own. */
	public static function renderCourses( ?string $input, array $args, Parser $parser, PPFrame $frame ): string {
		self::prepare( $parser );
		$courses = CourseCatalogue::courses();
		if ( !$courses ) {
			return Html::element( 'p', [ 'class' => 'mimi-empty' ],
				'No courses have been published yet. A course lives at a page named for its '
				. 'language pair, such as Course:Spanish for English speakers.' );
		}
		$entries = '';
		foreach ( $courses as $course ) {
			$parser->getOutput()->addLink( $course->title );
			$entries .= self::courseEntry( $course );
		}
		return Html::rawElement( 'ul', [ 'class' => 'mimi-courses' ], $entries );
	}

	/**
	 * The box that turns a language pair into the course it names, whether or
	 * not anybody has written that course yet. Special:NewCourse decides which
	 * of the two it is, so the box works with JavaScript switched off.
	 */
	public static function renderNewCourse( ?string $input, array $args, Parser $parser, PPFrame $frame ): string {
		self::prepare( $parser );
		return NewCourse::form();
	}

	/**
	 * A few sentences the courses actually teach, in the spirit of "Did you
	 * know". They are drawn at random, so the ten-minute cache rotates them.
	 */
	public static function renderSentences( ?string $input, array $args, Parser $parser, PPFrame $frame ): string {
		self::prepare( $parser );
		$examples = CourseCatalogue::exampleSentences( self::limit( $args, 4 ) );
		if ( !$examples ) {
			return Html::element( 'p', [ 'class' => 'mimi-empty' ],
				'No sentence has both its text and a translation yet.' );
		}
		$items = '';
		foreach ( $examples as $example ) {
			$parser->getOutput()->addLink( $example['skill'] );
			$items .= Html::rawElement( 'li', [],
				Html::element( 'span', [ 'class' => 'mimi-sentence' ], $example['text'] ) .
				' ' .
				Html::element( 'span', [ 'class' => 'mimi-translation' ], $example['translation'] ) .
				' ' .
				Html::rawElement( 'span', [ 'class' => 'mimi-source' ],
					'(from ' . self::link( $example['skill'], self::leafName( $example['skill'] ) ) . ')' ) );
		}
		return Html::rawElement( 'ul', [ 'class' => 'mimi-sentences' ], $items );
	}

	/** The structured pages edited most recently, newest first. */
	public static function renderActivity( ?string $input, array $args, Parser $parser, PPFrame $frame ): string {
		self::prepare( $parser );
		$language = MediaWikiServices::getInstance()->getContentLanguage();
		$entries = '';
		foreach ( self::recentlyUpdated( self::limit( $args, 8 ) ) as [ $title, $editor, $timestamp ] ) {
			$parser->getOutput()->addLink( $title );
			$entries .= self::activityEntry( $title, $editor, $timestamp, $language );
		}
		if ( $entries === '' ) {
			return Html::element( 'p', [ 'class' => 'mimi-empty' ], 'Nothing has been edited yet.' );
		}
		return Html::rawElement( 'ul', [ 'class' => 'mimi-activity' ], $entries );
	}

	/**
	 * One course as a card of its own: its name under the pair's flags, then
	 * how much of it there is. A card is scanned, not read, so the facts are
	 * a line of bare counts, not prose.
	 */
	private static function courseEntry( CourseSummary $course ): string {
		// The language pair is the course's whole name, so the flags sit inside
		// the link itself: Spain's before "Spanish", England's before "English
		// speakers". A course not named as a pair has nothing to flag.
		$label = $course->targetLanguage === ''
			? htmlspecialchars( $course->name )
			: Flag::forLanguage( $course->targetLanguage )
				. ' ' . htmlspecialchars( $course->targetLanguage ) . ' for '
				. Flag::forLanguage( $course->sourceLanguage )
				. ' ' . htmlspecialchars( $course->sourceLanguage ) . ' speakers';

		$facts = [];
		foreach ( [
			[ $course->skills, 'skill', 'skills' ],
			[ $course->glossaryTerms, 'glossary term', 'glossary terms' ],
			[ $course->sentences, 'sentence', 'sentences' ],
		] as [ $count, $singular, $plural ] ) {
			$facts[] = htmlspecialchars( self::quantity( $count, $singular, $plural ) );
		}

		return Html::rawElement( 'li', [],
			Html::rawElement( 'div', [ 'class' => 'mimi-course-title' ],
				self::rawLink( $course->title, $label ) ) .
			Html::rawElement( 'div', [ 'class' => 'mimi-course-meta' ], implode( ' · ', $facts ) ) );
	}

	private static function activityEntry( Title $title, string $editor, string $timestamp, Language $language ): string {
		$icon = self::NAMESPACE_ICONS[$title->getNamespace()] ?? 'cdxIconArticle';
		$label = self::leafName( $title );
		// A skill is a subpage of its course; a course and its glossary are
		// named for the course already, so naming it again would only repeat.
		$context = array_filter( [
			$language->getNsText( $title->getNamespace() ),
			$label === $title->getText() ? '' : CourseName::fromPage( $title ),
			$editor,
			$language->date( $timestamp, false ),
		] );
		return Html::rawElement( 'li', [],
			Icon::codex( $icon, 'mimi-activity-icon' ) .
			self::link( $title, $label ) .
			Html::element( 'span', [ 'class' => 'mimi-activity-meta' ], implode( ' · ', $context ) ) );
	}

	/**
	 * The structured pages whose latest revision is newest. One row per page
	 * rather than per edit, so a burst of edits cannot fill the list.
	 *
	 * @return list<array{0:Title,1:string,2:string}>
	 */
	private static function recentlyUpdated( int $limit ): array {
		$rows = MediaWikiServices::getInstance()->getConnectionProvider()->getReplicaDatabase()
			->newSelectQueryBuilder()
			->select( [ 'page_namespace', 'page_title', 'actor_name', 'rev_timestamp' ] )
			->from( 'page' )
			->join( 'revision', null, 'rev_id = page_latest' )
			->join( 'actor', null, 'actor_id = rev_actor' )
			->where( [ 'page_namespace' => array_keys( self::NAMESPACE_ICONS ) ] )
			->orderBy( 'rev_timestamp', SelectQueryBuilder::SORT_DESC )
			->limit( $limit )
			->caller( __METHOD__ )
			->fetchResultSet();
		$updates = [];
		foreach ( $rows as $row ) {
			$updates[] = [
				Title::makeTitle( (int)$row->page_namespace, $row->page_title ),
				$row->actor_name,
				$row->rev_timestamp,
			];
		}
		return $updates;
	}

	/** Every tag styles itself, and every tag's content ages out of the cache. */
	private static function prepare( Parser $parser ): void {
		$parser->getOutput()->addModuleStyles( [ 'ext.mimiIncubator.frontpage' ] );
		$parser->getOutput()->updateCacheExpiry( self::CACHE_SECONDS );
	}

	/** @param array<string,string> $args */
	private static function limit( array $args, int $default ): int {
		return max( 1, min( 20, (int)( $args['limit'] ?? $default ) ) );
	}

	/** "1 course", "9 skills" — counted the way the wiki's language counts. */
	private static function quantity( int $count, string $singular, string $plural ): string {
		$language = MediaWikiServices::getInstance()->getContentLanguage();
		return $language->formatNum( $count ) . ' ' . ( $count === 1 ? $singular : $plural );
	}

	/** Skills are subpages of their course, so only the leaf names them. */
	private static function leafName( Title $title ): string {
		return preg_replace( '#^.*/#', '', $title->getText() );
	}

	/** A plain link to a page, label escaped. */
	private static function link( Title $title, string $label ): string {
		return MediaWikiServices::getInstance()->getLinkRenderer()->makeLink( $title, $label );
	}

	/** A link whose label is markup of our own making — the flagged course name. */
	private static function rawLink( Title $title, string $html ): string {
		return MediaWikiServices::getInstance()->getLinkRenderer()
			->makeLink( $title, new HtmlArmor( $html ) );
	}
}
