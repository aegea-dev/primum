# primum docs

The source for primum's documentation site, built with
[Astro](https://astro.build) and [Starlight](https://starlight.astro.build),
the same setup as videre's.

```bash
yarn install      # once
yarn dev          # http://localhost:4321
yarn build        # static site into dist/, then checks its local assets
yarn preview      # serve dist/ locally
yarn test         # the build check's own tests
```

Yarn 4, pinned by `packageManager` in `package.json`; `corepack enable` makes
that exact version run. `.yarnrc.yml` sets `nodeLinker: node-modules`: Yarn's
PnP linker breaks Astro's virtual modules, so keep it.

## Layout

```
src/content/docs/
  index.mdx          landing page
  start/             install, quickstart, concepts
  commands/          one page per command
  guides/            tasks across commands
  reference/         file formats, the board, troubleshooting
src/assets/          logos and the board screenshots
astro.config.mjs     sidebar, theme
```

Adding a page means creating the file and adding it to `sidebar` in
`astro.config.mjs`. A front matter value containing `: ` must be quoted.

`board-light.png` and `board-dark.png` in this folder are what the root
README shows (by their raw GitHub URLs); `src/assets/` holds copies for the
site.

## Deploying

`yarn build` writes plain static files to `dist/`; any static host works.
`wrangler.jsonc` describes a Cloudflare assets-only Worker:

| Setting | Value |
|---|---|
| Root directory | `docs` |
| Build command | `corepack enable && yarn install --immutable && yarn build` |
| Build output directory | `dist` |
| Node version | from `.node-version` |

Once the site has a domain, set `site` in `astro.config.mjs` (canonical URLs
and the sitemap) and add a `Sitemap:` line to `public/robots.txt`.

## Where things belong

- **README.md** (repository root): what primum is, install, a quick start,
  and a link here.
- **This site**: every command, option, file and behavior users meet.
- **CLAUDE.md**: building, testing and getting a change into primum.
