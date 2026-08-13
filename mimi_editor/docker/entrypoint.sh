#!/bin/sh
set -eu

server="${MIMI_SERVER:-http://mimi.localhost:4771}"
db_password="${MIMI_DB_PASSWORD:-mediawiki}"

# LocalSettings.php is mounted in from the host read-only, and is there before
# the wiki is, so its presence no longer distinguishes a fresh install from an
# existing one. Ask the database instead.
database_is_populated() {
  php -r '
$password = getenv( "MIMI_DB_PASSWORD" ) ?: "mediawiki";
$link = @mysqli_connect( "database", "mediawiki", $password, "mediawiki" );
if ( !$link ) { exit( 1 ); }
$result = mysqli_query( $link, "SHOW TABLES LIKE \"page\"" );
exit( $result && mysqli_num_rows( $result ) ? 0 : 1 );
' >/dev/null 2>&1
}

if ! database_is_populated; then
  # We only want the schema and the administrator account out of this run; the
  # LocalSettings.php it insists on generating is redundant. Pointing
  # MW_CONFIG_FILE at a path that does not exist makes the maintenance runner
  # bootstrap the way it would for a genuinely fresh wiki, ignoring the mounted
  # settings file, and --confpath sends the generated copy somewhere disposable.
  install_dir=/tmp/mimi-install
  mkdir -p "$install_dir"

  MW_CONFIG_FILE="$install_dir/LocalSettings.php" \
  php /var/www/html/maintenance/install.php \
    --dbtype=mysql \
    --dbserver=database \
    --dbname=mediawiki \
    --dbuser=mediawiki \
    --dbpass="$db_password" \
    --server="$server" \
    --scriptpath= \
    --lang=en \
    --pass="$MEDIAWIKI_ADMIN_PASSWORD" \
    --confpath="$install_dir" \
    "mimi editor" Admin

  rm -rf "$install_dir"
fi

# Write the front page without overwriting subsequent editor changes: the script
# leaves the wiki alone once the main page no longer holds the installer's
# placeholder.
php /var/www/html/extensions/MimiIncubator/maintenance/SeedMainPage.php

exec docker-php-entrypoint apache2-foreground
