import { useEffect, useState } from "react";
import type { FormEvent } from "react";
import { Button } from "@blueprintjs/core";
import { requestPasswordReset, resetPassword } from "../data/api";

/* The two halves of a forgotten password, one component because they are one
   errand and share every field recipe on the page: "email me a link" and, an
   inbox later, "here is the link, let me choose a new one".

   Neither half has a session behind it, which is what separates this from the
   password field on the settings page. There the current password authorises
   the change; here the whole problem is that nobody has it, so the proof is
   reaching the address on the account. */
type Mode = "forgot" | "reset";

export default function PasswordResetForm({ mode }: { mode: Mode }) {
    const [login, setLogin] = useState("");
    const [password, setPassword] = useState("");
    const [confirmedPassword, setConfirmedPassword] = useState("");
    /* The token arrives in the query string, and this page is prerendered: the
       first client render has to match HTML that knows nothing about the URL,
       so it starts empty and an effect fills it in, the way AuthForm handles
       `?next=`. `null` is therefore "not looked yet" and "" is "looked, and
       there is no token", which are different screens. */
    const [token, setToken] = useState<string | null>(null);
    const [error, setError] = useState<string | null>(null);
    const [done, setDone] = useState(false);
    const [submitting, setSubmitting] = useState(false);

    useEffect(() => {
        if (mode !== "reset") return;
        setToken(new URLSearchParams(window.location.search).get("token") ?? "");
    }, [mode]);

    async function submit(event: FormEvent<HTMLFormElement>) {
        event.preventDefault();
        setError(null);
        if (mode === "reset" && password !== confirmedPassword) {
            setError("The passwords do not match.");
            return;
        }
        setSubmitting(true);
        try {
            if (mode === "forgot") {
                await requestPasswordReset(login.trim());
            } else {
                await resetPassword(token ?? "", password);
            }
            setDone(true);
        } catch (value) {
            /* The rules about what a password may be live in mimi_auth, so its
               refusal is the sentence worth showing; the transport prefix in
               front of it is not. Same unwrapping as AuthForm. */
            const message =
                value instanceof Error ? value.message : String(value);
            setError(
                message.includes(": ")
                    ? message.split(": ").slice(1).join(": ")
                    : message,
            );
        } finally {
            setSubmitting(false);
        }
    }

    if (mode === "forgot") {
        return (
            <main className="auth-page">
                <section className="panel auth-card">
                    <h1>Forgot your password?</h1>
                    {done ? (
                        <>
                            {/* Said the same way whether or not that account
                                exists. The backend answers identically on
                                purpose (an email address is a login, so a
                                difference here would say who has an account),
                                and this sentence is the frontend keeping that
                                bargain rather than quietly breaking it. */}
                            <p className="auth-intro">
                                If there is a mimi account for{" "}
                                <strong>{login.trim()}</strong>, a link to
                                choose a new password is on its way to the email
                                address on it. The link works once and expires
                                in an hour.
                            </p>
                            <p className="auth-intro">
                                Nothing has changed yet, so your current
                                password still works until you use that link.
                            </p>
                        </>
                    ) : (
                        <>
                            <p className="auth-intro">
                                Tell us which account, and we will email a link
                                for choosing a new password.
                            </p>
                            <form className="auth-form" onSubmit={submit}>
                                <label>
                                    <span>Username or email</span>
                                    <input
                                        name="login"
                                        value={login}
                                        onChange={(event) =>
                                            setLogin(event.target.value)
                                        }
                                        autoComplete="username"
                                        required
                                        autoFocus
                                    />
                                </label>
                                {error && (
                                    <p className="auth-error" role="alert">
                                        {error}
                                    </p>
                                )}
                                <Button
                                    type="submit"
                                    intent="primary"
                                    fill
                                    large
                                    disabled={submitting}
                                >
                                    {submitting
                                        ? "Please wait…"
                                        : "Email me a link"}
                                </Button>
                            </form>
                        </>
                    )}
                    <p className="auth-switch">
                        Remembered it? <a href="/login">Sign in</a>
                    </p>
                </section>
            </main>
        );
    }

    return (
        <main className="auth-page">
            <section className="panel auth-card">
                <h1>Choose a new password</h1>
                {done ? (
                    <>
                        {/* A reset ends every session on the account, this
                            browser's included, so there is nothing to carry
                            forward and signing in is genuinely the next step
                            rather than a formality we are imposing. */}
                        <p className="auth-done">
                            Your password has been changed.
                        </p>
                        <p className="auth-intro">
                            Everything that was signed in to this account has
                            been signed out, so use your new password to sign
                            back in.
                        </p>
                        <Button
                            intent="primary"
                            fill
                            large
                            onClick={() => window.location.assign("/login")}
                        >
                            Sign in
                        </Button>
                    </>
                ) : token === "" ? (
                    <>
                        <p className="auth-error" role="alert">
                            This link is missing its token. Copy the whole
                            address out of the email, or ask for a new link.
                        </p>
                        <p className="auth-switch">
                            <a href="/forgot-password">Email me a new link</a>
                        </p>
                    </>
                ) : (
                    <>
                        <p className="auth-intro">
                            Pick something you have not used elsewhere. At least
                            8 characters, and not one of the hundred thousand
                            most common passwords.
                        </p>
                        <form className="auth-form" onSubmit={submit}>
                            <label>
                                <span>New password</span>
                                <input
                                    name="password"
                                    type="password"
                                    value={password}
                                    onChange={(event) =>
                                        setPassword(event.target.value)
                                    }
                                    autoComplete="new-password"
                                    minLength={8}
                                    maxLength={1024}
                                    required
                                    autoFocus
                                />
                                <small>Use at least 8 characters.</small>
                            </label>
                            <label>
                                <span>Confirm new password</span>
                                <input
                                    name="confirm-password"
                                    type="password"
                                    value={confirmedPassword}
                                    onChange={(event) =>
                                        setConfirmedPassword(event.target.value)
                                    }
                                    autoComplete="new-password"
                                    minLength={8}
                                    maxLength={1024}
                                    required
                                />
                            </label>
                            {error && (
                                <p className="auth-error" role="alert">
                                    {error}
                                </p>
                            )}
                            <Button
                                type="submit"
                                intent="primary"
                                fill
                                large
                                disabled={submitting || token === null}
                            >
                                {submitting
                                    ? "Please wait…"
                                    : "Change my password"}
                            </Button>
                        </form>
                        <p className="auth-switch">
                            Link expired?{" "}
                            <a href="/forgot-password">Ask for a new one</a>
                        </p>
                    </>
                )}
            </section>
        </main>
    );
}
