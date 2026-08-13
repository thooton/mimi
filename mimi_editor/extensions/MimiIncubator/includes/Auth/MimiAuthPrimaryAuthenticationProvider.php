<?php

namespace MediaWiki\Extension\MimiIncubator\Auth;

use MediaWiki\Auth\AbstractPasswordPrimaryAuthenticationProvider;
use MediaWiki\Auth\AuthenticationRequest;
use MediaWiki\Auth\AuthenticationResponse;
use MediaWiki\Auth\AuthManager;
use MediaWiki\Auth\PasswordAuthenticationRequest;
use MediaWiki\Http\HttpRequestFactory;
use MediaWiki\Password\InvalidPassword;
use MediaWiki\User\UserRigorOptions;
use StatusValue;
use Wikimedia\Rdbms\IConnectionProvider;
use Wikimedia\Rdbms\IDBAccessObject;

/**
 * Sign-in against mimi_auth, the service that holds every mimi site's
 * credentials.
 *
 * The three mimi sites do not know about each other: the wiki, mimi_backend and
 * anything else all verify the same username and password against mimi_auth and
 * then keep their own session. That is not single sign-on — signing in here does
 * not sign anybody in over there — but one account and one password work
 * everywhere, which is the part that was actually wanted.
 *
 * This provider is deliberately **not** authoritative, and it sorts ahead of
 * core's LocalPasswordPrimaryAuthenticationProvider rather than replacing it.
 * Both facts matter:
 *
 *   - Sorting first means a mimi account is checked against mimi_auth before the
 *     wiki's own `user` table, so mimi_auth stays the one source of truth for
 *     anybody who has an account there.
 *   - Not being authoritative means a rejection here ABSTAINs instead of
 *     failing, so the wiki's local accounts — `Admin`, above all — fall through
 *     to the local password check and keep working. Core's provider is the
 *     authoritative last word, so a genuinely wrong password still ends in
 *     "wrong password" rather than something vaguer.
 *
 * mimi_auth compares usernames case-insensitively (`COLLATE NOCASE`), which is
 * what makes this safe at all: MediaWiki capitalises the first letter of every
 * username, so the wiki asks about `Mimi` for an account registered as `mimi`.
 */
final class MimiAuthPrimaryAuthenticationProvider extends AbstractPasswordPrimaryAuthenticationProvider {

	private HttpRequestFactory $httpRequestFactory;
	private IConnectionProvider $dbProvider;

	/**
	 * @param HttpRequestFactory $httpRequestFactory
	 * @param IConnectionProvider $dbProvider Only for telling a local account
	 *   apart from one that mimi_auth authenticates; see hasLocalPassword().
	 * @param array $params Passed to the parent; see the class comment for why
	 *   'authoritative' has to stay false.
	 */
	public function __construct(
		HttpRequestFactory $httpRequestFactory,
		IConnectionProvider $dbProvider,
		array $params = []
	) {
		parent::__construct( $params );
		$this->httpRequestFactory = $httpRequestFactory;
		$this->dbProvider = $dbProvider;
	}

	/** @inheritDoc */
	public function beginPrimaryAuthentication( array $reqs ) {
		$req = AuthenticationRequest::getRequestByClass( $reqs, PasswordAuthenticationRequest::class );
		if ( !$req || $req->username === null || $req->password === null ) {
			return AuthenticationResponse::newAbstain();
		}

		// mimi_auth accepts a username or an email address as the login, but a
		// wiki sign-in form only ever carries something shaped like a username,
		// so anything that is not a usable one is not ours to answer.
		$username = $this->userNameUtils->getCanonical( $req->username, UserRigorOptions::RIGOR_USABLE );
		if ( $username === false ) {
			return AuthenticationResponse::newAbstain();
		}

		[ $code, $body ] = $this->post( '/v1/login', [
			'login' => $username,
			'password' => $req->password,
		] );

		if ( $code === 200 ) {
			// AuthManager creates the local account itself when this name has
			// never signed in here before, which is why the wiki needs the
			// 'autocreateaccount' right for anonymous users.
			return AuthenticationResponse::newPass( $username );
		}

		if ( $code === 401 ) {
			return $this->failResponse( $req );
		}

		// mimi_auth is unreachable or broken. Abstaining rather than failing
		// keeps the local accounts usable while it is down — which is exactly
		// when somebody needs to sign in as Admin — at the cost of the sign-in
		// form blaming the password. The log line is the honest explanation.
		$this->logger->error( 'mimi_auth could not answer a sign-in for {user}: HTTP {code}', [
			'user' => $username,
			'code' => $code,
			'body' => $body['error'] ?? '',
		] );
		return AuthenticationResponse::newAbstain();
	}

	/** @inheritDoc */
	public function testUserExists( $username, $flags = IDBAccessObject::READ_NORMAL ) {
		// mimi_auth exposes no way to ask whether a name is taken without also
		// presenting its password, and inventing one would be a username oracle
		// on a service that deliberately does not have one. Saying "no" here is
		// the honest answer; the local user table still reports the accounts
		// that have signed in at least once.
		return false;
	}

	/** @inheritDoc */
	public function accountCreationType() {
		return self::TYPE_CREATE;
	}

	/** @inheritDoc */
	public function beginPrimaryAccountCreation( $user, $creator, array $reqs ) {
		$req = AuthenticationRequest::getRequestByClass( $reqs, PasswordAuthenticationRequest::class );
		if ( !$req || $req->password === null || $req->password === '' ) {
			return AuthenticationResponse::newAbstain();
		}

		// mimi_auth requires an address, because it is the other half of the
		// login it accepts, while MediaWiki treats the field as optional — so
		// the requirement has to be restated here. The address is read off the
		// User rather than out of $reqs: AuthManager applies
		// UserDataAuthenticationRequest with populateUser() before any primary
		// provider runs, and does not pass that request on, so $user is where
		// the address has been validated and put.
		$email = trim( $user->getEmail() );
		if ( $email === '' ) {
			return AuthenticationResponse::newFail( wfMessage( 'mimiincubator-auth-email-required' ) );
		}

		[ $code, $body ] = $this->post( '/v1/register', [
			'username' => $user->getName(),
			'email' => $email,
			'password' => $req->password,
		] );

		if ( $code === 201 ) {
			return AuthenticationResponse::newPass( $user->getName() );
		}

		// mimi_auth's own wording is more specific than anything restated here
		// could be — it is the side that knows the username charset, the length
		// bounds and which of the two fields was already taken.
		if ( $code === 400 || $code === 409 ) {
			return AuthenticationResponse::newFail(
				wfMessage( 'mimiincubator-auth-rejected' )
					->plaintextParams( $body['error'] ?? '' )
			);
		}

		$this->logger->error( 'mimi_auth could not answer a registration for {user}: HTTP {code}', [
			'user' => $user->getName(),
			'code' => $code,
		] );
		return AuthenticationResponse::newFail( wfMessage( 'mimiincubator-auth-unavailable' ) );
	}

	/** @inheritDoc */
	public function getAuthenticationRequests( $action, array $options ) {
		if ( $action === AuthManager::ACTION_CHANGE ) {
			return [ new MimiPasswordChangeRequest() ];
		}
		return parent::getAuthenticationRequests( $action, $options );
	}

	/** @inheritDoc */
	public function providerAllowsAuthenticationDataChange( AuthenticationRequest $req, $checkData = true ) {
		if ( $req instanceof MimiPasswordChangeRequest ) {
			return $this->checkPasswordChange( $req, $checkData );
		}

		// Core's own change request would write a password into the wiki's user
		// table, and for an account whose credentials live in mimi_auth that
		// second password would keep working after the Mimi one was changed or
		// the account disabled. Refusing it is what keeps there being one
		// password. Accounts that really are local — Admin — have a local
		// password already, and core stays in charge of theirs.
		if ( get_class( $req ) === PasswordAuthenticationRequest::class
			&& $req->username !== null
			&& !$this->hasLocalPassword( $req->username )
		) {
			return StatusValue::newFatal( 'mimiincubator-auth-change-elsewhere' );
		}

		return StatusValue::newGood( 'ignored' );
	}

	/** @inheritDoc */
	public function providerChangeAuthenticationData( AuthenticationRequest $req ) {
		if ( !$req instanceof MimiPasswordChangeRequest || $req->username === null ) {
			return;
		}

		$username = $this->userNameUtils->getCanonical( $req->username, UserRigorOptions::RIGOR_USABLE );
		if ( $username === false ) {
			return;
		}

		[ $code, $body ] = $this->post( '/v1/password', [
			'login' => $username,
			'current_password' => (string)$req->current_password,
			'new_password' => (string)$req->password,
		] );

		// This method cannot report a failure — core declares it void, and
		// AuthManager has already told the user the change went through. The
		// checks in providerAllowsAuthenticationDataChange run first and against
		// the same service, so the realistic way to arrive here is mimi_auth
		// going down in between. Say so in the log; there is nowhere else.
		if ( $code !== 200 ) {
			$this->logger->error( 'mimi_auth refused a password change for {user} that was already approved: HTTP {code}', [
				'user' => $username,
				'code' => $code,
				'body' => $body['error'] ?? '',
			] );
		}
	}

	/**
	 * Validate a password change, and prove the current password, before
	 * anything is written.
	 *
	 * All of the reporting has to happen here: providerChangeAuthenticationData
	 * returns void, so this is the only place a bad current password can still
	 * become an error message.
	 *
	 * @param MimiPasswordChangeRequest $req
	 * @param bool $checkData Whether the request carries data worth checking, or
	 *   is only being asked about in the abstract.
	 * @return StatusValue
	 */
	private function checkPasswordChange( MimiPasswordChangeRequest $req, bool $checkData ): StatusValue {
		if ( !$checkData ) {
			return StatusValue::newGood();
		}

		$username = $req->username !== null
			? $this->userNameUtils->getCanonical( $req->username, UserRigorOptions::RIGOR_USABLE )
			: false;
		if ( $username === false ) {
			return StatusValue::newGood( 'ignored' );
		}

		if ( $req->password === null || $req->password !== $req->retype ) {
			return StatusValue::newFatal( 'badretype' );
		}

		// The wiki's own policy first, so a password mimi_auth would refuse is
		// refused here in MediaWiki's wording. The two agree because
		// LocalSettings sets the policy from mimi_auth's bounds.
		//
		// Merged rather than tested with isOK(): a policy this wiki only
		// *suggests* — which is how MinimalPasswordLength arrives, since it
		// carries suggestChangeOnLogin — leaves isOK() true and isGood() false.
		// AuthManager blocks the change on isGood(), so merging is what makes a
		// six-character password a refusal here instead of a silent 400 from
		// mimi_auth after the user has been told the change succeeded.
		$sv = StatusValue::newGood();
		$sv->merge( $this->checkPasswordValidity( $username, $req->password ) );
		if ( !$sv->isGood() ) {
			return $sv;
		}

		// mimi_auth has no tokens, so retyping the current password is what
		// authorises the change. Verifying it through /v1/login rather than
		// trusting the session keeps that true even when MediaWiki considers
		// the session fresh enough to skip reauthentication.
		[ $code ] = $this->post( '/v1/login', [
			'login' => $username,
			'password' => (string)$req->current_password,
		] );
		if ( $code === 401 ) {
			return StatusValue::newFatal( 'mimiincubator-auth-change-wrongpassword' );
		}
		if ( $code !== 200 ) {
			$this->logger->error( 'mimi_auth could not answer a password change for {user}: HTTP {code}', [
				'user' => $username,
				'code' => $code,
			] );
			return StatusValue::newFatal( 'mimiincubator-auth-unavailable-change' );
		}

		return StatusValue::newGood();
	}

	/**
	 * Whether the wiki holds a usable password of its own for this name.
	 *
	 * Accounts created here before mimi_auth existed, and Admin, have one;
	 * accounts autocreated on a Mimi sign-in have an empty hash and are
	 * authenticated only by mimi_auth. That difference is what decides who may
	 * still change a password through core.
	 *
	 * @param string $username
	 * @return bool
	 */
	private function hasLocalPassword( string $username ): bool {
		$canonical = $this->userNameUtils->getCanonical( $username, UserRigorOptions::RIGOR_USABLE );
		if ( $canonical === false ) {
			return false;
		}
		$hash = $this->dbProvider->getReplicaDatabase()->newSelectQueryBuilder()
			->select( [ 'user_password' ] )
			->from( 'user' )
			->where( [ 'user_name' => $canonical ] )
			->caller( __METHOD__ )->fetchField();
		if ( $hash === false || $hash === null || $hash === '' ) {
			return false;
		}
		return !$this->getPassword( $hash ) instanceof InvalidPassword;
	}

	/**
	 * POST a JSON body to mimi_auth.
	 *
	 * Core's HttpRequestFactory::post() returns only the body, and every
	 * decision here turns on telling 401 from 409 from "no answer at all", so
	 * the request is built and executed directly instead.
	 *
	 * @param string $path
	 * @param array $body
	 * @return array{0:int,1:array} The HTTP status — 0 when the service could
	 *   not be reached — and the decoded response body.
	 */
	private function post( string $path, array $body ): array {
		$url = rtrim( $this->config->get( 'MimiAuthUrl' ), '/' ) . $path;
		$request = $this->httpRequestFactory->create( $url, [
			'method' => 'POST',
			'postData' => json_encode( $body ),
			'timeout' => 10,
		], __METHOD__ );
		$request->setHeader( 'Content-Type', 'application/json' );

		// A 4xx makes execute() return a non-OK Status, but the body and the
		// code are still on the request, and both are wanted here.
		$request->execute();
		$decoded = json_decode( (string)$request->getContent(), true );

		return [ (int)$request->getStatus(), is_array( $decoded ) ? $decoded : [] ];
	}
}
