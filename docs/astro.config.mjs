// @ts-check
import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

const REPO = 'https://github.com/erhangundogan/primum';
// Served by GitHub Pages at https://erhangundogan.github.io/primum/. With a
// custom domain later, set SITE to it and BASE to '/'.
const SITE = 'https://erhangundogan.github.io';
const BASE = '/primum';

// Starlight prefixes BASE onto sidebar and asset links, but not onto links
// written in pages, so `[add](/commands/add/)` would leave the site. Pages keep
// writing root links and this adds the prefix, so a move to a domain of its
// own changes nothing but the two constants above.
function remarkBase() {
	const walk = (node) => {
		if (node.type === 'link' && /^\/(?!\/)/.test(node.url) && BASE !== '/') {
			node.url = BASE + node.url;
		}
		for (const child of node.children ?? []) walk(child);
	};
	return walk;
}

// https://astro.build/config
export default defineConfig({
	// Canonical URLs, the sitemap and the search index are built from these.
	site: SITE,
	base: BASE,
	trailingSlash: 'always',
	// Published pages that moved: their old URLs keep working.
	redirects: {
		'/guides/many-projects/': `${BASE}/guides/workspaces/`.replace('//', '/'),
	},
	markdown: { remarkPlugins: [remarkBase] },
	integrations: [
		starlight({
			title: 'primum',
			description:
				'Prioritized work items as plain markdown files, with a command line, a local web board, history and sync.',
			expressiveCode: {
				themes: ['starlight-dark', 'github-light'],
				useStarlightDarkModeSwitch: true,
				useStarlightUiThemeColors: true,
			},
			logo: {
				light: './src/assets/logo-horizontal-light.svg',
				dark: './src/assets/logo-horizontal-dark.svg',
				replacesTitle: true,
			},
			favicon: '/favicon.svg',
			customCss: ['./src/styles/custom.css'],
			social: [{ icon: 'github', label: 'GitHub', href: REPO }],
			editLink: { baseUrl: `${REPO}/edit/main/docs/` },
			lastUpdated: true,
			// Search is Pagefind, built at compile time and served as static
			// assets: no account, no API key, nothing leaves the reader's browser.
			sidebar: [
				{
					label: 'Start here',
					items: [
						{ label: 'What primum is', slug: 'index' },
						{ label: 'Install', slug: 'start/install' },
						{ label: 'Quickstart', slug: 'start/quickstart' },
						{ label: 'Items, lanes and scores', slug: 'start/concepts' },
					],
				},
				{
					label: 'Commands',
					items: [
						{ label: 'Overview', slug: 'commands' },
						{ label: 'init', slug: 'commands/init' },
						{ label: 'workspace', slug: 'commands/workspace' },
						{ label: 'add', slug: 'commands/add' },
						{ label: 'list', slug: 'commands/list' },
						{ label: 'show', slug: 'commands/show' },
						{ label: 'next', slug: 'commands/next' },
						{ label: 'set', slug: 'commands/set' },
						{ label: 'start and done', slug: 'commands/start-done' },
						{ label: 'note', slug: 'commands/note' },
						{ label: 'doc', slug: 'commands/doc' },
						{ label: 'kind', slug: 'commands/kind' },
						{ label: 'delete', slug: 'commands/delete' },
						{ label: 'serve', slug: 'commands/serve' },
						{ label: 'log', slug: 'commands/log' },
						{ label: 'projects', slug: 'commands/projects' },
						{ label: 'remote', slug: 'commands/remote' },
						{ label: 'clone', slug: 'commands/clone' },
						{ label: 'sync', slug: 'commands/sync' },
						{ label: 'resolve', slug: 'commands/resolve' },
						{ label: 'import', slug: 'commands/import' },
						{ label: 'migrate', slug: 'commands/migrate' },
						{ label: 'context', slug: 'commands/context' },
					],
				},
				{
					label: 'Guides',
					items: [
						{ label: 'Adding many items', slug: 'guides/adding-many-items' },
						{ label: 'Specs, plans and other documents', slug: 'guides/documents' },
						{ label: 'Workspaces', slug: 'guides/workspaces' },
						{ label: 'History and sync', slug: 'guides/history-and-sync' },
						{ label: 'Conflicts', slug: 'guides/conflicts' },
						{ label: 'Working with AI agents', slug: 'guides/agents' },
					],
				},
				{
					label: 'Reference',
					items: [
						{ label: 'Item files', slug: 'reference/item-files' },
						{ label: 'Lifecycles', slug: 'reference/lifecycles' },
						{ label: 'The lifecycle manager', slug: 'reference/manager' },
						{ label: 'The daemon', slug: 'reference/daemon' },
						{ label: 'The worker', slug: 'reference/worker' },
						{ label: 'A project folder', slug: 'reference/project-folder' },
						{ label: 'The board', slug: 'reference/board' },
						{ label: 'Board HTTP interface', slug: 'reference/http' },
						{ label: 'Troubleshooting', slug: 'reference/troubleshooting' },
					],
				},
			],
		}),
	],
});
