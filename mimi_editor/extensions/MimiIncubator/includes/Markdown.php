<?php

namespace MediaWiki\Extension\MimiIncubator;

use MediaWiki\Html\Html;

/**
 * The small Markdown subset a tip is written in.
 *
 * Tips are edited in a formatting canvas, not by typing syntax, so this subset
 * exists to give the canvas something plain to store: a revision has to diff as
 * text in page history, and JSON holding a blob of HTML would neither diff
 * legibly nor be safe to render back. Markdown keeps both, the stored body is
 * readable on its own, and nothing outside this grammar can survive a round
 * trip through the editor.
 *
 * The grammar is deliberately closed. Blocks are separated by blank lines and
 * are one of `## heading`, `- bullet`, `1. numbered`, or a paragraph. Inline
 * runs are `bold`, `italic`, `<u>underline</u>` and `[label](url)`, and a
 * backslash escapes the character after it. Underline is the one borrowing from
 * Markdown's inline HTML, because Markdown itself has no underline and the
 * editor offers one.
 *
 * `resources/editor.js` carries a matching renderer, because the canvas has to
 * show the same document the read view will. Change one and change the other.
 */
final class Markdown {
	/** Links may only point where a reader can safely follow: the web, or this wiki. */
	private const SAFE_LINK = '#^(?:https?://|/|\#)#i';

	/**
	 * Read-view and editor-canvas styling for the tags below. Tips render as
	 * bare elements so that both surfaces can share one set of rules, the
	 * canvas builds its elements with execCommand and cannot be given classes.
	 */
	public const PROSE_CLASSES = '[&_h3]:mb-1 [&_h3]:mt-4 [&_h3]:border-0 [&_h3]:p-0 [&_h3]:font-sans ' .
		'[&_h3]:text-base [&_h3]:font-semibold [&_h3:first-child]:mt-0 ' .
		'[&_p]:my-2 [&_p]:text-sm [&_p]:leading-relaxed [&_p:first-child]:mt-0 [&_p:last-child]:mb-0 ' .
		'[&_ul]:my-2 [&_ul]:list-disc [&_ol]:my-2 [&_ol]:list-decimal [&_ul]:pl-6 [&_ol]:pl-6 ' .
		'[&_li]:text-sm [&_li]:leading-relaxed ' .
		'[&_strong]:font-semibold [&_em]:italic [&_u]:underline [&_a]:text-[#3366cc]';

	/** One tip body as HTML. Anything outside the subset is shown as written. */
	public static function toHtml( string $text ): string {
		$html = '';
		$paragraph = [];
		$items = [];
		$listTag = '';
		// A trailing blank line closes whatever block the last real line opened.
		foreach ( array_merge( preg_split( '/\R/', $text ), [ '' ] ) as $line ) {
			$line = trim( $line );
			$bullet = preg_match( '/^-\s+(.*)$/', $line, $match ) ? $match[1] : null;
			$number = preg_match( '/^\d+\.\s+(.*)$/', $line, $match ) ? $match[1] : null;
			$heading = preg_match( '/^#{1,6}\s+(.*)$/', $line, $match ) ? $match[1] : null;
			$wanted = $bullet !== null ? 'ul' : ( $number !== null ? 'ol' : '' );
			if ( $items && $wanted !== $listTag ) {
				$html .= Html::rawElement( $listTag, [], implode( '', $items ) );
				$items = [];
			}
			$listTag = $wanted;
			if ( $paragraph && ( $wanted !== '' || $heading !== null || $line === '' ) ) {
				$html .= Html::rawElement( 'p', [], self::inline( implode( ' ', $paragraph ) ) );
				$paragraph = [];
			}
			if ( $wanted !== '' ) {
				$items[] = Html::rawElement( 'li', [], self::inline( $bullet ?? $number ) );
			} elseif ( $heading !== null ) {
				// Tip titles are already h2, so a heading inside a tip sits below one.
				$html .= Html::rawElement( 'h3', [], self::inline( $heading ) );
			} elseif ( $line !== '' ) {
				$paragraph[] = $line;
			}
		}
		return $html;
	}

	/** A tip body with its markup taken off, for summaries and previews. */
	public static function toPlainText( string $text ): string {
		$plain = html_entity_decode( strip_tags( self::toHtml( $text ) ), ENT_QUOTES );
		return trim( preg_replace( '/\s+/', ' ', $plain ) );
	}

	/**
	 * Inline runs within one block. Walks the text rather than replacing
	 * patterns in it, so that an escaped marker is never mistaken for a real
	 * one and the text between markers is escaped exactly once.
	 */
	private static function inline( string $text ): string {
		$html = '';
		$plain = '';
		$length = strlen( $text );
		$index = 0;
		while ( $index < $length ) {
			if ( $text[$index] === '\\' && $index + 1 < $length ) {
				$plain .= $text[$index + 1];
				$index += 2;
				continue;
			}
			$run = self::run( substr( $text, $index ) );
			if ( $run === null ) {
				$plain .= $text[$index];
				$index++;
				continue;
			}
			$html .= htmlspecialchars( $plain ) . $run[0];
			$plain = '';
			$index += $run[1];
		}
		return $html . htmlspecialchars( $plain );
	}

	/**
	 * The formatted run opening the given text, as HTML and the number of bytes
	 * it consumed, or null when the text does not open one.
	 * @return array{0:string,1:int}|null
	 */
	private static function run( string $text ): ?array {
		// The closing  may not be followed by a third asterisk: "a *b***"
		// ends with an italic run inside the bold one, and stopping at the first
		// pair would close the bold early and strand the odd asterisk.
		if ( preg_match( '/^\*\*((?:\\\\.|[^\\\\])+?)\*\*(?!\*)/s', $text, $match ) ) {
			return [ Html::rawElement( 'strong', [], self::inline( $match[1] ) ), strlen( $match[0] ) ];
		}
		if ( preg_match( '/^\*((?:\\\\.|[^\\\\*])+?)\*/s', $text, $match ) ) {
			return [ Html::rawElement( 'em', [], self::inline( $match[1] ) ), strlen( $match[0] ) ];
		}
		if ( preg_match( '#^<u>(.+?)</u>#s', $text, $match ) ) {
			return [ Html::rawElement( 'u', [], self::inline( $match[1] ) ), strlen( $match[0] ) ];
		}
		if ( preg_match( '/^\[((?:\\\\.|[^\\\\\]])+?)\]\(([^()\s]*)\)/s', $text, $match ) &&
			preg_match( self::SAFE_LINK, $match[2] )
		) {
			// Tips are wiki content written by contributors, so their outgoing
			// links carry the same nofollow the rest of the wiki's would.
			return [ Html::rawElement( 'a', [ 'href' => $match[2], 'rel' => 'nofollow' ],
				self::inline( $match[1] ) ), strlen( $match[0] ) ];
		}
		return null;
	}
}
