import { useEffect, useState } from "react";
import type { FormEvent } from "react";
import { Button, Dialog } from "@blueprintjs/core";
import type { ApiProfileEdit } from "../../data/api";
import { updateProfile } from "../../data/api";
import type { Profile } from "../../data/profile";
import { safeAvatar } from "../../data/profile";

/* The profile editor: the things a person may say about themselves, in one
   form submitted whole (see ApiProfileEdit). It is deliberately small —
   everything else on the profile page is derived from what the user actually
   did, and none of that is editable by anybody.

   The picture is a **link**, not an upload. Mimi hosts no images: that would
   mean storage, moderation and a bill, none of which is what this app is for.
   What the user gets instead is a URL field with a live preview beside it, so
   a link that doesn't resolve to a picture is obvious here rather than on
   their public page. */

/** The same limits the backend enforces (profile.rs). Spelling them out here
    buys the ordinary typing experience — a field that stops rather than a
    submission that fails — and the backend still has the last word, because a
    maxLength is a courtesy and not a check. */
const MAX_DISPLAY = 32;
const MAX_BIO = 300;
const MAX_AVATAR = 300;

export default function EditProfileDialog({
    profile,
    isOpen,
    onClose,
    onSaved,
}: {
    /** the profile as it stands — the form's starting values */
    profile: Profile;
    isOpen: boolean;
    onClose: () => void;
    /** the edit landed; the page refetches rather than patching its own copy */
    onSaved: () => void;
}) {
    const [display, setDisplay] = useState(profile.display);
    const [bio, setBio] = useState(profile.bio);
    const [avatar, setAvatar] = useState(profile.avatar ?? "");
    const [error, setError] = useState<string | null>(null);
    const [saving, setSaving] = useState(false);

    /* Opening the dialog re-reads the profile, so a cancelled edit is really
     cancelled rather than waiting in the fields for next time.

     Keyed on `isOpen` alone, deliberately: `profile` is rebuilt from the API
     response on every render of the page behind this, so depending on it
     would throw away what somebody had typed the moment anything up there
     re-rendered. The values it reads are still the current ones — the effect
     runs with the props of the render that opened the dialog. */
    useEffect(() => {
        if (!isOpen) return;
        setDisplay(profile.display);
        setBio(profile.bio);
        setAvatar(profile.avatar ?? "");
        setError(null);
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [isOpen]);

    async function submit(event: FormEvent<HTMLFormElement>) {
        event.preventDefault();
        setError(null);
        setSaving(true);
        const edit: ApiProfileEdit = {
            display,
            bio,
            /* The backend still accepts the legacy field as part of its
               whole-profile request. Preserve it while it is absent from the
               editor, rather than making another edit silently clear it. */
            cefr: profile.cefr,
            /* an empty field is not an empty URL, it is no picture */
            avatar: avatar.trim() === "" ? null : avatar.trim(),
        };
        try {
            await updateProfile(edit);
            onSaved();
            onClose();
        } catch (value) {
            /* the backend's message says which field is wrong and why, and it is
         better than anything this dialog could guess; the request's method
         and path are the part the reader doesn't need */
            const message =
                value instanceof Error ? value.message : String(value);
            setError(
                message.includes(": ")
                    ? message.split(": ").slice(1).join(": ")
                    : message,
            );
        } finally {
            setSaving(false);
        }
    }

    /* The preview shows only what the page itself would be willing to load, so
     a rejected URL previews as the initial — which is exactly what other
     people would see. */
    const preview = safeAvatar(avatar.trim() === "" ? null : avatar.trim());

    return (
        <Dialog
            isOpen={isOpen}
            onClose={onClose}
            icon="edit"
            title="Edit profile"
            className="edit-dialog"
        >
            <form className="edit-form" onSubmit={submit}>
                <label>
                    <span>Display name</span>
                    <input
                        name="display"
                        value={display}
                        onChange={(event) => setDisplay(event.target.value)}
                        maxLength={MAX_DISPLAY}
                        required
                        autoFocus
                    />
                </label>

                <label>
                    <span>Bio</span>
                    <textarea
                        name="bio"
                        value={bio}
                        onChange={(event) => setBio(event.target.value)}
                        maxLength={MAX_BIO}
                        rows={3}
                    />
                    <small>
                        {bio.length}/{MAX_BIO}
                    </small>
                </label>

                <label>
                    <span>Picture URL</span>
                    <div className="edit-avatar-row">
                        <span className="edit-avatar" aria-hidden="true">
                            {preview ? (
                                <img src={preview} alt="" />
                            ) : (
                                display.charAt(0)
                            )}
                        </span>
                        <input
                            name="avatar"
                            type="url"
                            value={avatar}
                            onChange={(event) => setAvatar(event.target.value)}
                            maxLength={MAX_AVATAR}
                            placeholder="https://example.com/me.png"
                        />
                    </div>
                </label>

                {error && (
                    <p className="edit-error" role="alert">
                        {error}
                    </p>
                )}

                <div className="edit-foot">
                    <Button text="Cancel" onClick={onClose} disabled={saving} />
                    <Button
                        type="submit"
                        intent="primary"
                        text={saving ? "Saving…" : "Save"}
                        disabled={saving}
                    />
                </div>
            </form>
        </Dialog>
    );
}
