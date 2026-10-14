// @ts-check
import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

const REPO = 'https://github.com/erhangundogan/primum';

// https://astro.build/config
export default defineConfig({
	// Set `site` to the deployed origin once it has one: canonical URLs and the
	// sitemap are built from it, and without it Starlight skips the sitemap.
	integrations: [
		starlight({
			title: 'primum',
			description:
				'Prioritized work items as plain markdown files, with a command line, a local web board, history, sync and GitHub Issues.',
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
						{ label: 'github', slug: 'commands/github' },
						{ label: 'import', slug: 'commands/import' },
					],
				},
				{
					label: 'Guides',
					items: [
						{ label: 'Adding many items', slug: 'guides/adding-many-items' },
						{ label: 'Specs, plans and other documents', slug: 'guides/documents' },
						{ label: 'Many projects', slug: 'guides/many-projects' },
						{ label: 'History and sync', slug: 'guides/history-and-sync' },
						{ label: 'Conflicts', slug: 'guides/conflicts' },
						{ label: 'GitHub Issues', slug: 'guides/github-issues' },
						{ label: 'Working with AI agents', slug: 'guides/agents' },
					],
				},
				{
					label: 'Reference',
					items: [
						{ label: 'Item files', slug: 'reference/item-files' },
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
