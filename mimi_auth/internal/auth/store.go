package auth

import (
	"context"
	"database/sql"
	"errors"
	"fmt"
	"strings"
	"time"

	_ "modernc.org/sqlite"
)

var ErrUserExists = errors.New("username or email is already registered")

type User struct {
	ID           int64
	Username     string
	Email        string
	PasswordHash string
	CreatedAt    time.Time
}

type Store struct{ db *sql.DB }

func Open(path string) (*Store, error) {
	db, err := sql.Open("sqlite", path)
	if err != nil {
		return nil, err
	}
	db.SetMaxOpenConns(1)
	if _, err = db.Exec(`PRAGMA foreign_keys = ON; PRAGMA busy_timeout = 5000;`); err != nil {
		db.Close()
		return nil, err
	}
	// Decompress the common-password list now rather than on the first
	// registration, so a corrupt embedded copy stops the service at startup
	// instead of panicking inside a request.
	commonPasswords()
	if _, err = db.Exec(`CREATE TABLE IF NOT EXISTS users (
		id INTEGER PRIMARY KEY AUTOINCREMENT,
		-- NOCASE compares the ASCII alphabet case-insensitively, which is
		-- exactly the alphabet registration permits. SQLite still stores the
		-- spelling supplied here, so a name keeps its chosen capitalisation.
		username TEXT NOT NULL COLLATE NOCASE UNIQUE,
		email TEXT NOT NULL COLLATE NOCASE UNIQUE,
		password_hash TEXT NOT NULL,
		created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
	)`); err != nil {
		db.Close()
		return nil, err
	}
	// One row per outstanding "I forgot my password". Only the token's SHA-256
	// is kept, so a copy of this database is not a pile of working reset links;
	// see CreateReset for why a fast hash is the right one here.
	//
	// expires_at is unix seconds rather than a DATETIME because it is compared
	// rather than displayed, and an integer comparison cannot be thrown off by
	// the several string formats SQLite is willing to call a date. ON DELETE
	// CASCADE matters little today (nothing deletes accounts) but means a row
	// here can never outlive the account it resets.
	if _, err = db.Exec(`CREATE TABLE IF NOT EXISTS password_resets (
		id INTEGER PRIMARY KEY AUTOINCREMENT,
		user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
		token_hash TEXT NOT NULL UNIQUE,
		expires_at INTEGER NOT NULL,
		created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
	)`); err != nil {
		db.Close()
		return nil, err
	}
	return &Store{db: db}, nil
}

func (s *Store) Close() error { return s.db.Close() }

func (s *Store) CreateUser(ctx context.Context, username, email, passwordHash string) (User, error) {
	result, err := s.db.ExecContext(ctx, `INSERT INTO users(username,email,password_hash) VALUES(?,?,?)`, username, strings.ToLower(email), passwordHash)
	if err != nil {
		if strings.Contains(strings.ToLower(err.Error()), "unique constraint") {
			return User{}, ErrUserExists
		}
		return User{}, fmt.Errorf("insert user: %w", err)
	}
	id, _ := result.LastInsertId()
	return s.UserByID(ctx, id)
}

func (s *Store) UserByID(ctx context.Context, id int64) (User, error) {
	return scanUser(s.db.QueryRowContext(ctx, `SELECT id,username,email,password_hash,created_at FROM users WHERE id=?`, id))
}

func (s *Store) UserByLogin(ctx context.Context, login string) (User, error) {
	return scanUser(s.db.QueryRowContext(ctx, `SELECT id,username,email,password_hash,created_at FROM users WHERE username=? OR email=?`, login, strings.ToLower(login)))
}

// UpdatePassword replaces one user's hash. It takes the id rather than the
// login the caller supplied, so the row that is written is always the row whose
// current password was just verified.
func (s *Store) UpdatePassword(ctx context.Context, id int64, passwordHash string) error {
	if _, err := s.db.ExecContext(ctx, `UPDATE users SET password_hash=? WHERE id=?`, passwordHash, id); err != nil {
		return fmt.Errorf("update password: %w", err)
	}
	return nil
}

// UpdateEmail moves one user's address, by id for the same reason as above. The
// column is UNIQUE and an email is also a login, so an address already spoken
// for comes back as ErrUserExists rather than a database error.
func (s *Store) UpdateEmail(ctx context.Context, id int64, email string) (User, error) {
	if _, err := s.db.ExecContext(ctx, `UPDATE users SET email=? WHERE id=?`, strings.ToLower(email), id); err != nil {
		if strings.Contains(strings.ToLower(err.Error()), "unique constraint") {
			return User{}, ErrUserExists
		}
		return User{}, fmt.Errorf("update email: %w", err)
	}
	return s.UserByID(ctx, id)
}

// ErrResetInvalid covers every way a reset token can fail to be usable:
// never issued, already spent, or expired. They are deliberately one error,
// because the difference is not something the person holding the token can act
// on and telling them apart would say whether a token was ever real.
var ErrResetInvalid = errors.New("this reset link is invalid or has expired")

// CreateReset records a reset token for one account, replacing any earlier one.
//
// Only the token's hash arrives here; the caller keeps the token itself just
// long enough to mail it. A SHA-256 with no salt or stretching is right for
// this and wrong for a password: the token is 256 bits from crypto/rand, so
// there is no guessing to slow down, and the property actually wanted is that
// the lookup in ConsumeReset can be an indexed one.
//
// Replacing the previous row is what makes "email it to me again" safe to press
// twice: the older link stops working the moment a newer one is sent, so a
// message sitting in an inbox from an hour ago is not a second key.
func (s *Store) CreateReset(ctx context.Context, userID int64, tokenHash string, expiresAt time.Time) error {
	tx, err := s.db.BeginTx(ctx, nil)
	if err != nil {
		return fmt.Errorf("create reset: %w", err)
	}
	defer tx.Rollback()
	if _, err := tx.ExecContext(ctx, `DELETE FROM password_resets WHERE user_id=?`, userID); err != nil {
		return fmt.Errorf("create reset: %w", err)
	}
	if _, err := tx.ExecContext(ctx, `INSERT INTO password_resets(user_id,token_hash,expires_at) VALUES(?,?,?)`,
		userID, tokenHash, expiresAt.Unix()); err != nil {
		return fmt.Errorf("create reset: %w", err)
	}
	return tx.Commit()
}

// ConsumeReset spends a token and writes the new hash in the same transaction.
//
// Spending and using have to be one step. If the row were deleted after the
// password was written, a crash in between would leave a live link; if it were
// deleted first, a failed write would burn the token and strand somebody who
// has no way to ask for another except by starting over. Deleting rather than
// flagging the row also means expiry and use are the same state, which is why
// there is no used_at column.
func (s *Store) ConsumeReset(ctx context.Context, tokenHash, passwordHash string, now time.Time) (User, error) {
	tx, err := s.db.BeginTx(ctx, nil)
	if err != nil {
		return User{}, fmt.Errorf("consume reset: %w", err)
	}
	defer tx.Rollback()
	var userID int64
	var expiresAt int64
	err = tx.QueryRowContext(ctx, `SELECT user_id,expires_at FROM password_resets WHERE token_hash=?`, tokenHash).
		Scan(&userID, &expiresAt)
	if errors.Is(err, sql.ErrNoRows) {
		return User{}, ErrResetInvalid
	}
	if err != nil {
		return User{}, fmt.Errorf("consume reset: %w", err)
	}
	if now.Unix() >= expiresAt {
		// Clear it on the way past: an expired row is only litter, and this is
		// the one moment we are certain somebody is looking at it.
		_, _ = tx.ExecContext(ctx, `DELETE FROM password_resets WHERE token_hash=?`, tokenHash)
		_ = tx.Commit()
		return User{}, ErrResetInvalid
	}
	if _, err := tx.ExecContext(ctx, `DELETE FROM password_resets WHERE token_hash=?`, tokenHash); err != nil {
		return User{}, fmt.Errorf("consume reset: %w", err)
	}
	if _, err := tx.ExecContext(ctx, `UPDATE users SET password_hash=? WHERE id=?`, passwordHash, userID); err != nil {
		return User{}, fmt.Errorf("consume reset: %w", err)
	}
	user, err := scanUser(tx.QueryRowContext(ctx, `SELECT id,username,email,password_hash,created_at FROM users WHERE id=?`, userID))
	if err != nil {
		return User{}, fmt.Errorf("consume reset: %w", err)
	}
	if err := tx.Commit(); err != nil {
		return User{}, fmt.Errorf("consume reset: %w", err)
	}
	return user, nil
}

type scanner interface{ Scan(...any) error }

func scanUser(row scanner) (User, error) {
	var u User
	err := row.Scan(&u.ID, &u.Username, &u.Email, &u.PasswordHash, &u.CreatedAt)
	return u, err
}
