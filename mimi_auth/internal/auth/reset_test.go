package auth

import (
	"context"
	"encoding/json"
	"io"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"
)

// captureMailer stands in for Resend and keeps what it was asked to send, so a
// test can follow the same path a person does: ask for a link, read the link
// out of the mail, and use it.
type captureMailer struct {
	sent []struct{ to, username, link string }
	err  error
}

func (m *captureMailer) SendPasswordReset(_ context.Context, to, username, link string) error {
	if m.err != nil {
		return m.err
	}
	m.sent = append(m.sent, struct{ to, username, link string }{to, username, link})
	return nil
}

// waitForMail waits for the send that requestReset deliberately does not block
// on. Polling rather than a channel keeps captureMailer usable by the tests
// that never look at what it collected.
func (m *captureMailer) waitForMail(t *testing.T, want int) {
	t.Helper()
	deadline := time.Now().Add(2 * time.Second)
	for time.Now().Before(deadline) {
		if len(m.sent) >= want {
			return
		}
		time.Sleep(5 * time.Millisecond)
	}
	t.Fatalf("waited for %d emails, got %d", want, len(m.sent))
}

func resetFixture(t *testing.T) (*http.ServeMux, *captureMailer, func(path, body string) *httptest.ResponseRecorder) {
	t.Helper()
	store, err := Open(t.TempDir() + "/test.db")
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { store.Close() })
	mailer := &captureMailer{}
	handler := NewHandler(store)
	handler.SetMailer(mailer)
	handler.SetResetURL("http://localhost:4773/reset-password")
	mux := http.NewServeMux()
	handler.Routes(mux)
	call := func(path, body string) *httptest.ResponseRecorder {
		r := httptest.NewRequest(http.MethodPost, path, strings.NewReader(body))
		w := httptest.NewRecorder()
		mux.ServeHTTP(w, r)
		return w
	}
	w := call("/v1/register", `{"username":"mimi_user","email":"mimi@example.com","password":"correct horse battery staple"}`)
	if w.Code != http.StatusCreated {
		t.Fatalf("register: %d %s", w.Code, w.Body.String())
	}
	return mux, mailer, call
}

// tokenFrom pulls the token out of a mailed link, which is the only place a
// caller of this service ever sees one.
func tokenFrom(t *testing.T, link string) string {
	t.Helper()
	_, query, ok := strings.Cut(link, "?token=")
	if !ok {
		t.Fatalf("no token in %q", link)
	}
	return query
}

func TestResetPassword(t *testing.T) {
	_, mailer, call := resetFixture(t)

	w := call("/v1/reset/request", `{"login":"mimi@example.com"}`)
	if w.Code != http.StatusAccepted {
		t.Fatalf("request: %d %s", w.Code, w.Body.String())
	}
	mailer.waitForMail(t, 1)
	if mailer.sent[0].to != "mimi@example.com" || mailer.sent[0].username != "mimi_user" {
		t.Fatalf("mailed the wrong account: %+v", mailer.sent[0])
	}
	token := tokenFrom(t, mailer.sent[0].link)

	// The old password still works while the token is merely outstanding, which
	// is what makes an unrequested reset email safe to ignore.
	if w := call("/v1/login", `{"login":"mimi_user","password":"correct horse battery staple"}`); w.Code != http.StatusOK {
		t.Fatalf("old password stopped working before the reset was confirmed: %d", w.Code)
	}

	body, _ := json.Marshal(resetConfirmRequest{Token: token, NewPassword: "a different long password"})
	if w := call("/v1/reset/confirm", string(body)); w.Code != http.StatusOK {
		t.Fatalf("confirm: %d %s", w.Code, w.Body.String())
	}
	if w := call("/v1/login", `{"login":"mimi_user","password":"a different long password"}`); w.Code != http.StatusOK {
		t.Fatalf("new password rejected: %d %s", w.Code, w.Body.String())
	}
	if w := call("/v1/login", `{"login":"mimi_user","password":"correct horse battery staple"}`); w.Code != http.StatusUnauthorized {
		t.Fatalf("old password still works: %d", w.Code)
	}
	// Single use: the same link a second time is worth nothing.
	if w := call("/v1/reset/confirm", string(body)); w.Code != http.StatusUnauthorized {
		t.Fatalf("token was reusable: %d %s", w.Code, w.Body.String())
	}
}

// An unknown login must be indistinguishable from a known one, because an email
// address is a login here and any difference would say who has an account.
func TestResetRequestDoesNotRevealAccounts(t *testing.T) {
	_, mailer, call := resetFixture(t)
	known := call("/v1/reset/request", `{"login":"mimi_user"}`)
	unknown := call("/v1/reset/request", `{"login":"nobody@example.com"}`)
	if known.Code != unknown.Code || known.Body.String() != unknown.Body.String() {
		t.Fatalf("answers differ: %d %s vs %d %s",
			known.Code, known.Body.String(), unknown.Code, unknown.Body.String())
	}
	mailer.waitForMail(t, 1)
	if len(mailer.sent) != 1 {
		t.Fatalf("sent %d emails, want 1 (only the real account)", len(mailer.sent))
	}
}

// Asking twice invalidates the first link, so an older message in an inbox is
// not a second key to the account.
func TestResetRequestSupersedesEarlierToken(t *testing.T) {
	_, mailer, call := resetFixture(t)
	call("/v1/reset/request", `{"login":"mimi_user"}`)
	mailer.waitForMail(t, 1)
	call("/v1/reset/request", `{"login":"mimi_user"}`)
	mailer.waitForMail(t, 2)

	first, _ := json.Marshal(resetConfirmRequest{Token: tokenFrom(t, mailer.sent[0].link), NewPassword: "first new password"})
	if w := call("/v1/reset/confirm", string(first)); w.Code != http.StatusUnauthorized {
		t.Fatalf("superseded token still worked: %d %s", w.Code, w.Body.String())
	}
	second, _ := json.Marshal(resetConfirmRequest{Token: tokenFrom(t, mailer.sent[1].link), NewPassword: "second new password"})
	if w := call("/v1/reset/confirm", string(second)); w.Code != http.StatusOK {
		t.Fatalf("latest token refused: %d %s", w.Code, w.Body.String())
	}
}

func TestResetConfirmRejectsBadInput(t *testing.T) {
	_, mailer, call := resetFixture(t)
	call("/v1/reset/request", `{"login":"mimi_user"}`)
	mailer.waitForMail(t, 1)
	token := tokenFrom(t, mailer.sent[0].link)

	for _, c := range []struct {
		name, body string
		want       int
	}{
		{"unknown token", `{"token":"not-a-real-token","new_password":"a long enough password"}`, http.StatusUnauthorized},
		{"missing token", `{"token":"","new_password":"a long enough password"}`, http.StatusBadRequest},
		{"short password", `{"token":"` + token + `","new_password":"short"}`, http.StatusBadRequest},
		{"common password", `{"token":"` + token + `","new_password":"password1"}`, http.StatusBadRequest},
	} {
		if w := call("/v1/reset/confirm", c.body); w.Code != c.want {
			t.Errorf("%s: got %d, want %d (%s)", c.name, w.Code, c.want, w.Body.String())
		}
	}
	// A refused new password must not have spent the token: the person still
	// has only the one link, and the mistake was theirs to correct.
	good, _ := json.Marshal(resetConfirmRequest{Token: token, NewPassword: "a genuinely fine password"})
	if w := call("/v1/reset/confirm", string(good)); w.Code != http.StatusOK {
		t.Fatalf("token was spent by a rejected attempt: %d %s", w.Code, w.Body.String())
	}
}

// Expiry is enforced against the clock the store is given, so the test can move
// it instead of waiting an hour.
func TestResetTokenExpires(t *testing.T) {
	store, err := Open(t.TempDir() + "/test.db")
	if err != nil {
		t.Fatal(err)
	}
	defer store.Close()
	ctx := context.Background()
	hash, _ := HashPassword("correct horse battery staple")
	u, err := store.CreateUser(ctx, "mimi_user", "mimi@example.com", hash)
	if err != nil {
		t.Fatal(err)
	}
	token, tokenHash, err := newResetToken()
	if err != nil {
		t.Fatal(err)
	}
	issued := time.Now()
	if err := store.CreateReset(ctx, u.ID, tokenHash, issued.Add(resetLifetime)); err != nil {
		t.Fatal(err)
	}
	newHash, _ := HashPassword("a different long password")
	if _, err := store.ConsumeReset(ctx, hashResetToken(token), newHash, issued.Add(resetLifetime+time.Second)); err != ErrResetInvalid {
		t.Fatalf("expired token was accepted: %v", err)
	}
	// And the account is untouched by the attempt.
	after, err := store.UserByID(ctx, u.ID)
	if err != nil {
		t.Fatal(err)
	}
	if ok, _ := VerifyPassword("correct horse battery staple", after.PasswordHash); !ok {
		t.Fatal("an expired reset changed the password anyway")
	}
}

// Only the hash is stored, so a copy of the database is not a set of working
// links.
func TestResetTokenIsStoredHashed(t *testing.T) {
	token, tokenHash, err := newResetToken()
	if err != nil {
		t.Fatal(err)
	}
	if tokenHash == token || strings.Contains(tokenHash, token) {
		t.Fatal("the token itself is what gets stored")
	}
	if hashResetToken(token) != tokenHash {
		t.Fatal("hashing a token twice gives different answers")
	}
}

// The Resend call is one JSON POST; this checks the shape of it without
// touching the network.
func TestResendMailerPostsToResend(t *testing.T) {
	var gotAuth, gotBody string
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		gotAuth = r.Header.Get("Authorization")
		body, _ := io.ReadAll(r.Body)
		gotBody = string(body)
		WriteJSON(w, http.StatusOK, map[string]string{"id": "test"})
	}))
	defer server.Close()
	previous := ResendEndpoint
	ResendEndpoint = server.URL
	defer func() { ResendEndpoint = previous }()

	mailer := NewResendMailer("re_test_key", "Mimi <mimi@example.com>")
	link := "http://localhost:4773/reset-password?token=abc123"
	if err := mailer.SendPasswordReset(context.Background(), "mimi@example.com", "mimi_user", link); err != nil {
		t.Fatal(err)
	}
	if gotAuth != "Bearer re_test_key" {
		t.Errorf("authorization header: %q", gotAuth)
	}
	var payload struct {
		From    string   `json:"from"`
		To      []string `json:"to"`
		Subject string   `json:"subject"`
		Text    string   `json:"text"`
		HTML    string   `json:"html"`
	}
	if err := json.Unmarshal([]byte(gotBody), &payload); err != nil {
		t.Fatalf("body is not JSON: %v", err)
	}
	if payload.From != "Mimi <mimi@example.com>" || len(payload.To) != 1 || payload.To[0] != "mimi@example.com" {
		t.Errorf("addressed wrongly: %+v", payload)
	}
	if !strings.Contains(payload.Text, link) || !strings.Contains(payload.HTML, link) {
		t.Error("the link is missing from one of the two bodies")
	}
	if payload.Subject == "" {
		t.Error("no subject")
	}
}

// A failed send must not change the answer the requester gets, or a mail
// provider's outage would become an account oracle.
func TestResetRequestAnswersTheSameWhenMailFails(t *testing.T) {
	store, err := Open(t.TempDir() + "/test.db")
	if err != nil {
		t.Fatal(err)
	}
	defer store.Close()
	handler := NewHandler(store)
	handler.SetMailer(&captureMailer{err: io.ErrUnexpectedEOF})
	mux := http.NewServeMux()
	handler.Routes(mux)
	call := func(path, body string) *httptest.ResponseRecorder {
		r := httptest.NewRequest(http.MethodPost, path, strings.NewReader(body))
		w := httptest.NewRecorder()
		mux.ServeHTTP(w, r)
		return w
	}
	call("/v1/register", `{"username":"mimi_user","email":"mimi@example.com","password":"correct horse battery staple"}`)
	if w := call("/v1/reset/request", `{"login":"mimi_user"}`); w.Code != http.StatusAccepted {
		t.Fatalf("a failed send changed the answer: %d %s", w.Code, w.Body.String())
	}
}

// A link built for a base that already carries a query keeps it.
func TestResetLinkKeepsExistingQuery(t *testing.T) {
	got := resetLink("http://localhost:4773/reset-password?lang=es", "abc")
	if !strings.Contains(got, "lang=es") || !strings.Contains(got, "token=abc") {
		t.Fatalf("built %q", got)
	}
}
