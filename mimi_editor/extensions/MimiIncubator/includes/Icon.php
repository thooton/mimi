<?php

namespace MediaWiki\Extension\MimiIncubator;

use MediaWiki\Html\Html;
use MediaWiki\MediaWikiServices;
use MediaWiki\ResourceLoader\CodexModule;

/**
 * Server-rendered SVGs from MediaWiki's bundled, open-source Codex Icons, so
 * read views carry no icon assets of their own. The editor reads the same
 * icons through ResourceLoader instead.
 */
final class Icon {
	public static function codex( string $iconName, string $class = 'h-5 w-5' ): string {
		$services = MediaWikiServices::getInstance();
		$definition = CodexModule::getIcons( null, $services->getMainConfig(), [ $iconName ] )[$iconName];
		if ( is_array( $definition ) ) {
			$language = $services->getContentLanguage();
			$definition = $definition['langCodeMap'][$language->getCode()] ??
				$definition[$language->isRTL() ? 'rtl' : 'ltr'] ??
				$definition['ltr'] ?? $definition['default'] ?? '';
		}
		return Html::rawElement( 'svg', [
			'class' => $class,
			'viewBox' => '0 0 20 20',
			'width' => '20',
			'height' => '20',
			'fill' => 'currentColor',
			'aria-hidden' => 'true',
		], $definition );
	}
}
