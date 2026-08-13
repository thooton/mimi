<?php

namespace MediaWiki\Extension\MimiIncubator\Auth;

use MediaWiki\Auth\AuthManager;
use MediaWiki\Auth\PasswordAuthenticationRequest;
use MediaWiki\SpecialPage\Hook\SpecialPageBeforeExecuteHook;
use MediaWiki\SpecialPage\SpecialPage;

/**
 * Send Special:ChangePassword to the Mimi form for the accounts that need it.
 *
 * Special:ChangePassword is core's shortcut to
 * Special:ChangeCredentials/MediaWiki\Auth\PasswordAuthenticationRequest, and
 * MimiAuthPrimaryAuthenticationProvider refuses that credential for accounts
 * whose password lives in mimi_auth. Left alone the shortcut therefore dead-ends
 * on "…is not a valid credential type" — a worse answer than the one the
 * provider took the trouble to write, and reached from a link in preferences
 * that people actually follow.
 *
 * Whether to redirect is not decided here. The provider is asked the same
 * question it would be asked anyway, so there is one rule about who has a local
 * password rather than two that can disagree.
 */
final class ChangePasswordRedirect implements SpecialPageBeforeExecuteHook {

	private AuthManager $authManager;

	public function __construct( AuthManager $authManager ) {
		$this->authManager = $authManager;
	}

	/**
	 * @param SpecialPage $special
	 * @param string|null $subPage
	 * @return bool False to stop the special page running, having redirected.
	 */
	public function onSpecialPageBeforeExecute( $special, $subPage ) {
		if ( $special->getName() !== 'ChangePassword' ) {
			return true;
		}

		// An anonymous visitor is on their way to the login form, which is a
		// better destination than either password form.
		$user = $special->getUser();
		if ( !$user->isRegistered() ) {
			return true;
		}

		$req = new PasswordAuthenticationRequest();
		$req->username = $user->getName();
		// $checkData false: this asks whether the local credential may be
		// changed at all, not whether some particular new password is any good.
		if ( $this->authManager->allowsAuthenticationDataChange( $req, false )->isGood() ) {
			return true;
		}

		$special->getOutput()->redirect(
			SpecialPage::getTitleFor( 'ChangeCredentials', 'MimiPassword' )->getFullURL()
		);
		return false;
	}
}
