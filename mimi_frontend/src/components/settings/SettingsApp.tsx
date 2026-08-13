import { useState } from "react";
import type { FormEvent } from "react";
import { Button } from "@blueprintjs/core";
import { useAuth } from "../../data/auth";

/* Account settings: the two things a learner may change about the account
   itself, and the one they may not.

   Everything here is a credential, and credentials live in mimi_auth rather
   than in this app's database (see mimi_backend/AGENTS.md) — so both forms
   ask for the current password. That is not belt-and-braces over the session
   cookie: the credential service has no sessions of its own, and a password
   typed just now is the only evidence it accepts that the person at the
   keyboard is the account's owner rather than whoever found it logged in.

   The **username is deliberately not editable**. It addresses a profile
   (/u/<name>), it is the name on every leaderboard row, and anything anyone
   has linked to points at it; changing one is a migration with a redirect at
   the end of it, not a setting. Saying so in the panel is part of the design
   — a field that is merely disabled invites the question. */

/** A failed request reads "PUT /me/email: that email is already registered".
    The verb and path are for the console; what the learner needs is the half
    after the colon, which is the credential service's own wording — it owns
    every rule about passwords and addresses, so its refusals are the honest
    thing to show rather than a message re-guessed here. */
function reason(value: unknown): string {
    const message = value instanceof Error ? value.message : String(value);
    return message.includes(": ")
        ? message.split(": ").slice(1).join(": ")
        : message;
}

/** The outcome line under a form: one slot, so a success and a failure can
    never sit on the page together contradicting each other. */
function Outcome({
    error,
    done,
}: {
    error: string | null;
    done: string | null;
}) {
    if (error)
        return (
            <p className="auth-error" role="alert">
                {error}
            </p>
        );
    if (done)
        return (
            <p className="settings-done" role="status">
                {done}
            </p>
        );
    return null;
}

function EmailSection({ email }: { email: string }) {
    const { setEmail } = useAuth();
    const [next, setNext] = useState("");
    const [password, setPassword] = useState("");
    const [error, setError] = useState<string | null>(null);
    const [done, setDone] = useState<string | null>(null);
    const [saving, setSaving] = useState(false);

    async function submit(event: FormEvent<HTMLFormElement>) {
        event.preventDefault();
        setError(null);
        setDone(null);
        setSaving(true);
        try {
            const user = await setEmail(password, next);
            /* The address on the page comes back from the backend rather than
               from the box above, because the two can differ: it is stored
               folded to lower case. */
            setDone(`Your email is now ${user.email}.`);
            setNext("");
            setPassword("");
        } catch (value) {
            setError(reason(value));
        } finally {
            setSaving(false);
        }
    }

    return (
        <section className="panel settings-card">
            <h2>Email</h2>
            <p className="settings-intro">
                Currently <strong>{email}</strong>
            </p>
            <form className="auth-form" onSubmit={submit}>
                <label>
                    <span>New email</span>
                    <input
                        name="email"
                        type="email"
                        value={next}
                        onChange={(event) => setNext(event.target.value)}
                        autoComplete="email"
                        maxLength={254}
                        required
                    />
                </label>
                <label>
                    <span>Current password</span>
                    <input
                        name="current-password"
                        type="password"
                        value={password}
                        onChange={(event) => setPassword(event.target.value)}
                        autoComplete="current-password"
                        maxLength={1024}
                        required
                    />
                </label>
                <Outcome error={error} done={done} />
                <div className="settings-actions">
                    <Button type="submit" intent="primary" disabled={saving}>
                        {saving ? "Saving…" : "Change email"}
                    </Button>
                </div>
            </form>
        </section>
    );
}

function PasswordSection() {
    const { setPassword } = useAuth();
    const [current, setCurrent] = useState("");
    const [next, setNext] = useState("");
    const [again, setAgain] = useState("");
    const [error, setError] = useState<string | null>(null);
    const [done, setDone] = useState<string | null>(null);
    const [saving, setSaving] = useState(false);

    async function submit(event: FormEvent<HTMLFormElement>) {
        event.preventDefault();
        setError(null);
        setDone(null);
        /* The only rule this form judges for itself. Everything else — the
           length floor, the common-password list, "it must be different" —
           belongs to mimi_auth, which enforces it for the wiki too; but
           whether the two boxes agree is a question about this form, and the
           server has no way to be asked it. */
        if (next !== again) {
            setError("The two new passwords do not match.");
            return;
        }
        setSaving(true);
        try {
            await setPassword(current, next);
            setDone(
                "Your password has been changed. Any other browsers signed in to this account have been signed out.",
            );
            setCurrent("");
            setNext("");
            setAgain("");
        } catch (value) {
            setError(reason(value));
        } finally {
            setSaving(false);
        }
    }

    return (
        <section className="panel settings-card">
            <h2>Password</h2>
            <form className="auth-form" onSubmit={submit}>
                <label>
                    <span>Current password</span>
                    <input
                        name="current-password"
                        type="password"
                        value={current}
                        onChange={(event) => setCurrent(event.target.value)}
                        autoComplete="current-password"
                        maxLength={1024}
                        required
                    />
                </label>
                <label>
                    <span>New password</span>
                    <input
                        name="new-password"
                        type="password"
                        value={next}
                        onChange={(event) => setNext(event.target.value)}
                        autoComplete="new-password"
                        minLength={8}
                        maxLength={1024}
                        required
                    />
                    <small>Use at least 8 characters.</small>
                </label>
                <label>
                    <span>New password again</span>
                    <input
                        name="confirm-password"
                        type="password"
                        value={again}
                        onChange={(event) => setAgain(event.target.value)}
                        autoComplete="new-password"
                        minLength={8}
                        maxLength={1024}
                        required
                    />
                </label>
                <Outcome error={error} done={done} />
                <div className="settings-actions">
                    <Button type="submit" intent="primary" disabled={saving}>
                        {saving ? "Saving…" : "Change password"}
                    </Button>
                </div>
            </form>
        </section>
    );
}

/** What a visitor with no account of their own gets. A guest is one of these:
    their record is real, but mimi_auth has never heard of it, so there is
    nothing here for them to change — and the useful thing to offer is the
    same page the navbar offers, which turns the record they have been
    building into an account rather than starting a new one. */
function NoAccount({ guest }: { guest: boolean }) {
    return (
        <main className="settings-page">
            <div className="settings-column">
                <h1 className="settings-title">Settings</h1>
                <section className="panel settings-card">
                    <h2>{guest ? "Save your progress first" : "Sign in"}</h2>
                    <p className="settings-intro">
                        {guest
                            ? "You are learning as a guest, so there are no account settings yet. Creating an account keeps everything you have done so far — your words, your place in the course and your streak."
                            : "These settings belong to an account. Sign in to change your email or your password."}
                    </p>
                    <div className="settings-actions">
                        <a
                            className="bp6-button bp6-intent-primary"
                            href={
                                guest
                                    ? "/signup?next=%2Fsettings"
                                    : "/login?next=%2Fsettings"
                            }
                        >
                            {guest ? "Create account" : "Sign in"}
                        </a>
                        {guest && (
                            <a
                                className="bp6-button"
                                href="/login?next=%2Fsettings"
                            >
                                Sign in
                            </a>
                        )}
                    </div>
                </section>
            </div>
        </main>
    );
}

export default function SettingsApp() {
    const { user, ready } = useAuth();

    /* The page is prerendered and the viewer arrives afterwards, so the first
       paint knows nothing about who is looking — the same wait the navbar and
       the profile page make. An empty frame holds the layout rather than
       flashing a signed-out page at somebody who is signed in. */
    if (!ready) return <main className="settings-page" />;
    if (!user || user.guest) return <NoAccount guest={user?.guest ?? false} />;

    return (
        <main className="settings-page">
            <div className="settings-column">
                <h1 className="settings-title">Settings</h1>

                <section className="panel settings-card">
                    <h2>Account</h2>
                    <dl className="settings-facts">
                        <div>
                            <dt className="eyebrow">Username</dt>
                            <dd>{user.username}</dd>
                        </div>
                        <div>
                            <dt className="eyebrow">Email</dt>
                            <dd>{user.email}</dd>
                        </div>
                    </dl>
                    <p className="settings-note">
                        Your username can't be changed — it is the address of
                        your profile and the name on the leaderboard, so
                        anything anyone has linked to points at it.
                    </p>
                </section>

                <EmailSection email={user.email ?? ""} />
                <PasswordSection />
            </div>
        </main>
    );
}
