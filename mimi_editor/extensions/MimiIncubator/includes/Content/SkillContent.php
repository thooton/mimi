<?php

namespace MediaWiki\Extension\MimiIncubator\Content;

use MediaWiki\Content\JsonContent;

final class SkillContent extends JsonContent {
	public function __construct( string $text ) {
		parent::__construct( $text, 'mimi-skill' );
	}
}
