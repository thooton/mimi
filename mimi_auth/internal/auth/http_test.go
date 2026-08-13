package auth

import (
	"bytes"
	"encoding/json"
	"fmt"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
)

func TestRegisterAndLogin(t *testing.T) {
	store, err := Open(t.TempDir() + "/test.db")
	if err != nil {
		t.Fatal(err)
	}
	defer store.Close()
	mux := http.NewServeMux()
	NewHandler(store).Routes(mux)
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
	for _, login := range []string{"mimi_user", "mimi@example.com", "MIMI@EXAMPLE.COM"} {
		body, _ := json.Marshal(loginRequest{Login: login, Password: "correct horse battery staple"})
		w = call("/v1/login", string(body))
		if w.Code != http.StatusOK {
			t.Errorf("login %q: %d %s", login, w.Code, w.Body.String())
		}
	}
	w = call("/v1/login", `{"login":"mimi_user","password":"wrong"}`)
	if w.Code != http.StatusUnauthorized {
		t.Fatalf("wrong password: %d", w.Code)
	}
	w = call("/v1/register", `{"username":"other_user","email":"mimi@example.com","password":"another secure password"}`)
	if w.Code != http.StatusConflict {
		t.Fatalf("duplicate: %d", w.Code)
	}
}

func TestUsernameRules(t *testing.T) {
	store, err := Open(t.TempDir() + "/test.db")
	if err != nil {
		t.Fatal(err)
	}
	defer store.Close()
	mux := http.NewServeMux()
	NewHandler(store).Routes(mux)
	register := func(username, email string) *httptest.ResponseRecorder {
		body, _ := json.Marshal(registerRequest{
			Username: username,
			Email:    email,
			Password: "correct horse battery staple",
		})
		r := httptest.NewRequest(http.MethodPost, "/v1/register", bytes.NewReader(body))
		w := httptest.NewRecorder()
		mux.ServeHTTP(w, r)
		return w
	}

	created := register("AbCdE_9", "mixed@example.com")
	if created.Code != http.StatusCreated {
		t.Fatalf("register mixed-case username: %d %s", created.Code, created.Body.String())
	}
	var got userResponse
	if err := json.Unmarshal(created.Body.Bytes(), &got); err != nil {
		t.Fatal(err)
	}
	if got.Username != "AbCdE_9" {
		t.Errorf("stored username = %q, want original capitalisation %q", got.Username, "AbCdE_9")
	}
	loginBody, _ := json.Marshal(loginRequest{
		Login:    "abcde_9",
		Password: "correct horse battery staple",
	})
	request := httptest.NewRequest(http.MethodPost, "/v1/login", bytes.NewReader(loginBody))
	loginResponse := httptest.NewRecorder()
	mux.ServeHTTP(loginResponse, request)
	if loginResponse.Code != http.StatusOK {
		t.Fatalf("case-insensitive login: %d %s", loginResponse.Code, loginResponse.Body.String())
	}
	if err := json.Unmarshal(loginResponse.Body.Bytes(), &got); err != nil {
		t.Fatal(err)
	}
	if got.Username != "AbCdE_9" {
		t.Errorf("username after case-insensitive login = %q, want %q", got.Username, "AbCdE_9")
	}

	// The original spelling is presentation, not identity: changing only its
	// ASCII letter case cannot create a second account.
	duplicate := register("abcde_9", "other@example.com")
	if duplicate.Code != http.StatusConflict {
		t.Errorf("case-only duplicate: got %d, want %d (%s)", duplicate.Code, http.StatusConflict, duplicate.Body.String())
	}

	for _, username := range []string{"dot.name", "dash-name", "two words", "café", "name!"} {
		w := register(username, username+"@example.com")
		if w.Code != http.StatusBadRequest {
			t.Errorf("register %q: got %d, want %d", username, w.Code, http.StatusBadRequest)
		}
	}

	for i, username := range []string{
		"administrator",
		"the_bureaucrat",
		"Steward123",
		"CHECKUSER_1",
		"oversight_team",
		"SuperAdmin",
		"wiki_sysop",
		"MyModeratorName",
	} {
		w := register(username, fmt.Sprintf("reserved%d@example.com", i))
		if w.Code != http.StatusBadRequest {
			t.Errorf("register reserved name %q: got %d, want %d", username, w.Code, http.StatusBadRequest)
		}
		if !strings.Contains(w.Body.String(), "reserved role or permission") {
			t.Errorf("register reserved name %q: unhelpful response %s", username, w.Body.String())
		}
	}
}

func TestChangePassword(t *testing.T) {
	store, err := Open(t.TempDir() + "/test.db")
	if err != nil {
		t.Fatal(err)
	}
	defer store.Close()
	mux := http.NewServeMux()
	NewHandler(store).Routes(mux)
	call := func(path, body string) *httptest.ResponseRecorder {
		r := httptest.NewRequest(http.MethodPost, path, strings.NewReader(body))
		w := httptest.NewRecorder()
		mux.ServeHTTP(w, r)
		return w
	}
	const old, fresh = "correct horse battery staple", "a different long password"
	if w := call("/v1/register", `{"username":"mimi_user","email":"mimi@example.com","password":"`+old+`"}`); w.Code != http.StatusCreated {
		t.Fatalf("register: %d %s", w.Code, w.Body.String())
	}

	for _, c := range []struct {
		name, body string
		want       int
	}{
		{"wrong current password", `{"login":"mimi_user","current_password":"nope nope nope","new_password":"` + fresh + `"}`, http.StatusUnauthorized},
		{"unknown login", `{"login":"nobody","current_password":"` + old + `","new_password":"` + fresh + `"}`, http.StatusUnauthorized},
		{"new password one short of the minimum", `{"login":"mimi_user","current_password":"` + old + `","new_password":"sevench"}`, http.StatusBadRequest},
		{"new password is a common one", `{"login":"mimi_user","current_password":"` + old + `","new_password":"password1"}`, http.StatusBadRequest},
		{"new password same as old", `{"login":"mimi_user","current_password":"` + old + `","new_password":"` + old + `"}`, http.StatusBadRequest},
		{"by email", `{"login":"MIMI@EXAMPLE.COM","current_password":"` + old + `","new_password":"` + fresh + `"}`, http.StatusOK},
	} {
		if w := call("/v1/password", c.body); w.Code != c.want {
			t.Errorf("%s: got %d want %d (%s)", c.name, w.Code, c.want, w.Body.String())
		}
	}

	// The change took effect, and took the old password out of use with it.
	if w := call("/v1/login", `{"login":"mimi_user","password":"`+fresh+`"}`); w.Code != http.StatusOK {
		t.Errorf("login with the new password: %d %s", w.Code, w.Body.String())
	}
	if w := call("/v1/login", `{"login":"mimi_user","password":"`+old+`"}`); w.Code != http.StatusUnauthorized {
		t.Errorf("the old password still works: %d", w.Code)
	}
}

func TestChangeEmail(t *testing.T) {
	store, err := Open(t.TempDir() + "/test.db")
	if err != nil {
		t.Fatal(err)
	}
	defer store.Close()
	mux := http.NewServeMux()
	NewHandler(store).Routes(mux)
	call := func(path, body string) *httptest.ResponseRecorder {
		r := httptest.NewRequest(http.MethodPost, path, strings.NewReader(body))
		w := httptest.NewRecorder()
		mux.ServeHTTP(w, r)
		return w
	}
	const password = "correct horse battery staple"
	if w := call("/v1/register", `{"username":"mimi_user","email":"mimi@example.com","password":"`+password+`"}`); w.Code != http.StatusCreated {
		t.Fatalf("register: %d %s", w.Code, w.Body.String())
	}
	if w := call("/v1/register", `{"username":"other_user","email":"other@example.com","password":"another secure password"}`); w.Code != http.StatusCreated {
		t.Fatalf("register other: %d %s", w.Code, w.Body.String())
	}

	for _, c := range []struct {
		name, body string
		want       int
	}{
		{"wrong password", `{"login":"mimi_user","password":"nope nope nope","new_email":"new@example.com"}`, http.StatusUnauthorized},
		{"unknown login", `{"login":"nobody","password":"` + password + `","new_email":"new@example.com"}`, http.StatusUnauthorized},
		{"not an address", `{"login":"mimi_user","password":"` + password + `","new_email":"not-an-email"}`, http.StatusBadRequest},
		{"the address it already has", `{"login":"mimi_user","password":"` + password + `","new_email":"MIMI@EXAMPLE.COM"}`, http.StatusBadRequest},
		{"somebody else's address", `{"login":"mimi_user","password":"` + password + `","new_email":"other@example.com"}`, http.StatusConflict},
		{"by email", `{"login":"MIMI@EXAMPLE.COM","password":"` + password + `","new_email":"New@Example.com"}`, http.StatusOK},
	} {
		if w := call("/v1/email", c.body); w.Code != c.want {
			t.Errorf("%s: got %d want %d (%s)", c.name, w.Code, c.want, w.Body.String())
		}
	}

	// The new address is a login, folded like the one registration stored,
	// and the old one no longer names anybody.
	if w := call("/v1/login", `{"login":"NEW@EXAMPLE.COM","password":"`+password+`"}`); w.Code != http.StatusOK {
		t.Errorf("login with the new address: %d %s", w.Code, w.Body.String())
	}
	if w := call("/v1/login", `{"login":"mimi@example.com","password":"`+password+`"}`); w.Code != http.StatusUnauthorized {
		t.Errorf("the old address still signs in: %d", w.Code)
	}
	// The username is untouched: an address is a contact detail, not the name.
	if w := call("/v1/login", `{"login":"mimi_user","password":"`+password+`"}`); w.Code != http.StatusOK {
		t.Errorf("login by username: %d %s", w.Code, w.Body.String())
	}
}

func TestCommonPasswordsAreRefused(t *testing.T) {
	store, err := Open(t.TempDir() + "/test.db")
	if err != nil {
		t.Fatal(err)
	}
	defer store.Close()
	mux := http.NewServeMux()
	NewHandler(store).Routes(mux)
	register := func(username, password string) int {
		body, _ := json.Marshal(registerRequest{Username: username, Email: username + "@example.com", Password: password})
		r := httptest.NewRequest(http.MethodPost, "/v1/register", strings.NewReader(string(body)))
		w := httptest.NewRecorder()
		mux.ServeHTTP(w, r)
		return w.Code
	}
	// Long enough to clear the length rule, and common enough that length was
	// never the thing protecting the account.
	for _, password := range []string{"password", "iloveyou", "trustno1", "superman", "password1", "qwertyuiop"} {
		if code := register("user_"+password, password); code != http.StatusBadRequest {
			t.Errorf("register with %q: got %d, want %d", password, code, http.StatusBadRequest)
		}
	}
	// The list carries its own capitalisations, so these are refused too.
	for _, password := range []string{"Password", "PASSWORD"} {
		if code := register("user_"+password, password); code != http.StatusBadRequest {
			t.Errorf("register with %q: got %d, want %d", password, code, http.StatusBadRequest)
		}
	}
	if code := register("ordinary_user", "correct horse battery staple"); code != http.StatusCreated {
		t.Errorf("an uncommon password was refused: %d", code)
	}
}

func TestCommonPasswordList(t *testing.T) {
	if n := len(commonPasswords()); n != 100000 {
		t.Errorf("list holds %d passwords, want 100000", n)
	}
	if !IsCommonPassword("123456") {
		t.Error("the most common password of all is not in the list")
	}
	if IsCommonPassword("correct horse battery staple") {
		t.Error("an uncommon password is in the list")
	}
	if IsCommonPassword("") {
		t.Error("the empty string is in the list, so blank lines were not skipped")
	}
}

func TestPasswordHash(t *testing.T) {
	hash, err := HashPassword("a strong password")
	if err != nil {
		t.Fatal(err)
	}
	if !strings.HasPrefix(hash, "$argon2id$v=19$") {
		t.Fatalf("unexpected hash: %s", hash)
	}
	ok, err := VerifyPassword("a strong password", hash)
	if err != nil || !ok {
		t.Fatalf("verify: %v %v", ok, err)
	}
	ok, _ = VerifyPassword("not it", hash)
	if ok {
		t.Fatal("incorrect password accepted")
	}
	if bytes.Contains([]byte(hash), []byte("a strong password")) {
		t.Fatal("plaintext leaked")
	}
}
