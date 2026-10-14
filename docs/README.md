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

The site is served by GitHub Pages at <https://erhangundogan.github.io/primum/>.
`.github/workflows/docs.yml` builds it on every pull request that touches
`docs/`, and publishes it from `main`. The repository's Pages setting must be
"GitHub Actions".

Under Pages the site lives at `/primum/`: `base` in `astro.config.mjs` says
so, and a small remark plugin there prefixes it onto root links written in
pages, so pages keep writing `[add](/commands/add/)`. Links in front matter
(the landing page's hero) are relative instead. To move to a domain of its
own, set `SITE` and `BASE` in `astro.config.mjs`, the base argument of the
build check in `package.json`, and the `Sitemap:` line in
`public/robots.txt`.

## Where things belong

- **README.md** (repository root): what primum is, install, a quick start,
  and a link here.
- **This site**: every command, option, file and behavior users meet.
- **CLAUDE.md**: building, testing and getting a change into primum.
