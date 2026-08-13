<?php

namespace MediaWiki\Extension\MimiIncubator\Content;

use MediaWiki\Content\JsonContent;

final class TipsContent extends JsonContent {
	public function __construct( string $text ) {
		parent::__construct( $text, 'mimi-tips' );
	}
}
