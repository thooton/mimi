<?php

namespace MediaWiki\Extension\MimiIncubator\Content;

use MediaWiki\Content\JsonContent;

final class CourseLayoutContent extends JsonContent {
	public function __construct( string $text ) {
		parent::__construct( $text, 'mimi-course-layout' );
	}
}
