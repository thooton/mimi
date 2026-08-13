<?php

namespace MediaWiki\Extension\MimiIncubator;

use MediaWiki\Content\Content;
use MediaWiki\Extension\MimiIncubator\Content\CourseLayoutContent;
use MediaWiki\Extension\MimiIncubator\Content\GlossaryContent;
use MediaWiki\Extension\MimiIncubator\Content\SkillContent;
use MediaWiki\Extension\MimiIncubator\Content\SkillContentHandler;
use MediaWiki\MediaWikiServices;
use MediaWiki\Revision\SlotRecord;
use MediaWiki\Title\Title;

/**
 * The courses this wiki publishes, read straight from the `Course:` pages.
 *
 * Summarising a course opens every skill it lists, so the results are held for
 * the rest of the request: the front page asks for the same catalogue once for
 * its totals and again for its listing.
 */
final class CourseCatalogue {
	/** @var CourseSummary[]|null */
	private static ?array $courses = null;

	/** @var array<string,?Content> Pages already read, by prefixed key. */
	private static array $contents = [];

	/** Every published course, by page title. @return CourseSummary[] */
	public static function courses(): array {
		if ( self::$courses !== null ) {
			return self::$courses;
		}
		$pages = MediaWikiServices::getInstance()->getPageStore()
			->newSelectQueryBuilder()
			->whereNamespace( NS_MIMI_COURSE )
			->orderByTitle()
			->caller( __METHOD__ )
			->fetchPageRecords();
		self::$courses = [];
		foreach ( $pages as $page ) {
			$summary = self::summarise( Title::newFromPageIdentity( $page ) );
			if ( $summary ) {
				self::$courses[] = $summary;
			}
		}
		return self::$courses;
	}

	/**
	 * How much teaching material the wiki holds altogether. Only content that
	 * a course lists is counted, so a draft skill nobody linked stays out.
	 *
	 * @return array{courses:int,skills:int,sentences:int,glossaryTerms:int}
	 */
	public static function totals(): array {
		$totals = [ 'courses' => 0, 'skills' => 0, 'sentences' => 0, 'glossaryTerms' => 0 ];
		foreach ( self::courses() as $course ) {
			$totals['courses']++;
			$totals['skills'] += $course->skills;
			$totals['sentences'] += $course->sentences;
			$totals['glossaryTerms'] += $course->glossaryTerms;
		}
		return $totals;
	}

	/**
	 * A skill's sentences, and how many of them are complete. A skill page that
	 * is missing or unreadable counts as empty rather than breaking the caller.
	 *
	 * @return array{0:int,1:int}
	 */
	public static function skillSentenceCounts( string $skillTitle ): array {
		try {
			$title = Title::newFromText( $skillTitle );
			$content = $title ? self::latestContent( $title ) : null;
			if ( !$content instanceof SkillContent ) {
				return [ 0, 0 ];
			}
			$total = 0;
			$complete = 0;
			foreach ( $content->getData()->getValue()->words ?? [] as $word ) {
				foreach ( $word->sentences ?? [] as $sentence ) {
					$total++;
					if ( SkillContentHandler::isSentenceComplete( $sentence ) ) {
						$complete++;
					}
				}
			}
			return [ $total, $complete ];
		} catch ( \Throwable ) {
			return [ 0, 0 ];
		}
	}

	/**
	 * A random handful of sentences the courses teach, for the front page to
	 * show off. Disabled sentences and half-written ones are left out: these
	 * are meant to be examples of the wiki at its best.
	 *
	 * @return list<array{text:string,translation:string,skill:Title}>
	 */
	public static function exampleSentences( int $limit ): array {
		$examples = [];
		foreach ( self::courses() as $course ) {
			$content = self::latestContent( $course->title );
			if ( !$content instanceof CourseLayoutContent ) {
				continue;
			}
			foreach ( $content->getData()->getValue()->skills ?? [] as $skill ) {
				$skillTitle = Title::newFromText( $skill );
				$skillContent = $skillTitle ? self::latestContent( $skillTitle ) : null;
				if ( !$skillContent instanceof SkillContent ) {
					continue;
				}
				foreach ( $skillContent->getData()->getValue()->words ?? [] as $word ) {
					foreach ( $word->sentences ?? [] as $sentence ) {
						if ( !empty( $sentence->disabled ) || !SkillContentHandler::isSentenceComplete( $sentence ) ) {
							continue;
						}
						$examples[] = [
							'text' => trim( $sentence->text ),
							'translation' => trim( $sentence->translation ),
							'skill' => $skillTitle,
						];
					}
				}
			}
		}
		shuffle( $examples );
		return array_slice( $examples, 0, $limit );
	}

	private static function summarise( ?Title $title ): ?CourseSummary {
		$content = $title ? self::latestContent( $title ) : null;
		if ( !$content instanceof CourseLayoutContent ) {
			return null;
		}
		$data = $content->getData()->getValue();
		$name = CourseName::fromPage( $title );
		[ $target, $source ] = CourseName::languages( $name );
		$sentences = 0;
		$complete = 0;
		foreach ( $data->skills ?? [] as $skill ) {
			[ $skillSentences, $skillComplete ] = self::skillSentenceCounts( $skill );
			$sentences += $skillSentences;
			$complete += $skillComplete;
		}
		return new CourseSummary(
			$title,
			$name,
			$target,
			$source,
			count( $data->skills ?? [] ),
			count( $data->rows ?? [] ),
			$sentences,
			$complete,
			self::glossaryTerms( $name )
		);
	}

	/**
	 * A course glossary is named after its course, and need not exist yet.
	 *
	 * A glossary too large for one page is spread over `Glossary:<course>/…`
	 * segments, so the count is of the root page and every segment beneath it:
	 * a course whose words are filed under twenty-six letters holds them all
	 * the same, and a front page saying otherwise would be counting pages
	 * rather than words.
	 */
	private static function glossaryTerms( string $courseName ): int {
		$root = Title::newFromText( 'Glossary:' . $courseName );
		if ( !$root ) {
			return 0;
		}
		$titles = [ $root ];
		$segments = MediaWikiServices::getInstance()->getPageStore()
			->newSelectQueryBuilder()
			->whereTitlePrefix( $root->getNamespace(), $root->getDBkey() . '/' )
			->orderByTitle()
			->caller( __METHOD__ )
			->fetchPageRecords();
		foreach ( $segments as $segment ) {
			$titles[] = Title::newFromPageIdentity( $segment );
		}
		$terms = 0;
		foreach ( $titles as $title ) {
			$content = self::latestContent( $title );
			if ( $content instanceof GlossaryContent ) {
				$terms += count( $content->getData()->getValue()->entries ?? [] );
			}
		}
		return $terms;
	}

	/**
	 * The current content of a page, or null when it is absent or invalid.
	 * Held for the request: the totals, the listing and the example sentences
	 * all walk the same courses and skills.
	 */
	private static function latestContent( Title $title ): ?Content {
		$key = $title->getPrefixedDBkey();
		if ( array_key_exists( $key, self::$contents ) ) {
			return self::$contents[$key];
		}
		$content = null;
		if ( $title->exists() ) {
			$revision = MediaWikiServices::getInstance()->getWikiPageFactory()
				->newFromTitle( $title )->getRevisionRecord();
			$content = $revision ? $revision->getContent( SlotRecord::MAIN ) : null;
			$content = $content && $content->isValid() ? $content : null;
		}
		return self::$contents[$key] = $content;
	}
}
