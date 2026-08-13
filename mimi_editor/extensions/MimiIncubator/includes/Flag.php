<?php

namespace MediaWiki\Extension\MimiIncubator;

use MediaWiki\Html\Html;

/**
 * The flag a language is known by, drawn as inline SVG.
 *
 * A page of courses is read by its colours before it is read by its titles,
 * which is why every course-listing tool in this field puts flags on the cards.
 * Languages are not countries, though, so the rule here is narrow: a language
 * gets the flag of the place it is named after, Spanish for Spain, English
 * for England, Portuguese for Portugal, and a language named after no one
 * place gets no flag rather than a contentious one.
 *
 * The drawings are our own and deliberately plain: each is a 30×20 box shown at
 * roughly the height of a capital letter, so emblems are cut back to the shapes
 * that survive at that size. Adding a language is adding a row to COUNTRIES and
 * a design beside it; a language with neither shows a blank flag, which keeps
 * the titles lined up without inventing a country for it.
 */
final class Flag {
	/** Lower-cased language name to the country whose flag it is drawn with. */
	private const COUNTRIES = [
		'castilian' => 'es',
		'catalan' => 'cat',
		'chinese' => 'cn',
		'czech' => 'cz',
		'danish' => 'dk',
		'dutch' => 'nl',
		'english' => 'gb-eng',
		'esperanto' => 'eo',
		'finnish' => 'fi',
		'french' => 'fr',
		'german' => 'de',
		'greek' => 'gr',
		'hebrew' => 'il',
		'hindi' => 'in',
		'hungarian' => 'hu',
		'icelandic' => 'is',
		'indonesian' => 'id',
		'irish' => 'ie',
		'italian' => 'it',
		'japanese' => 'jp',
		'korean' => 'kr',
		'mandarin' => 'cn',
		'norwegian' => 'no',
		'polish' => 'pl',
		'portuguese' => 'pt',
		'romanian' => 'ro',
		'russian' => 'ru',
		'scottish gaelic' => 'gb-sct',
		'spanish' => 'es',
		'swedish' => 'se',
		'thai' => 'th',
		'turkish' => 'tr',
		'ukrainian' => 'ua',
		'vietnamese' => 'vn',
	];

	/** The flag of the country a language is named after, or a blank one. */
	public static function forLanguage( string $language, string $class = 'mimi-flag' ): string {
		$code = self::COUNTRIES[strtolower( trim( $language ) )] ?? null;
		$design = $code === null ? null : self::designs()[$code] ?? null;
		return Html::rawElement( 'svg', [
			'class' => $design === null ? $class . ' mimi-flag-blank' : $class,
			'viewBox' => '0 0 30 20',
			'width' => '30',
			'height' => '20',
			'aria-hidden' => 'true',
		], $design ?? self::path( '#f8f9fa', 'M0 0h30v20H0z' ) );
	}

	/**
	 * Every flag we can draw, by country code. Built rather than written out
	 * because most of them are stripes, and a list of stripes says what a flag
	 * is far more plainly than the rectangles it comes down to.
	 *
	 * @return array<string,string>
	 */
	private static function designs(): array {
		static $designs = null;
		return $designs ??= [
			'cat' => self::rows(
				'#fcdd09', '#da121a', '#fcdd09', '#da121a', '#fcdd09',
				'#da121a', '#fcdd09', '#da121a', '#fcdd09'
			),
			'cn' => self::china(),
			'cz' => self::rows( '#fff', '#d7141a' ) . self::path( '#11457e', 'M0 0l12 10L0 20z' ),
			'de' => self::rows( '#000', '#dd0000', '#ffce00' ),
			'dk' => self::cross( '#c8102e', '#fff', 4.5, 11 ),
			// A green field, and the star sits in the white canton.
			'eo' => self::path( '#009900', 'M0 0h30v20H0z' ) . self::path( '#fff', 'M0 0h10v6.7H0z' )
				. self::path( '#009900', self::star( 5, 3.35, 2.4 ) ),
			// The middle band is twice the depth of the two red ones.
			'es' => self::path( '#aa151b', 'M0 0h30v20H0z' ) . self::path( '#f1bf00', 'M0 5h30v10H0z' ),
			'fi' => self::cross( '#fff', '#003580', 5, 11 ),
			'fr' => self::columns( '#002654', '#fff', '#ce1126' ),
			'gb-eng' => self::cross( '#fff', '#ce1124', 5, 15 ),
			'gb-sct' => self::path( '#005eb8', 'M0 0h30v20H0z' ) . Html::element( 'path', [
				'stroke' => '#fff',
				'stroke-width' => '4',
				'fill' => 'none',
				'd' => 'M0 0L30 20M30 0L0 20',
			] ),
			// Nine stripes and a canton, at the depths the real flag uses.
			'gr' => self::rows(
				'#0d5eaf', '#fff', '#0d5eaf', '#fff', '#0d5eaf', '#fff', '#0d5eaf', '#fff', '#0d5eaf'
			) . self::path( '#0d5eaf', 'M0 0h11.1v11.1H0z' )
				. self::path( '#fff', 'M4.45 0h2.2v11.1h-2.2zM0 4.45h11.1v2.2H0z' ),
			'hu' => self::rows( '#ce2939', '#fff', '#477050' ),
			'id' => self::rows( '#ce1126', '#fff' ),
			'ie' => self::columns( '#169b62', '#fff', '#ff883e' ),
			// Two triangles crossed, which is all the star of David needs to be.
			'il' => self::path( '#fff', 'M0 0h30v20H0z' )
				. self::path( '#0038b8', 'M0 2.6h30v2H0zM0 15.4h30v2H0z' )
				. Html::element( 'path', [
					'stroke' => '#0038b8',
					'stroke-width' => '0.9',
					'fill' => 'none',
					'd' => 'M15 6.4l3.4 5.9h-6.8zM15 13.6l-3.4-5.9h6.8z',
				] ),
			'in' => self::rows( '#ff9933', '#fff', '#138808' ) . Html::element( 'path', [
				'stroke' => '#000080',
				'stroke-width' => '0.4',
				'fill' => 'none',
				'd' => 'M15 7a3 3 0 100 6 3 3 0 100-6M12 10h6M15 7v6M12.9 7.9l4.2 4.2M12.9 12.1l4.2-4.2',
			] ),
			'is' => self::cross( '#02529c', '#fff', 6, 11 ) . self::cross( null, '#dc1e35', 2.4, 11 ),
			'it' => self::columns( '#009246', '#fff', '#ce2b37' ),
			'jp' => self::path( '#fff', 'M0 0h30v20H0z' )
				. Html::element( 'circle', [ 'cx' => '15', 'cy' => '10', 'r' => '6', 'fill' => '#bc002d' ] ),
			'kr' => self::korea(),
			'nl' => self::rows( '#ae1c28', '#fff', '#21468b' ),
			'no' => self::cross( '#ba0c2f', '#fff', 6, 11 ) . self::cross( null, '#00205b', 2.4, 11 ),
			'pl' => self::rows( '#fff', '#dc143c' ),
			// Green two fifths, red three, and the armillary sphere as a disc.
			'pt' => self::path( '#006600', 'M0 0h30v20H0z' ) . self::path( '#ff0000', 'M12 0h18v20H12z' )
				. Html::element( 'circle', [ 'cx' => '12', 'cy' => '10', 'r' => '4', 'fill' => '#ffe900' ] )
				. Html::element( 'circle', [ 'cx' => '12', 'cy' => '10', 'r' => '2', 'fill' => '#ff0000' ] ),
			'ro' => self::columns( '#002b7f', '#fcd116', '#ce1126' ),
			'ru' => self::rows( '#fff', '#0039a6', '#d52b1e' ),
			'se' => self::cross( '#006aa7', '#fecc00', 4.5, 11 ),
			// Red, white, and a blue band twice the depth of the rest.
			'th' => self::rows( '#a51931', '#fff', '#2d2a4a', '#fff', '#a51931' )
				. self::path( '#2d2a4a', 'M0 6.7h30v6.6H0z' ),
			'tr' => self::path( '#e30a17', 'M0 0h30v20H0z' )
				. Html::element( 'circle', [ 'cx' => '12', 'cy' => '10', 'r' => '5', 'fill' => '#fff' ] )
				. Html::element( 'circle', [ 'cx' => '13.8', 'cy' => '10', 'r' => '4', 'fill' => '#e30a17' ] )
				. self::path( '#fff', self::star( 19.5, 10, 2.6 ) ),
			'ua' => self::rows( '#0057b7', '#ffd700' ),
			'vn' => self::path( '#da251d', 'M0 0h30v20H0z' ) . self::path( '#ff0', self::star( 15, 10, 6 ) ),
		];
	}

	/**
	 * Horizontal bands, top to bottom. Each is drawn all the way to the bottom
	 * edge and the next paints over it, so no rounding can open a seam between
	 * two bands. `columns()` does the same rightwards.
	 */
	private static function rows( string ...$colours ): string {
		$svg = '';
		foreach ( $colours as $index => $colour ) {
			$y = round( $index * 20 / count( $colours ), 2 );
			$svg .= self::path( $colour, "M0 {$y}h30V20H0z" );
		}
		return $svg;
	}

	/** Vertical bands, hoist to fly. */
	private static function columns( string ...$colours ): string {
		$svg = '';
		foreach ( $colours as $index => $colour ) {
			$x = round( $index * 30 / count( $colours ), 2 );
			$svg .= self::path( $colour, "M{$x} 0h30v20H{$x}z" );
		}
		return $svg;
	}

	/**
	 * A cross reaching every edge: the Nordic ones, which sit left of centre,
	 * and England's, which does not. Passing no field draws the cross alone, so
	 * the outlined Norwegian and Icelandic crosses are two calls, not a case.
	 */
	private static function cross( ?string $field, string $colour, float $thickness, float $centre ): string {
		$x = round( $centre - $thickness / 2, 2 );
		$y = round( 10 - $thickness / 2, 2 );
		return ( $field === null ? '' : self::path( $field, 'M0 0h30v20H0z' ) )
			. self::path( $colour, "M{$x} 0h{$thickness}v20h-{$thickness}zM0 {$y}h30v{$thickness}H0z" );
	}

	/** The big star, and four small ones each turned to point at it. */
	private static function china(): string {
		$svg = self::path( '#de2910', 'M0 0h30v20H0z' ) . self::path( '#ffde00', self::star( 5, 5, 3 ) );
		foreach ( [ [ 10.5, 2 ], [ 12.5, 4.5 ], [ 12.5, 7.8 ], [ 10.5, 10.3 ] ] as [ $x, $y ] ) {
			$svg .= self::path( '#ffde00', self::star( $x, $y, 1.2, rad2deg( atan2( 5 - $x, $y - 5 ) ) ) );
		}
		return $svg;
	}

	/**
	 * The taegeuk, and the four trigrams as the bars they are made of, at this
	 * size the broken and unbroken bars are one grey smudge either way, so they
	 * are drawn unbroken.
	 */
	private static function korea(): string {
		$trigrams = '';
		foreach ( [ [ 5, 5, -56 ], [ 25, 5, 56 ], [ 5, 15, -124 ], [ 25, 15, 124 ] ] as [ $x, $y, $angle ] ) {
			$trigrams .= Html::rawElement( 'g', [ 'transform' => "rotate($angle $x $y)" ],
				Html::element( 'path', [
					'stroke' => '#000',
					'stroke-width' => '0.7',
					'fill' => 'none',
					'd' => 'M' . ( $x - 2 ) . ' ' . ( $y - 1.2 ) . 'h4M' . ( $x - 2 ) . " {$y}h4M"
						. ( $x - 2 ) . ' ' . ( $y + 1.2 ) . 'h4',
				] ) );
		}
		return self::path( '#fff', 'M0 0h30v20H0z' )
			. Html::element( 'circle', [ 'cx' => '15', 'cy' => '10', 'r' => '5', 'fill' => '#0047a0' ] )
			. self::path( '#cd2e3a', 'M10 10a2.5 2.5 0 0 1 5 0a2.5 2.5 0 0 0 5 0a5 5 0 0 0-10 0z' )
			. $trigrams;
	}

	/**
	 * A five-pointed star, upright unless turned. Computed rather than written
	 * out because ten corners of decimals say nothing about what they draw.
	 */
	private static function star( float $cx, float $cy, float $radius, float $rotation = 0.0 ): string {
		$points = [];
		for ( $corner = 0; $corner < 10; $corner++ ) {
			$reach = $corner % 2 === 0 ? $radius : $radius * 0.382;
			$angle = deg2rad( $rotation - 90 + $corner * 36 );
			$points[] = round( $cx + $reach * cos( $angle ), 2 ) . ' ' . round( $cy + $reach * sin( $angle ), 2 );
		}
		return 'M' . implode( 'L', $points ) . 'z';
	}

	private static function path( string $fill, string $d ): string {
		return Html::element( 'path', [ 'fill' => $fill, 'd' => $d ] );
	}
}
