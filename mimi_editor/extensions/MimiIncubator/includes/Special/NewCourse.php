<?php

namespace MediaWiki\Extension\MimiIncubator\Special;

use MediaWiki\Extension\MimiIncubator\CourseName;
use MediaWiki\Html\Html;
use MediaWiki\MainConfigNames;
use MediaWiki\MediaWikiServices;
use MediaWiki\SpecialPage\SpecialPage;

/**
 * Where naming a language pair takes you to its course.
 *
 * A course is named after the pair it bridges, so a learner who knows the pair
 * knows the page; this turns the two halves of that name into a box on the
 * front page. The course opens if it has been started, and its editor opens if
 * it has not, which is the same red link the rest of the wiki offers, reached
 * without having to know how a course is titled.
 *
 * The work happens here rather than in the browser so the box needs no
 * JavaScript: the form is a plain GET, and this page is what it submits to.
 */
final class NewCourse extends SpecialPage {
	public function __construct() {
		parent::__construct( 'NewCourse' );
	}

	protected function getGroupName() {
		return 'pages';
	}

	public function execute( $subPage ) {
		$this->setHeaders();
		$this->outputHeader();
		$out = $this->getOutput();
		$out->addModuleStyles( [ 'ext.mimiIncubator.frontpage' ] );

		$target = trim( $this->getRequest()->getText( 'target' ) );
		$source = trim( $this->getRequest()->getText( 'source' ) );
		// An untouched form is the page being visited rather than submitted.
		if ( $target === '' && $source === '' ) {
			$out->addHTML( self::form() );
			return;
		}

		$title = CourseName::titleFor( $target, $source );
		if ( !$title ) {
			$out->addHTML( self::form( $target, $source, $target === '' || $source === ''
				? 'Name both languages: the one the course teaches, and the one it teaches in.'
				: 'A course cannot be named that. Try a language name without punctuation.' ) );
			return;
		}

		// An unwritten course goes straight to its editor, the way a red link
		// does. A written one opens as a reader would find it.
		$out->redirect( $title->getFullURL( $title->exists() ? [] : [ 'action' => 'edit' ] ) );
	}

	/**
	 * The box itself: a course's name with its two languages left to fill in.
	 *
	 * The form submits with GET to the script rather than to this page's own
	 * URL, because a GET form drops whatever query string its action carries,
	 * and on a wiki without pretty URLs the page is the query string.
	 */
	public static function form( string $target = '', string $source = '', string $error = '' ): string {
		$fields =
			self::language( 'target', $target, 'Spanish', 'Language this course teaches' ) .
			Html::element( 'span', [], 'for' ) .
			self::language( 'source', $source, 'English', 'Language of the speakers it teaches' ) .
			Html::element( 'span', [], 'speakers' ) .
			Html::element( 'button', [ 'type' => 'submit', 'class' => 'mimi-newcourse-go' ], 'Go' );

		return Html::rawElement( 'form', [
			'class' => 'mimi-newcourse',
			'action' => MediaWikiServices::getInstance()->getMainConfig()->get( MainConfigNames::Script ),
			'method' => 'get',
		],
			Html::hidden( 'title', self::getTitleFor( 'NewCourse' )->getPrefixedDBkey() ) .
			Html::rawElement( 'div', [ 'class' => 'mimi-newcourse-name' ], $fields ) .
			( $error === '' ? '' : Html::element( 'p', [ 'class' => 'mimi-newcourse-error' ], $error ) )
		);
	}

	private static function language( string $name, string $value, string $placeholder, string $label ): string {
		return Html::element( 'input', [
			'type' => 'text',
			'name' => $name,
			'value' => $value,
			'class' => 'mimi-newcourse-language',
			'placeholder' => $placeholder,
			'aria-label' => $label,
			'autocomplete' => 'off',
			'size' => 12,
		] );
	}
}
