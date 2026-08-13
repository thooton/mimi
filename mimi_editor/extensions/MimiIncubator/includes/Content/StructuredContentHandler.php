<?php

namespace MediaWiki\Extension\MimiIncubator\Content;

use MediaWiki\Content\Content;
use MediaWiki\Content\JsonContent;
use MediaWiki\Content\JsonContentHandler;
use MediaWiki\Content\Renderer\ContentParseParams;
use MediaWiki\Content\ValidationParams;
use MediaWiki\Extension\MimiIncubator\Action\StructuredEditAction;
use MediaWiki\Extension\MimiIncubator\CourseName;
use MediaWiki\Extension\MimiIncubator\Diff\StructuredSlotDiffRenderer;
use MediaWiki\Extension\MimiIncubator\SchemaValidator;
use MediaWiki\Html\Html;
use MediaWiki\Page\PageReference;
use MediaWiki\Parser\ParserOutput;
use StatusValue;

abstract class StructuredContentHandler extends JsonContentHandler {
	abstract protected function getSchemaFile(): string;
	abstract protected function getEditorKind(): string;

	/**
	 * The read view for one page of this model. The page itself is passed as
	 * well as its course, because a view may need to name pages beside it, a
	 * skill links to its tips, which share its name in another namespace.
	 */
	abstract protected function renderStructuredView( object $data, string $courseName, PageReference $page ): string;

	public function getActionOverrides() {
		return [ 'edit' => StructuredEditAction::class ];
	}

	public function validateSave( Content $content, ValidationParams $validationParams ) {
		$status = parent::validateSave( $content, $validationParams );
		if ( !$status->isOK() ) {
			return $status;
		}
		/** @var JsonContent $content */
		$schemaText = file_get_contents( dirname( __DIR__, 2 ) . '/schemas/' . $this->getSchemaFile() );
		$schema = json_decode( $schemaText );
		$errors = ( new SchemaValidator() )->validate( $content->getData()->getValue(), $schema );
		$errors = array_merge( $errors, $this->validateSemantics(
			$content->getData()->getValue(),
			CourseName::fromPage( $validationParams->getPageIdentity() )
		) );
		return $errors ? StatusValue::newFatal( 'mimi-invalid-schema', implode( '; ', $errors ) ) : $status;
	}

	/** @return string[] */
	protected function validateSemantics( object $data, string $courseName ): array {
		return [];
	}

	protected function getSlotDiffRendererWithOptions( \IContextSource $context, $options = [] ) {
		return new StructuredSlotDiffRenderer( $this->getEditorKind() );
	}

	protected function fillParserOutput(
		Content $content,
		ContentParseParams $cpoParams,
		ParserOutput &$parserOutput
	) {
		if ( !$cpoParams->getGenerateHtml() ) {
			$parserOutput->setRawText( null );
			return;
		}
		/** @var JsonContent $content */
		if ( !$content->isValid() ) {
			$parserOutput->setRawText( Html::errorBox( wfMessage( 'invalid-json-data' )->text() ) );
			return;
		}
		$parserOutput->setRawText( $this->renderStructuredView(
			$content->getData()->getValue(),
			CourseName::fromPage( $cpoParams->getPage() ),
			$cpoParams->getPage()
		) );
		$parserOutput->addModuleStyles( [ 'ext.mimiIncubator.styles' ] );
		$parserOutput->addModules( [ 'ext.mimiIncubator.view' ] );
	}
}
