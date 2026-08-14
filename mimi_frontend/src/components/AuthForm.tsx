import { useEffect, useState } from "react";
import type { FormEvent } from "react";
import { Button } from "@blueprintjs/core";
import { useAuth } from "../data/auth";
import { safeNext, withNext } from "../data/next";
import { usernameContainsReservedTerm } from "../data/username";

type Mode = "login" | "signup";

export default function AuthForm({ mode }: { mode: Mode }) {
    const { user, ready, signIn, signUp } = useAuth();
    /* Where to go afterwards, a guest is sent here from wherever they were
     and should land back there. The page is prerendered, so the first client
     render has to agree with HTML that knows nothing about the query string:
     the search starts empty and an effect fills it in, the same way the auth
     and language stores handle what the server couldn't know. */
    const [search, setSearch] = useState("");
    useEffect(() => setSearch(window.location.search), []);
    const next = safeNext(search);
    const [username, setUsername] = useState("");
    const [email, setEmail] = useState("");
    const [login, setLogin] = useState("");
    const [password, setPassword] = useState("");
    const [confirmedPassword, setConfirmedPassword] = useState("");
    const [error, setError] = useState<string | null>(null);
    const [submitting, setSubmitting] = useState(false);

    /* Somebody who is already signed in has no business on either page, but a
     guest very much does: signing up is the whole reason they were sent
     here, and bouncing them back to the course would make the offer
     unanswerable. */
    const settled = ready && user !== null && !user.guest;
    useEffect(() => {
        if (settled) window.location.replace(next);
    }, [settled, next]);

    async function submit(event: FormEvent<HTMLFormElement>) {
        event.preventDefault();
        setError(null);
        if (mode === "signup" && usernameContainsReservedTerm(username)) {
            setError(
                "That username contains a reserved role or permission name.",
            );
            return;
        }
        if (mode === "signup" && password !== confirmedPassword) {
            setError("The passwords do not match.");
            return;
        }
        setSubmitting(true);
        try {
            if (mode === "signup") {
                await signUp(username, email, password);
            } else {
                await signIn(login, password);
            }
            window.location.assign(next);
        } catch (value) {
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

    if (!ready || settled) return <main className="auth-page" />;

    const signup = mode === "signup";
    /* A guest arriving here has already done some of the course, and saying so
     is the difference between an errand and finishing something: registering
     claims the record they have been building, rather than opening a new one
     (see mimi_backend/AGENTS.md). Signing in discards it, so that page says
     so plainly rather than letting them find out afterwards. */
    const saving = user?.guest ?? false;
    return (
        <main className="auth-page">
            <section className="panel auth-card">
                <h1>{signup ? "Create your mimi account" : "Welcome back"}</h1>
                <p className="auth-intro"></p>

                <form className="auth-form" onSubmit={submit}>
                    {signup ? (
                        <>
                            <label>
                                <span>Username</span>
                                <input
                                    name="username"
                                    value={username}
                                    onChange={(event) =>
                                        setUsername(event.target.value)
                                    }
                                    autoComplete="username"
                                    minLength={3}
                                    maxLength={64}
                                    pattern="[A-Za-z0-9_]+"
                                    title="Use 3–64 ASCII letters, numbers, or underscores."
                                    required
                                    autoFocus
                                />
                                <small>
                                    Use 3–64 ASCII letters, numbers, or underscores.
                                </small>
                            </label>
                            <label>
                                <span>Email</span>
                                <input
                                    name="email"
                                    type="email"
                                    value={email}
                                    onChange={(event) =>
                                        setEmail(event.target.value)
                                    }
                                    autoComplete="email"
                                    maxLength={254}
                                    required
                                />
                            </label>
                        </>
                    ) : (
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
                    )}

                    <label>
                        <span>Password</span>
                        <input
                            name="password"
                            type="password"
                            value={password}
                            onChange={(event) =>
                                setPassword(event.target.value)
                            }
                            autoComplete={
                                signup ? "new-password" : "current-password"
                            }
                            minLength={signup ? 8 : undefined}
                            maxLength={1024}
                            required
                        />
                        {signup && <small>Use at least 8 characters.</small>}
                    </label>

                    {/* Only on the sign-in side: on the sign-up side there is
                        no password yet to have forgotten. */}
                    {!signup && (
                        <p className="auth-forgot">
                            <a href="/forgot-password">
                                Forgot your password?
                            </a>
                        </p>
                    )}

                    {signup && (
                        <label>
                            <span>Confirm password</span>
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
                    )}

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
                            : signup
                              ? "Create account"
                              : "Sign in"}
                    </Button>
                </form>

                <p className="auth-switch">
                    {signup ? "Already have an account?" : "New to mimi?"}{" "}
                    <a href={withNext(signup ? "/login" : "/signup", search)}>
                        {signup ? "Sign in" : "Create an account"}
                    </a>
                </p>
            </section>
        </main>
    );
}
