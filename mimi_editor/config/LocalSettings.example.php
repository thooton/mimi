<?php
/**
 * The wiki's configuration, as this repository expects it.
 *
 * Copy this file to config/LocalSettings.php before the first start:
 *
 *     cp config/LocalSettings.example.php config/LocalSettings.php
 *
 * The copy is gitignored, because the keys at the bottom belong to one
 * installation rather than to the project. Compose mounts it read-only, and a
 * missing mount source becomes a root-owned directory rather than an error, so
 * the file has to exist before `docker compose up`.
 *
 * Everything above the keys is the shared configuration: change it here, so
 * that every clone of the repository runs the same wiki.
 */
if ( !defined( 'MEDIAWIKI' ) ) {
	exit;
}

$wgSitename = 'mimi editor';
$wgMetaNamespace = 'Mimi_editor';
$wgScriptPath = '';
// MIMI_PORT and MIMI_SERVER remain overridable, but their ports must agree.
$wgServer = getenv( 'MIMI_SERVER' ) ?: 'http://mimi.localhost:4771';
$wgMimiLearnerUrl = getenv( 'MIMI_LEARNER_URL' ) ?: 'http://localhost:4773';
// Production terminates TLS at its reverse proxy. The proxy must pass
// X-Forwarded-Proto: https so MediaWiki recognises the original request and
// does not redirect it back through the proxy forever.
$wgForceHTTPS = filter_var( getenv( 'MIMI_FORCE_HTTPS' ) ?: 'false', FILTER_VALIDATE_BOOLEAN );
$wgResourceBasePath = $wgScriptPath;
// The same photograph supplies both the compact icon and the legacy logo slot,
// so every bundled skin has a useful mark without maintaining two subtly
// different versions of the Mimi identity.
$wgLogos = [
	'1x' => "$wgResourceBasePath/mimi.jpg",
	'icon' => "$wgResourceBasePath/mimi.jpg",
];
$wgFavicon = "$wgResourceBasePath/favicon.png";

$wgEnableEmail = true;
$wgEnableUserEmail = true;
$wgEmergencyContact = '';
$wgPasswordSender = '';
$wgEnotifUserTalk = false;
$wgEnotifWatchlist = false;
$wgEmailAuthentication = true;

// Sign-in goes to mimi_auth, the credential service every Mimi site shares. The
// sites do not know about each other and each keeps its own session, so this is
// not single sign-on, one account and one password simply work on all of them.
// MimiIncubator's provider sorts ahead of core's local password check and is
// deliberately not authoritative, so the wiki's own accounts (Admin above all)
// still sign in normally, including while mimi_auth is unreachable.
$wgMimiAuthUrl = getenv( 'MIMI_AUTH_URL' ) ?: 'http://host.docker.internal:4770';
// A Mimi account that has never been here has no row in the user table, and
// AuthManager only creates one on a successful sign-in if this is granted.
// Without it every first sign-in fails with "auto-creation failed".
$wgGroupPermissions['*']['autocreateaccount'] = true;
// Match mimi_auth's own bounds, so a password it would refuse is refused here
// first, in MediaWiki's wording, rather than after a pointless round trip.
$wgPasswordPolicy['policies']['default']['MinimalPasswordLength']['value'] = 8;
$wgPasswordPolicy['policies']['default']['MaximalPasswordLength']['value'] = 1024;
// AuthManager's own channel, sent to the container's stderr so it reaches
// `docker compose logs mediawiki`. Without this the logs go nowhere, and an
// unreachable mimi_auth looks exactly like a mistyped password, the sign-in
// form says "wrong password" and nothing anywhere says otherwise.
$wgDebugLogGroups['authentication'] = 'php://stderr';

$wgDBtype = 'mysql';
$wgDBserver = 'database';
$wgDBname = 'mediawiki';
$wgDBuser = 'mediawiki';
$wgDBpassword = getenv( 'MIMI_DB_PASSWORD' ) ?: 'mediawiki';
$wgDBprefix = '';
$wgDBssl = false;
$wgDBTableOptions = 'ENGINE=InnoDB, DEFAULT CHARSET=binary';
$wgSharedTables[] = 'actor';

$wgMainCacheType = CACHE_ACCEL;
$wgMemCachedServers = [];
$wgEnableUploads = false;
$wgUseImageMagick = true;
$wgImageMagickConvertCommand = '/usr/bin/convert';
$wgUseInstantCommons = false;
$wgPingback = false;
$wgLanguageCode = 'en';
$wgLocaltimezone = 'UTC';
// A glossary segment is a couple of thousand rows of JSON (the letter C of a
// five-thousand-word Spanish glossary is close to two megabytes on its own)
// and core's default of 2048 KB would refuse the largest of them. Segments are
// what keeps a glossary out of one impossible page in the first place; this is
// the headroom that stops the busiest letter from being the one nobody can
// save.
$wgMaxArticleSize = 4096;
// One installation's keys, which is why the copy stays out of the repository.
// These placeholders are fine for a wiki nothing but localhost can reach.
// Replace them with real ones before it is reachable by anything else:
//
//     php -r 'echo bin2hex( random_bytes( 32 ) ), "\n", bin2hex( random_bytes( 8 ) ), "\n";'
//
$wgSecretKey = getenv( 'MIMI_SECRET_KEY' )
	?: '0000000000000000000000000000000000000000000000000000000000000000';
$wgAuthenticationTokenVersion = '1';
$wgUpgradeKey = getenv( 'MIMI_UPGRADE_KEY' ) ?: '0000000000000000';
// Course material and every other user contribution share one explicit license.
// Keep RightsPage empty so MediaWiki uses the canonical external license URL in
// edit notices, page footers and API metadata rather than pointing at a local
// page whose text could drift from the license it is meant to describe.
$wgRightsPage = '';
$wgRightsUrl = 'https://creativecommons.org/licenses/by-nc-sa/4.0/';
$wgRightsText = 'Creative Commons Attribution-NonCommercial-ShareAlike 4.0 International';
$wgRightsIcon = "$wgResourceBasePath/resources/assets/licenses/cc-by-nc-sa.png";
$wgDiff3 = '/usr/bin/diff3';
$wgDefaultSkin = 'vector-2022';
$wgDefaultUserOptions['vector-appearance-pinned'] = 0;

wfLoadSkin( 'MinervaNeue' );
wfLoadSkin( 'MonoBook' );
wfLoadSkin( 'Timeless' );
wfLoadSkin( 'Vector' );
wfLoadExtension( 'MimiIncubator' );
