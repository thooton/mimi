import { defineConfig } from 'astro/config';
import react from '@astrojs/react';

const port = Number(process.env.PORT ?? 4773);
if (!Number.isInteger(port) || port < 1 || port > 65535) {
  throw new Error('PORT must be an integer between 1 and 65535');
}

/* Public profiles are one page serving every name.
 *
 * The site is prerendered to a directory of files (`//mimi_frontend:build`
 * zips `dist/`), and accounts are created long after that build — so there is
 * no way to emit a page per profile. Instead `src/pages/u/index.astro` is
 * emitted once and every `/u/<name>` is **rewritten** onto it with a 200;
 * `PublicProfile` reads the name back out of `location`.
 *
 * That rewrite is the host's job, and this file is the only place both halves
 * of it are written down.
 *
 * The dev server's half is the middleware below. The deployment half has to be
 * configured wherever `dist/` is served — for nginx:
 *
 *     location ^~ /u/ {
 *         try_files $uri $uri/ /u/index.html;
 *     }
 *
 * `public/_redirects` carries the same rule in Netlify/Cloudflare Pages form,
 * so those hosts need nothing else. A host with **no** rewrite configured
 * serves /u/<name> as a 404 and only /u/ works, which is the failure to look
 * for if profile links break in production.
 */
const profileRewrite = {
  name: 'mimi-profile-rewrite',
  configureServer(server) {
    server.middlewares.use((request, _response, next) => {
      const url = request.url ?? '';
      /* `/u/sam` and `/u/sam?x=1` become `/u/` and `/u/?x=1`; `/u/` itself
         already matches a route and is left alone, as is anything deeper
         than one segment (there is nothing under a profile to ask for). */
      const match = /^\/u\/[^/?#]+(?=$|[?#])/.exec(url);
      if (match) request.url = `/u/${url.slice(match[0].length)}`;
      next();
    });
  },
};

// https://astro.build/config
export default defineConfig({
  integrations: [react()],
  server: {
    port,
  },
  vite: {
    plugins: [profileRewrite],
    /* Vite normally searches upward when its port is occupied. Every other
       Mimi service treats its assigned port as a contract, so do the same:
       fail visibly and let the operator choose another PORT. */
    server: {
      strictPort: true,
    },
    preview: {
      strictPort: true,
    },
  },
});
