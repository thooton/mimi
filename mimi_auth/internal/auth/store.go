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

type scanner interface{ Scan(...any) error }

func scanUser(row scanner) (User, error) {
	var u User
	err := row.Scan(&u.ID, &u.Username, &u.Email, &u.PasswordHash, &u.CreatedAt)
	return u, err
}
