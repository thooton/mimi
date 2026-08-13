/* Facts about the project itself, quoted by the pages that talk about it. */

/* Where the code lives. Every "open source" link on the site points here, so
   it exists exactly once. */
export const REPO_URL = 'https://github.com/thooton/mimi';

/* The course editor: kept in one configurable place for the navbar and every
   marketing CTA. Astro replaces the public environment value while building;
   the fallback keeps a source checkout useful without local configuration. */
export const EDITOR_URL =
  import.meta.env.PUBLIC_MIMI_EDITOR ?? 'http://mimi.localhost:4771';
