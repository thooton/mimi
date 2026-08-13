<?php

namespace MediaWiki\Extension\MimiIncubator\Action;

use MediaWiki\Actions\EditAction;
use MediaWiki\Exception\PermissionsError;
use MediaWiki\Extension\MimiIncubator\Content\CourseLayoutContentHandler;
use MediaWiki\Extension\MimiIncubator\CourseName;
use MediaWiki\Html\Html;
use MediaWiki\MainConfigNames;
use MediaWiki\MediaWikiServices;
use MediaWiki\Revision\RevisionStore;
use MediaWiki\Revision\SlotRecord;
use MediaWiki\Title\Title;
use MediaWiki\User\ExternalUserNames;

final class StructuredEditAction extends EditAction {
	public function show() {
		$this->useTransactionalTimeLimit();
		$out = $this->getOutput();
		$out->setRobotPolicy( 'noindex,nofollow' );
		$out->disableClientCache();
		$request = $this->getRequest();
		$undo = $request->getInt( 'undo' );
		$undoAfter = $request->getInt( 'undoafter' );
		if ( $undo > 0 && $undoAfter > 0 ) {
			// The structured editor owns action=edit, so core never gets the
			// opportunity to turn an undo link into its dedicated confirmation
			// form. That form keeps the proposed content immutable, renders it
			// through this model's structured diff renderer, and asks the author
			// to confirm the summary before publishing the revert. McrUndoAction
			// does not generate EditPage's automatic summary itself, so carry the
			// same localized text into its wpSummary field at the handoff.
			$params = [
				'action' => 'mcrundo',
				'undo' => $undo,
				'undoafter' => $undoAfter,
			];
			$summary = $this->undoSummary( $undo, $undoAfter );
			if ( $summary !== '' ) {
				$params['wpSummary'] = $summary;
			}
			$out->redirect( $this->getTitle()->getFullURL( $params ) );
			return;
		}
		// On narrow screens every structured editor replaces the wiki chrome with
		// its own workspace, so mark the body before ResourceLoader paints.
		$out->addBodyClasses( 'mimi-structured-edit' );
		$title = $this->getTitle();
		if ( !$this->getAuthority()->isAllowed( 'edit' ) ) {
			throw new PermissionsError( 'edit' );
		}
		$handler = $this->getArticle()->getPage()->getContentHandler();
		$revision = $this->getArticle()->getPage()->getRevisionRecord();
		$content = $revision ? $revision->getContent( SlotRecord::MAIN ) : $handler->makeEmptyContent();
		$model = $handler->getModelID();
		$out->setPageTitle( $title->exists() ? 'Edit ' . $title->getPrefixedText() : 'Create ' . $title->getPrefixedText() );
		// Every stored page is at the current schema version, so the editor is
		// handed its content as it stands. There is no upgrade step here and no
		// reader anywhere that accepts an older shape: the versions the wiki
		// passed through were migrated once and are gone.
		$data = json_decode( $content->getText(), true );
		$courseName = CourseName::fromPage( $title );
		$skillStats = [];
		$skillExists = [];
		if ( $model === 'mimi-course-layout' ) {
			foreach ( $data['skills'] ?? [] as $skill ) {
				$skillStats[$skill] = CourseLayoutContentHandler::skillCompletion( $skill );
				$skillTitle = Title::newFromText( $skill );
				$skillExists[$skill] = $skillTitle !== null && $skillTitle->exists();
			}
		}
		$out->addJsConfigVars( 'mimiEditorConfig', [
			'title' => $title->getPrefixedText(),
			'model' => $model,
			'kind' => match ( $model ) {
				'mimi-skill' => 'skill',
				'mimi-glossary' => 'glossary',
				'mimi-tips' => 'tips',
				default => 'course',
			},
			'displayName' => $title->getText(),
			'courseName' => $courseName,
			'baseRevisionId' => $revision ? $revision->getId() : 0,
			'content' => $data,
			'skillStats' => $skillStats,
			'skillExists' => $skillExists,
		] );
		$out->addModules( [ 'ext.mimiIncubator.editor' ] );
		$out->addHTML( Html::rawElement( 'div', [ 'id' => 'mimi-editor-root' ], Html::element( 'p', [], 'Loading structured editor…' ) ) );
	}

	/**
	 * Reproduce EditPage's undo autosummary for the MCR confirmation form.
	 *
	 * Core's ordinary editor fills this before it redirects to an undo preview,
	 * but structured content has to use McrUndoAction and that action otherwise
	 * leaves wpSummary empty. The message choice and parameters deliberately
	 * match EditPage::generateUndoEditSummary(), including the less common
	 * hidden, imported and multiple-revision cases.
	 */
	private function undoSummary( int $undo, int $undoAfter ): string {
		$services = MediaWikiServices::getInstance();
		$revisionStore = $services->getRevisionStore();
		$title = $this->getTitle();
		$oldRevision = $revisionStore->getRevisionByTitle( $title, $undoAfter );
		$undoRevision = $revisionStore->getRevisionByTitle( $title, $undo );
		if ( $oldRevision === null || $undoRevision === null ) {
			return '';
		}

		$firstRevision = $revisionStore->getNextRevision( $oldRevision );
		if ( $firstRevision === null ) {
			return '';
		}

		if ( $firstRevision->getId() !== $undo ) {
			$count = $revisionStore->countRevisionsBetween(
				$firstRevision->getPageId(),
				$firstRevision,
				$undoRevision,
				null,
				[ RevisionStore::INCLUDE_BOTH, RevisionStore::INCLUDE_DELETED_REVISIONS ]
			);
			return $this->getContext()->msg( 'undo-summary-multiple' )
				->numParams( $count )
				->params( $firstRevision->getId(), $undoRevision->getId() )
				->inContentLanguage()
				->text();
		}

		$user = $undoRevision->getUser();
		if ( $user === null ) {
			return $this->getContext()->msg( 'undo-summary-username-hidden', $undo )
				->inContentLanguage()
				->text();
		}

		$userText = $user->getName();
		if ( ExternalUserNames::isExternal( $userText ) ) {
			$userLinkTitle = ExternalUserNames::getUserLinkTitle( $userText );
			if ( $userLinkTitle !== null ) {
				return $this->getContext()->msg(
					'undo-summary-import',
					$undo,
					$userLinkTitle->getPrefixedText(),
					$userText
				)->inContentLanguage()->text();
			}
			return $this->getContext()->msg( 'undo-summary-import2', $undo, $userText )
				->inContentLanguage()
				->text();
		}

		$anonymousTalkDisabled = !$user->isRegistered() &&
			$services->getMainConfig()->get( MainConfigNames::DisableAnonTalk );
		$message = $anonymousTalkDisabled
			? 'undo-summary-anon'
			: 'undo-summary';
		return $this->getContext()->msg( $message, $undo, $userText )
			->inContentLanguage()
			->text();
	}
}
