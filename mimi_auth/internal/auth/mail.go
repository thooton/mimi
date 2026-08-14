package auth

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"html"
	"io"
	"log/slog"
	"net/http"
	"net/url"
	"strings"
	"time"
)

// Mailer is the outbox this service spent its first life without.
//
// It exists only for password resets, and it is an interface for two reasons:
// the tests must not post to anybody's real Resend account, and a deployment
// that has not configured a key should still be able to run and reset a
// password by reading the log. Everything else here still sends nothing; an
// address remains a way to sign in and a way to be reached, not proof of
// anything, and confirming a *new* address is still unbuilt.
type Mailer interface {
	SendPasswordReset(ctx context.Context, to, username, resetLink string) error
}

// LogMailer is the fallback when no MIMI_RESEND_API_KEY is set. It writes the
// link where the operator can see it, which is what a local stack wants: an
// email nobody can receive is worse than a line in a terminal, and pretending
// to have sent one would make a broken configuration look like a working one.
type LogMailer struct{}

func (LogMailer) SendPasswordReset(_ context.Context, to, username, resetLink string) error {
	slog.Warn("no mailer configured; password reset link not sent",
		"to", to, "username", username, "link", resetLink)
	return nil
}

// ResendMailer posts to Resend's REST API. There is no SDK dependency here on
// purpose: this is one JSON POST to one endpoint, and the SDK would bring a
// dependency tree for it.
type ResendMailer struct {
	APIKey string
	From   string
	Client *http.Client
}

// ResendEndpoint is a variable so the tests can point it at an httptest server.
var ResendEndpoint = "https://api.resend.com/emails"

// NewResendMailer returns a Resend mailer, or a LogMailer when no key is set,
// so callers can hand the result straight to a Handler either way.
func NewResendMailer(apiKey, from string) Mailer {
	apiKey = strings.TrimSpace(apiKey)
	if apiKey == "" {
		return LogMailer{}
	}
	if strings.TrimSpace(from) == "" {
		from = DefaultResendFrom
	}
	return &ResendMailer{
		APIKey: apiKey,
		From:   from,
		// Bounded, because this call happens while somebody is waiting to be
		// told their email is on its way.
		Client: &http.Client{Timeout: 15 * time.Second},
	}
}

// DefaultResendFrom is Resend's own sandbox sender, which works without a
// verified domain. It is a testing address: Resend will only deliver from it to
// the account owner's own address, so any real deployment must set
// MIMI_RESEND_FROM to something at a domain it has verified.
const DefaultResendFrom = "Mimi <onboarding@resend.dev>"

func (m *ResendMailer) SendPasswordReset(ctx context.Context, to, username, resetLink string) error {
	subject, text, htmlBody := passwordResetMessage(username, resetLink)
	payload, err := json.Marshal(map[string]any{
		"from":    m.From,
		"to":      []string{to},
		"subject": subject,
		"text":    text,
		"html":    htmlBody,
	})
	if err != nil {
		return fmt.Errorf("encode email: %w", err)
	}
	req, err := http.NewRequestWithContext(ctx, http.MethodPost, ResendEndpoint, bytes.NewReader(payload))
	if err != nil {
		return fmt.Errorf("build email request: %w", err)
	}
	req.Header.Set("Authorization", "Bearer "+m.APIKey)
	req.Header.Set("Content-Type", "application/json")
	resp, err := m.Client.Do(req)
	if err != nil {
		return fmt.Errorf("send email: %w", err)
	}
	defer resp.Body.Close()
	if resp.StatusCode >= 300 {
		// Resend explains refusals (unverified domain, bad key) in the body,
		// and that explanation is the whole diagnostic value of the failure.
		// Bounded because it is going into a log line.
		body, _ := io.ReadAll(io.LimitReader(resp.Body, 2<<10))
		return fmt.Errorf("send email: resend returned %d: %s", resp.StatusCode, strings.TrimSpace(string(body)))
	}
	_, _ = io.Copy(io.Discard, resp.Body)
	return nil
}

// passwordResetMessage is the one message this service sends. It names the
// account, because somebody with several should know which one is being reset,
// and it says plainly that ignoring the mail leaves the password alone, which
// is the only useful instruction for a person who did not ask for it.
func passwordResetMessage(username, resetLink string) (subject, text, htmlBody string) {
	subject = "Reset your Mimi password"
	text = fmt.Sprintf(`Hi %s,

Somebody asked to reset the password for your Mimi account. Open this link to
choose a new one:

%s

The link works once and expires in %s.

If this wasn't you, you can ignore this email. Your password stays as it is,
and nobody can sign in without one of these links.
`, username, resetLink, resetLifetimeText)

	// Hand-written and deliberately plain: the link has to survive clients that
	// strip styling, and everything user-supplied is escaped because a username
	// is chosen by the person the mail is about.
	safeUser, safeLink := html.EscapeString(username), html.EscapeString(resetLink)
	htmlBody = fmt.Sprintf(`<div style="font-family:system-ui,-apple-system,Segoe UI,sans-serif;font-size:16px;line-height:1.5;color:#3c3c3c">
  <p>Hi %s,</p>
  <p>Somebody asked to reset the password for your Mimi account.</p>
  <p><a href="%s" style="display:inline-block;background:#58cc02;color:#fff;font-weight:700;text-decoration:none;padding:12px 20px;border-radius:12px">Choose a new password</a></p>
  <p style="font-size:14px;color:#777">The link works once and expires in %s. If the button does not work, paste this into your browser:<br><a href="%s">%s</a></p>
  <p style="font-size:14px;color:#777">If this wasn't you, you can ignore this email &mdash; your password stays as it is.</p>
</div>`, safeUser, safeLink, resetLifetimeText, safeLink, safeLink)
	return subject, text, htmlBody
}

// resetLink puts the token on the page a person actually visits, which belongs
// to a consumer (the learner site) and never to this service: nothing here
// renders HTML, and browsers are not supposed to reach this port at all.
func resetLink(base, token string) string {
	u, err := url.Parse(base)
	if err != nil {
		// A misconfigured base would otherwise mail a link to nowhere with no
		// explanation. Falling back keeps the token reachable by hand.
		return base + "?token=" + url.QueryEscape(token)
	}
	q := u.Query()
	q.Set("token", token)
	u.RawQuery = q.Encode()
	return u.String()
}
