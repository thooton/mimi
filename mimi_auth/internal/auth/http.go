package auth

import (
	"context"
	"database/sql"
	"encoding/json"
	"errors"
	"log/slog"
	"net/http"
	"net/mail"
	"strings"
	"time"
)

type Handler struct {
	store  *Store
	mailer Mailer
	// resetBase is the consumer page a reset link points at. It is a consumer's
	// URL, never one of ours: this service serves JSON to the backend and the
	// wiki, and a person following a link from their inbox has to land on a
	// site that can render a form and hold a session.
	resetBase string
}

func NewHandler(store *Store) *Handler {
	return &Handler{store: store, mailer: LogMailer{}, resetBase: DefaultResetURL}
}

// SetMailer replaces the log-only fallback. Separate from NewHandler so that
// every existing caller, and every test that does not care about email, keeps
// working without a mailer to hand.
func (h *Handler) SetMailer(m Mailer) {
	if m != nil {
		h.mailer = m
	}
}

// SetResetURL points reset links at a consumer's page.
func (h *Handler) SetResetURL(base string) {
	if strings.TrimSpace(base) != "" {
		h.resetBase = strings.TrimSpace(base)
	}
}

func (h *Handler) Routes(mux *http.ServeMux) {
	mux.HandleFunc("GET /healthz", func(w http.ResponseWriter, _ *http.Request) {
		WriteJSON(w, http.StatusOK, map[string]string{"status": "ok"})
	})
	mux.HandleFunc("POST /v1/register", h.register)
	mux.HandleFunc("POST /v1/login", h.login)
	mux.HandleFunc("POST /v1/password", h.changePassword)
	mux.HandleFunc("POST /v1/email", h.changeEmail)
	mux.HandleFunc("POST /v1/reset/request", h.requestReset)
	mux.HandleFunc("POST /v1/reset/confirm", h.confirmReset)
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
type resetRequestRequest struct {
	Login string `json:"login"`
}
type resetConfirmRequest struct {
	Token       string `json:"token"`
	NewPassword string `json:"new_password"`
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
		writeError(w, http.StatusBadRequest, "username must be 3-64 characters using only A-Z, a-z, 0-9 or '_'")
		return
	}
	if reservedUsername(in.Username) {
		writeError(w, http.StatusBadRequest, "username must not contain a reserved role or permission name")
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
// that consumers hold their own sessions and nothing here can end them: a
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
// same reason it authorises a password change, there being no tokens here, and
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

// requestReset is the way in for somebody who cannot authorise anything,
// because what they have lost is the only thing that authorises. It is
// therefore the one endpoint here that acts on an unauthenticated request, and
// its whole design is about giving that request as little as possible.
//
// It always answers 202 with the same body, whether or not the login names an
// account. Anything else turns this into a free membership oracle: an address
// is a login, so "no such account" would say who has signed up. The person who
// really owns the address learns the difference from their inbox, which is the
// only place that distinction belongs.
//
// A successful request does not sign anybody out and does not touch the
// existing password. Until a token is spent, the old password still works,
// which is what makes an unrequested reset email harmless to ignore.
func (h *Handler) requestReset(w http.ResponseWriter, r *http.Request) {
	var in resetRequestRequest
	if !decodeJSON(w, r, &in) {
		return
	}
	login := strings.TrimSpace(in.Login)
	if login == "" {
		writeError(w, http.StatusBadRequest, "login is required")
		return
	}
	u, err := h.store.UserByLogin(r.Context(), login)
	switch {
	case errors.Is(err, sql.ErrNoRows):
		// Deliberately nothing: same answer as success, below.
	case err != nil:
		writeError(w, http.StatusInternalServerError, "could not start a password reset")
		return
	default:
		token, tokenHash, err := newResetToken()
		if err != nil {
			writeError(w, http.StatusInternalServerError, "could not start a password reset")
			return
		}
		if err := h.store.CreateReset(r.Context(), u.ID, tokenHash, time.Now().Add(resetLifetime)); err != nil {
			writeError(w, http.StatusInternalServerError, "could not start a password reset")
			return
		}
		// Sent without waiting, and without the request's context, for two
		// reasons: a mail provider's latency should not be the reply's, and a
		// reply that is slow exactly when the account exists would give away
		// what the identical bodies are there to hide. A failure to send is
		// logged rather than returned, because there is no answer we are
		// willing to give that distinguishes it from having no account.
		link := resetLink(h.resetBase, token)
		go func() {
			ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
			defer cancel()
			if err := h.mailer.SendPasswordReset(ctx, u.Email, u.Username, link); err != nil {
				slog.Error("could not send password reset email", "username", u.Username, "error", err)
			}
		}()
	}
	WriteJSON(w, http.StatusAccepted, map[string]string{
		"status": "if that account exists, a reset link is on its way",
	})
}

// confirmReset spends a token and sets the new password.
//
// The token replaces the current password that every other edit here demands:
// holding one is proof of reaching the account's inbox, which is the only proof
// available to somebody who has forgotten the password. That is why it is
// single-use and short-lived, and why it is the store, not this handler, that
// decides a token is spent.
//
// Unlike changePassword there is no "must differ from the current one" rule.
// The whole premise is that the person does not know the current password, so
// the check could only be a way of telling them what it was.
func (h *Handler) confirmReset(w http.ResponseWriter, r *http.Request) {
	var in resetConfirmRequest
	if !decodeJSON(w, r, &in) {
		return
	}
	in.Token = strings.TrimSpace(in.Token)
	if in.Token == "" {
		writeError(w, http.StatusBadRequest, "token is required")
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
	hash, err := HashPassword(in.NewPassword)
	if err != nil {
		writeError(w, http.StatusInternalServerError, "could not reset the password")
		return
	}
	u, err := h.store.ConsumeReset(r.Context(), hashResetToken(in.Token), hash, time.Now())
	if errors.Is(err, ErrResetInvalid) {
		writeError(w, http.StatusUnauthorized, err.Error())
		return
	}
	if err != nil {
		writeError(w, http.StatusInternalServerError, "could not reset the password")
		return
	}
	WriteJSON(w, http.StatusOK, response(u))
}

// An hour is long enough to survive a slow mail server and somebody who reads
// their email after dinner, and short enough that a forwarded or archived
// message stops being a key to the account fairly soon. resetLifetimeText says
// the same thing in the email; keep them in step.
const resetLifetime = time.Hour

const resetLifetimeText = "1 hour"

// DefaultResetURL is the learner site's page in the default local layout, where
// the frontend is 4773. A deployment sets MIMI_RESET_URL instead.
const DefaultResetURL = "http://localhost:4773/reset-password"

// Eight is the floor NIST SP 800-63B sets for a password a person chooses, and
// the upper bound only exists so that hashing cannot be turned into a denial of
// service. Length is otherwise the whole of the rule on purpose: composition
// requirements push people towards predictable substitutions rather than longer
// passwords. The same guidance pairs that floor with a check against the most
// used passwords, which is what IsCommonPassword does; at eight characters the
// list is doing most of the work.
const passwordLengthMessage = "password must be between 8 and 1024 characters"

const commonPasswordMessage = "password is one of the most commonly used passwords; please choose another"

func validPassword(s string) bool { return len(s) >= 8 && len(s) <= 1024 }

func validUsername(s string) bool {
	if len(s) < 3 || len(s) > 64 {
		return false
	}
	for _, r := range s {
		if !(r >= 'a' && r <= 'z' || r >= 'A' && r <= 'Z' || r >= '0' && r <= '9' || r == '_') {
			return false
		}
	}
	return true
}

// A username is shown without a separate badge everywhere from profiles to
// wiki history. These words would therefore let an ordinary account present
// itself as holding authority it does not have. Match substrings, not just a
// whole name, and fold case so decorations such as SuperAdmin or CHECKUSER_1
// cannot evade the rule.
var reservedUsernameTerms = []string{
	"administrator",
	"bureaucrat",
	"steward",
	"checkuser",
	"oversight",
	"admin",
	"sysop",
	"moderator",
}

func reservedUsername(s string) bool {
	folded := strings.ToLower(s)
	for _, term := range reservedUsernameTerms {
		if strings.Contains(folded, term) {
			return true
		}
	}
	return false
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
