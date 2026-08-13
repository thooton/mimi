<?php

use MediaWiki\CommentStore\CommentStoreComment;
use MediaWiki\Content\WikitextContent;
use MediaWiki\Maintenance\Maintenance;
use MediaWiki\Revision\SlotRecord;
use MediaWiki\Title\Title;
use MediaWiki\User\User;

require_once dirname( __DIR__, 3 ) . '/maintenance/Maintenance.php';

/**
 * Replace the installer's placeholder main page with Mimi's front page.
 *
 * The wording is ordinary wikitext so it can be rewritten on-wiki; the parts
 * that have to stay true to the content, the course cards, the totals, the
 * list of recent edits, come from the extension's parser tags.
 */
class SeedMainPage extends Maintenance {
	/** The text MediaWiki's installer leaves behind, and nobody wants to keep. */
	private const PLACEHOLDER = 'MediaWiki has been installed';

	public function __construct() {
		parent::__construct();
		$this->addDescription( "Write Mimi's front page over the installer's placeholder main page" );
		$this->addOption( 'force', 'Overwrite the main page even if it has been edited since' );
	}

	public function execute() {
		$title = Title::newMainPage();
		if ( !$this->shouldWrite( $title ) ) {
			$this->output( $title->getPrefixedText() . " has been written since; leaving it unchanged.\n" );
			return;
		}
		$page = $this->getServiceContainer()->getWikiPageFactory()->newFromTitle( $title );
		$updater = $page->newPageUpdater( User::newSystemUser( User::MAINTENANCE_SCRIPT_USER, [ 'steal' => true ] ) );
		$updater->setContent( SlotRecord::MAIN, new WikitextContent( $this->frontPage() ) );
		$updater->saveRevision(
			CommentStoreComment::newUnsavedComment( "Set up Mimi's front page" ),
			EDIT_SUPPRESS_RC
		);
		if ( !$updater->getStatus()->isOK() ) {
			$this->fatalError( 'Could not write ' . $title->getPrefixedText() );
		}
		$this->output( 'Wrote the front page to ' . $title->getPrefixedText() . ".\n" );
	}

	/**
	 * Only the installer's placeholder is ours to replace. Anything else on the
	 * main page is somebody's edit, and a container restart must not undo it.
	 */
	private function shouldWrite( Title $title ): bool {
		if ( $this->hasOption( 'force' ) || !$title->exists() ) {
			return true;
		}
		$page = $this->getServiceContainer()->getWikiPageFactory()->newFromTitle( $title );
		$revision = $page->getRevisionRecord();
		$content = $revision ? $revision->getContent( SlotRecord::MAIN ) : null;
		return $content instanceof WikitextContent
			&& str_contains( $content->getText(), self::PLACEHOLDER );
	}

	private function frontPage(): string {
		$courses = $this->url( 'Special:AllPages', 'namespace=' . NS_MIMI_COURSE );
		$skills = $this->url( 'Special:AllPages', 'namespace=' . NS_MIMI_SKILL );
		$glossaries = $this->url( 'Special:AllPages', 'namespace=' . NS_MIMI_GLOSSARY );

		return <<<WIKITEXT
			<div id="mimi-topbanner" class="mimi-box">
			<div id="mimi-welcome"><h1>Welcome to Mimi</h1>,</div>
			<div id="mimi-free">the language-learning courses that anyone can edit.</div>
			<mimilearn />
			<mimistats />
			</div>
			<div id="mimi-upper">
			<div id="mimi-left" class="mimi-box">
			<h2 class="mimi-h2">Courses</h2>
			<div id="mimi-courses"><mimicourses />
			A course is named for the pair of languages it bridges, so ''Spanish for English speakers'' and ''English for Spanish speakers'' are separate courses, each with sentences of its own.
			</div>
			<h2 class="mimi-h2">Start a course</h2>
			<div id="mimi-newcourse"><miminewcourse />
			Name a pair and you will be taken to that course, or to a blank one to write if nobody has started it yet.
			</div>
			<h2 class="mimi-h2">From the courses&nbsp;…</h2>
			<div id="mimi-sentences"><mimisentences limit="4" /></div>
			</div>
			<div id="mimi-right" class="mimi-box">
			<h2 class="mimi-h2">Recently updated</h2>
			<div id="mimi-activity"><mimiactivity limit="8" /></div>
			<h2 class="mimi-h2">How to help</h2>
			<div id="mimi-help">
			* '''Add a sentence.''' Open a skill, press '''Edit''', and give one of its words an example sentence with a translation. A sentence is ready to teach once it has both.
			* '''Offer alternatives.''' Most sentences can be said, or translated, more than one way. Recording the alternatives lets a learner be right without guessing your phrasing.
			* '''Fill in a glossary.''' Every word a course teaches deserves an entry, with the most useful translation first.
			* '''Start a course.''' Name a language pair in the box on the left, then write skills beneath the course at ''Skill:French for English speakers/Greetings''.
			</div>
			</div>
			</div>
			<div id="mimi-bottom" class="mimi-box">
			<h2 class="mimi-h2">Other areas of Mimi</h2>
			* '''[$courses Courses]''' – the skill trees learners work down.
			* '''[$skills Skills]''' – the words and example sentences themselves.
			* '''[$glossaries Glossaries]''' – every word a course teaches, with its translations.
			* '''[[Special:RecentChanges|Recent changes]]''' – everything edited lately, in full.
			* '''[[Special:Random/Skill|Random skill]]''' – somewhere to start reading.
			* '''[[Special:SpecialPages|Special pages]]''' – the wiki's own tools.
			</div>__NOTOC____NOEDITSECTION__
			WIKITEXT;
	}

	/** A link target the wiki resolves itself, so no server name is baked in. */
	private function url( string $page, string $query ): string {
		return '{{fullurl:' . $page . '|' . $query . '}}';
	}
}

$maintClass = SeedMainPage::class;
require_once RUN_MAINTENANCE_IF_MAIN;
