<?php

namespace MediaWiki\Extension\MimiIncubator;

use MediaWiki\MediaWikiServices;
use MediaWiki\Page\PageReference;
use MediaWiki\Title\Title;

/**
 * A course is named after its language pair — "Spanish for English speakers" —
 * so the pair is never stored in the content. Course, glossary, and skill pages
 * all take their course from the page they sit on.
 */
final class CourseName {
	/** The course a page belongs to: its own name, minus any skill subpage. */
	public static function fromPage( ?PageReference $page ): string {
		$name = $page ? str_replace( '_', ' ', $page->getDBkey() ) : '';
		return trim( explode( '/', $name )[0] );
	}

	/** What a page is called beneath its course: a skill's name, or ''. */
	public static function subpageName( ?PageReference $page ): string {
		$name = $page ? str_replace( '_', ' ', $page->getDBkey() ) : '';
		return trim( substr( $name, strpos( $name, '/' ) === false ? strlen( $name ) : strpos( $name, '/' ) + 1 ) );
	}

	/**
	 * The page of the same name in another Mimi namespace. A skill and its tips
	 * are one page name in two namespaces — `Skill:<course>/<skill>` and
	 * `Tips:<course>/<skill>` — so neither has to store a pointer to the other.
	 */
	public static function sibling( int $namespace, ?PageReference $page ): ?Title {
		return $page ? Title::makeTitleSafe( $namespace, $page->getDBkey() ) : null;
	}

	/**
	 * The taught language and the language it is taught in. Both are empty when
	 * the page is not named as a course, which the language pair cannot outvote.
	 *
	 * @return array{0:string,1:string}
	 */
	public static function languages( string $courseName ): array {
		return preg_match( '/^(.+) for (.+) speakers$/', $courseName, $match )
			? [ trim( $match[1] ), trim( $match[2] ) ]
			: [ '', '' ];
	}

	/**
	 * The course page a language pair names, or null when the pair cannot name
	 * one: either language missing, or characters no page title may hold.
	 * Languages are capitalised, so "spanish" and "Spanish" reach one course
	 * rather than two.
	 */
	public static function titleFor( string $targetLanguage, string $sourceLanguage ): ?Title {
		$target = self::tidy( $targetLanguage );
		$source = self::tidy( $sourceLanguage );
		if ( $target === '' || $source === '' ) {
			return null;
		}
		return Title::makeTitleSafe( NS_MIMI_COURSE, "$target for $source speakers" );
	}

	/** A language as it is written in a course name, whatever the box holds. */
	private static function tidy( string $language ): string {
		$language = trim( preg_replace( '/\s+/', ' ', $language ) );
		return MediaWikiServices::getInstance()->getContentLanguage()->ucfirst( $language );
	}
}
