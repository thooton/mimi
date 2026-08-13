package auth

import (
	"database/sql"
	"encoding/json"
	"errors"
	"net/http"
	"net/mail"
	"strings"
)

type Handler struct{ store *Store }

func NewHandler(store *Store) *Handler { return &Handler{store: store} }

func (h *Handler) Routes(mux *http.ServeMux) {
	mux.HandleFunc("GET /healthz", func(w http.ResponseWriter, _ *http.Request) {
		WriteJSON(w, http.StatusOK, map[string]string{"status": "ok"})
	})
	mux.HandleFunc("POST /v1/register", h.register)
	mux.HandleFunc("POST /v1/login", h.login)
	mux.HandleFunc("POST /v1/password", h.changePassword)
	mux.HandleFunc("POST /v1/email", h.changeEmail)
}

type registerRequest struct {
	Username string `json:"username"`
	Password string `json:"password"`
	Email    string `json:"email"`
}
type loginRequest struct {
	Login    string `json:"login"`
	Password string `json:"password"`
}
type changePasswordRequest struct {
	Login           string `json:"login"`
	CurrentPassword string `json:"current_password"`
	NewPassword     string `json:"new_password"`
}
type changeEmailRequest struct {
	Login    string `json:"login"`
	Password string `json:"password"`
	NewEmail string `json:"new_email"`
}
type userResponse struct {
	ID       int64  `json:"id"`
	Username string `json:"username"`
	Email    string `json:"email"`
}

func (h *Handler) register(w http.ResponseWriter, r *http.Request) {
	var in registerRequest
	if !decodeJSON(w, r, &in) {
		return
	}
	in.Username, in.Email = strings.TrimSpace(in.Username), strings.TrimSpace(in.Email)
	if !validUsername(in.Username) {
		writeError(w, http.StatusBadRequest, "username must be 3-64 characters using letters, numbers, '.', '_' or '-'")
		return
	}
	if !validEmail(in.Email) {
		writeError(w, http.StatusBadRequest, "a valid email is required")
		return
	}
	if !validPassword(in.Password) {
		writeError(w, http.StatusBadRequest, passwordLengthMessage)
		return
	}
	if IsCommonPassword(in.Password) {
		writeError(w, http.StatusBadRequest, commonPasswordMessage)
		return
	}
	hash, err := HashPassword(in.Password)
	if err != nil {
		writeError(w, http.StatusInternalServerError, "could not create account")
		return
	}
	u, err := h.store.CreateUser(r.Context(), in.Username, in.Email, hash)
	if errors.Is(err, ErrUserExists) {
		writeError(w, http.StatusConflict, err.Error())
		return
	}
	if err != nil {
		writeError(w, http.StatusInternalServerError, "could not create account")
		return
	}
	WriteJSON(w, http.StatusCreated, response(u))
}

func (h *Handler) login(w http.ResponseWriter, r *http.Request) {
	var in loginRequest
	if !decodeJSON(w, r, &in) {
		return
	}
	if strings.TrimSpace(in.Login) == "" || in.Password == "" {
		writeError(w, http.StatusBadRequest, "login and password are required")
		return
	}
	u, err := h.store.UserByLogin(r.Context(), strings.TrimSpace(in.Login))
	if errors.Is(err, sql.ErrNoRows) {
		writeError(w, http.StatusUnauthorized, "invalid credentials")
		return
	}
	if err != nil {
		writeError(w, http.StatusInternalServerError, "could not authenticate")
		return
	}
	ok, err := VerifyPassword(in.Password, u.PasswordHash)
	if err != nil || !ok {
		writeError(w, http.StatusUnauthorized, "invalid credentials")
		return
	}
	WriteJSON(w, http.StatusOK, response(u))
}

// changePassword is the only way a stored hash is ever replaced. There are no
// tokens or sessions here yet, so the current password is what authorises the
// change: a consumer asks on behalf of somebody who has just retyped it. Note
// that consumers hold their own sessions, and nothing here can end them — a
// changed password does not sign the account out of any of the Mimi sites.
func (h *Handler) changePassword(w http.ResponseWriter, r *http.Request) {
	var in changePasswordRequest
	if !decodeJSON(w, r, &in) {
		return
	}
	if strings.TrimSpace(in.Login) == "" || in.CurrentPassword == "" {
		writeError(w, http.StatusBadRequest, "login and current_password are required")
		return
	}
	if !validPassword(in.NewPassword) {
		writeError(w, http.StatusBadRequest, passwordLengthMessage)
		return
	}
	if IsCommonPassword(in.NewPassword) {
		writeError(w, http.StatusBadRequest, commonPasswordMessage)
		return
	}
	u, err := h.store.UserByLogin(r.Context(), strings.TrimSpace(in.Login))
	if errors.Is(err, sql.ErrNoRows) {
		writeError(w, http.StatusUnauthorized, "invalid credentials")
		return
	}
	if err != nil {
		writeError(w, http.StatusInternalServerError, "could not authenticate")
		return
	}
	ok, err := VerifyPassword(in.CurrentPassword, u.PasswordHash)
	if err != nil || !ok {
		writeError(w, http.StatusUnauthorized, "invalid credentials")
		return
	}
	// Checked only once the current password is known to be right, so that a
	// wrong one cannot be told apart from a right one by the answer it gets.
	if in.NewPassword == in.CurrentPassword {
		writeError(w, http.StatusBadRequest, "the new password must be different from the current one")
		return
	}
	hash, err := HashPassword(in.NewPassword)
	if err != nil {
		writeError(w, http.StatusInternalServerError, "could not change the password")
		return
	}
	if err := h.store.UpdatePassword(r.Context(), u.ID, hash); err != nil {
		writeError(w, http.StatusInternalServerError, "could not change the password")
		return
	}
	WriteJSON(w, http.StatusOK, response(u))
}

// changeEmail moves an account's address. The password authorises it for the
// same reason it authorises a password change — there are no tokens here — and
// it is required even though the new address is not itself a secret: an email
// is also a login, and a session left open on a shared machine should not be
// enough to point the account at somewhere else.
//
// Nothing is sent to either address. Confirming a new one, and telling the old
// one that it changed, wants an outbox this service does not have; until then a
// consumer should treat the address as a contact detail rather than as proof of
// anything.
func (h *Handler) changeEmail(w http.ResponseWriter, r *http.Request) {
	var in changeEmailRequest
	if !decodeJSON(w, r, &in) {
		return
	}
	if strings.TrimSpace(in.Login) == "" || in.Password == "" {
		writeError(w, http.StatusBadRequest, "login and password are required")
		return
	}
	in.NewEmail = strings.TrimSpace(in.NewEmail)
	if !validEmail(in.NewEmail) {
		writeError(w, http.StatusBadRequest, "a valid email is required")
		return
	}
	u, err := h.store.UserByLogin(r.Context(), strings.TrimSpace(in.Login))
	if errors.Is(err, sql.ErrNoRows) {
		writeError(w, http.StatusUnauthorized, "invalid credentials")
		return
	}
	if err != nil {
		writeError(w, http.StatusInternalServerError, "could not authenticate")
		return
	}
	ok, err := VerifyPassword(in.Password, u.PasswordHash)
	if err != nil || !ok {
		writeError(w, http.StatusUnauthorized, "invalid credentials")
		return
	}
	// Addresses are stored folded, so this is the same comparison the UNIQUE
	// index would make. Checked after the password, so that a wrong one gets
	// the same answer whatever address it was paired with.
	if strings.EqualFold(in.NewEmail, u.Email) {
		writeError(w, http.StatusBadRequest, "that is already the address on this account")
		return
	}
	updated, err := h.store.UpdateEmail(r.Context(), u.ID, in.NewEmail)
	if errors.Is(err, ErrUserExists) {
		writeError(w, http.StatusConflict, "that email is already registered")
		return
	}
	if err != nil {
		writeError(w, http.StatusInternalServerError, "could not change the email")
		return
	}
	WriteJSON(w, http.StatusOK, response(updated))
}

// Eight is the floor NIST SP 800-63B sets for a password a person chooses, and
// the upper bound only exists so that hashing cannot be turned into a denial of
// service. Length is otherwise the whole of the rule on purpose: composition
// requirements push people towards predictable substitutions rather than longer
// passwords. The same guidance pairs that floor with a check against the most
// used passwords, which is what IsCommonPassword does — at eight characters the
// list is doing most of the work.
const passwordLengthMessage = "password must be between 8 and 1024 characters"

const commonPasswordMessage = "password is one of the most commonly used passwords; please choose another"

func validPassword(s string) bool { return len(s) >= 8 && len(s) <= 1024 }

func validUsername(s string) bool {
	if len(s) < 3 || len(s) > 64 {
		return false
	}
	for _, r := range s {
		if !(r >= 'a' && r <= 'z' || r >= 'A' && r <= 'Z' || r >= '0' && r <= '9' || strings.ContainsRune("._-", r)) {
			return false
		}
	}
	return true
}
func validEmail(s string) bool {
	a, err := mail.ParseAddress(s)
	return err == nil && a.Address == s && strings.Contains(s, "@") && len(s) <= 254
}
func response(u User) userResponse {
	return userResponse{ID: u.ID, Username: u.Username, Email: u.Email}
}

func decodeJSON(w http.ResponseWriter, r *http.Request, dst any) bool {
	r.Body = http.MaxBytesReader(w, r.Body, 1<<20)
	dec := json.NewDecoder(r.Body)
	dec.DisallowUnknownFields()
	if err := dec.Decode(dst); err != nil {
		writeError(w, http.StatusBadRequest, "invalid JSON body")
		return false
	}
	var extra any
	if dec.Decode(&extra) == nil {
		writeError(w, http.StatusBadRequest, "body must contain one JSON object")
		return false
	}
	return true
}
func WriteJSON(w http.ResponseWriter, status int, value any) {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(status)
	_ = json.NewEncoder(w).Encode(value)
}
func writeError(w http.ResponseWriter, status int, message string) {
	WriteJSON(w, status, map[string]string{"error": message})
}
