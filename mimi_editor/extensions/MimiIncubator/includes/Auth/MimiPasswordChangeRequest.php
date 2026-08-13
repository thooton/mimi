<?php

namespace MediaWiki\Extension\MimiIncubator\Auth;

use MediaWiki\Auth\AuthenticationRequest;
use MediaWiki\Message\Message;

/**
 * The fields for changing a Mimi password.
 *
 * This exists rather than reusing core's PasswordAuthenticationRequest for two
 * reasons, and both are load-bearing:
 *
 *   - mimi_auth needs the *current* password. It has no tokens, so retyping the
 *     password is what authorises replacing it, and core's change request
 *     carries only the new one. MediaWiki's own answer to that question is
 *     reauthentication, which proves who is asking but hands the provider
 *     nothing it could forward.
 *   - Core's LocalPasswordPrimaryAuthenticationProvider claims a change request
 *     with `get_class( $req ) === PasswordAuthenticationRequest::class`, an
 *     exact match that a subclass would not satisfy either. Being a separate
 *     class is therefore what stops core writing a local password beside the
 *     one mimi_auth holds — which was the whole problem worth fixing.
 */
final class MimiPasswordChangeRequest extends AuthenticationRequest {

	/** @var string|null The password being replaced. */
	public $current_password = null;

	/** @var string|null The password replacing it. */
	public $password = null;

	/** @var string|null Confirmation of the above. */
	public $retype = null;

	/**
	 * A short, stable id. The default would be this class's fully qualified
	 * name, which becomes the Special:ChangeCredentials subpage and would put
	 * namespace separators in the URL.
	 *
	 * @return string
	 */
	public function getUniqueId() {
		return 'MimiPassword';
	}

	/** @inheritDoc */
	public function getFieldInfo() {
		return [
			'current_password' => [
				'type' => 'password',
				'label' => wfMessage( 'oldpassword' ),
				'help' => wfMessage( 'mimiincubator-auth-currentpassword-help' ),
				'sensitive' => true,
			],
			'password' => [
				'type' => 'password',
				'label' => wfMessage( 'newpassword' ),
				'help' => wfMessage( 'mimiincubator-auth-newpassword-help' ),
				'sensitive' => true,
			],
			'retype' => [
				'type' => 'password',
				'label' => wfMessage( 'retypenew' ),
				'help' => wfMessage( 'mimiincubator-auth-retype-help' ),
				'sensitive' => true,
			],
		];
	}

	/** @inheritDoc */
	public function describeCredentials() {
		return [
			'provider' => wfMessage( 'mimiincubator-auth-credential-provider' ),
			'account' => new Message( 'mimiincubator-auth-credential-account', [ $this->username ] ),
		];
	}
}
