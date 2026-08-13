import { useEffect, useState } from "react";
import { usernameFromPath } from "../../data/profile";
import ProfileApp from "./ProfileApp";

/* Somebody else's profile, at /u/<name>.

   The name is read from the address bar rather than baked into the page,
   because the site is prerendered and there is no build at which the set of
   accounts is known — people sign up after it. One page is emitted, at
   /u/, and the host rewrites every /u/<name> onto it (see astro.config.mjs,
   which spells out both the dev-server rule and the nginx one). So this
   component is the other half of that rewrite: the server has thrown the
   name away by the time the page is served, and `location` is where it
   survives.

   That also means the name cannot be known while the page is being
   prerendered — `location` doesn't exist then — so it is resolved in an
   effect and nothing is rendered until it is. Passing `undefined` to
   ProfileApp in the meantime would be read as "show me my own profile",
   which would flash the wrong person's record on the way past. */

export default function PublicProfile() {
  /* undefined = not looked yet, null = looked and there is no name */
  const [username, setUsername] = useState<string | null | undefined>(
    undefined,
  );

  useEffect(() => {
    setUsername(usernameFromPath(window.location.pathname));
  }, []);

  if (username === undefined) {
    return <div className="shell profile-page" />;
  }

  /* /u/ on its own names nobody. Reachable by typing the URL, and by a
     rewrite rule that is too eager. */
  if (username === null) {
    return (
      <div className="shell profile-page">
        <section className="panel profile-empty">
          <h1 className="profile-name">No profile here</h1>
          <p className="profile-bio">
            A profile lives at /u/ followed by a username.
          </p>
        </section>
      </div>
    );
  }

  return <ProfileApp username={username} />;
}
