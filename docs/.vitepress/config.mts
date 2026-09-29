import { defineConfig, type DefaultTheme } from 'vitepress'

const repo = 'https://github.com/SemanS/kontext'

type Page = [slug: string, en: string, sk: string]
type Section = { dir: string; en: string; sk: string; pages: Page[] }

const sections: Section[] = [
  {
    dir: 'getting-started',
    en: 'Getting started',
    sk: 'Začíname',
    pages: [
      ['01-introduction', 'Introduction', 'Úvod'],
      ['02-installation', 'Installation', 'Inštalácia'],
      ['03-quickstart', 'Quick start', 'Rýchly štart'],
      ['04-setup-for-agents', 'Set up your agents', 'Nastavenie agentov'],
    ],
  },
  {
    dir: 'concepts',
    en: 'Concepts',
    sk: 'Koncepty',
    pages: [
      ['01-architecture', 'Architecture', 'Architektúra'],
      ['02-knowledge-store', 'Knowledge store', 'Úložisko znalostí'],
      ['03-capture-and-review', 'Capture and review', 'Zachytávanie a review'],
      ['04-context-layers', 'Context layers and budgets', 'Vrstvy kontextu a rozpočty'],
      ['05-retrieval', 'Retrieval', 'Vyhľadávanie'],
      ['06-init-pipeline', 'The init pipeline', 'Inicializačný pipeline'],
      ['07-git-integration', 'Git integration', 'Integrácia s gitom'],
      ['08-security', 'Security and privacy', 'Bezpečnosť a súkromie'],
    ],
  },
  {
    dir: 'adapters',
    en: 'Adapters',
    sk: 'Adaptéry',
    pages: [
      ['01-overview', 'Overview', 'Prehľad'],
      ['02-configuration', 'Configuring adapters', 'Konfigurácia adaptérov'],
      ['03-presets', 'Presets', 'Presety'],
      ['04-writing-an-adapter', 'Writing an adapter', 'Vlastný adaptér'],
    ],
  },
  {
    dir: 'agent-integrations',
    en: 'Agent integrations',
    sk: 'Integrácie agentov',
    pages: [
      ['01-overview', 'Overview', 'Prehľad'],
      ['02-claude-code', 'Claude Code', 'Claude Code'],
      ['03-codex', 'Codex', 'Codex'],
      ['04-other-clients', 'Cursor, OpenCode and other MCP clients', 'Cursor, OpenCode a ďalší MCP klienti'],
    ],
  },
  {
    dir: 'guides',
    en: 'Guides',
    sk: 'Návody',
    pages: [
      ['01-bootstrap-an-existing-repository', 'Bootstrap an existing repository', 'Zavedenie do existujúceho repozitára'],
      ['02-team-workflow', 'Team workflow', 'Tímový workflow'],
      ['03-openviking', 'OpenViking', 'OpenViking'],
      ['04-code-intelligence', 'Code intelligence', 'Inteligencia kódu'],
      ['05-session-history', 'Session history', 'História sessions'],
      ['06-autopilot', 'Deepen with an LLM', 'Prehĺbenie cez LLM'],
      ['07-ci', 'Validate knowledge in CI', 'Validácia znalostí v CI'],
      ['08-distill-threads', 'Knowledge from agent threads', 'Znalosti z vlákien agentov'],
    ],
  },
  {
    dir: 'reference',
    en: 'Reference',
    sk: 'Referencia',
    pages: [
      ['01-cli', 'CLI', 'CLI'],
      ['02-mcp', 'MCP tools, prompts and resources', 'MCP nástroje, prompty a zdroje'],
      ['03-configuration', 'Configuration', 'Konfigurácia'],
      ['04-templates', 'Templates and result mapping', 'Šablóny a mapovanie výsledkov'],
    ],
  },
  {
    dir: 'about',
    en: 'About',
    sk: 'O projekte',
    pages: [
      ['01-faq', 'FAQ', 'Časté otázky'],
      ['02-changelog', 'Changelog', 'Zoznam zmien'],
      ['03-roadmap', 'Roadmap', 'Plán'],
    ],
  },
]

function sidebar(lang: 'en' | 'sk'): DefaultTheme.SidebarItem[] {
  const prefix = lang === 'en' ? '' : '/sk'
  return sections.map((s) => ({
    text: lang === 'en' ? s.en : s.sk,
    collapsed: false,
    items: s.pages.map(([slug, en, sk]) => ({ text: lang === 'en' ? en : sk, link: `${prefix}/${s.dir}/${slug}` })),
  }))
}

function nav(lang: 'en' | 'sk'): DefaultTheme.NavItem[] {
  const p = lang === 'en' ? '' : '/sk'
  const t = (en: string, sk: string) => (lang === 'en' ? en : sk)
  return [
    { text: t('Guide', 'Príručka'), link: `${p}/getting-started/01-introduction`, activeMatch: `${p}/(getting-started|concepts|guides)/` },
    { text: t('Adapters', 'Adaptéry'), link: `${p}/adapters/01-overview`, activeMatch: `${p}/adapters/` },
    { text: t('Agents', 'Agenti'), link: `${p}/agent-integrations/01-overview`, activeMatch: `${p}/agent-integrations/` },
    { text: t('Reference', 'Referencia'), link: `${p}/reference/01-cli`, activeMatch: `${p}/reference/` },
    {
      text: 'v0.1',
      items: [
        { text: t('Changelog', 'Zoznam zmien'), link: `${p}/about/02-changelog` },
        { text: t('Roadmap', 'Plán'), link: `${p}/about/03-roadmap` },
        { text: t('Releases', 'Vydania'), link: `${repo}/releases` },
      ],
    },
  ]
}

export default defineConfig({
  title: 'kontext',
  description: 'Team context bridge for coding agents: git-native decision memory, one MCP server, pluggable adapters.',
  base: '/kontext/',
  cleanUrls: true,
  lastUpdated: true,
  // English sources live in docs/en/ (like docs/sk/) but are served at the site root
  rewrites: { 'en/:rest*': ':rest*' },
  // the docs show kontext templates such as {{query}} everywhere — keep Vue from interpolating them
  vue: { template: { compilerOptions: { delimiters: ['{%vue', 'vue%}'] } } },
  head: [
    ['link', { rel: 'icon', type: 'image/svg+xml', href: '/kontext/favicon.svg' }],
    ['meta', { name: 'theme-color', content: '#4338ca' }],
    ['meta', { property: 'og:title', content: 'kontext — team context bridge for coding agents' }],
    ['meta', { property: 'og:description', content: 'Git-native decision memory for Claude Code, Codex, Cursor and every MCP client.' }],
  ],
  themeConfig: {
    logo: '/logo.svg',
    socialLinks: [{ icon: 'github', link: repo }],
    search: {
      provider: 'local',
      options: {
        locales: {
          sk: {
            translations: {
              button: { buttonText: 'Hľadať', buttonAriaLabel: 'Hľadať' },
              modal: {
                noResultsText: 'Žiadne výsledky pre',
                resetButtonTitle: 'Zrušiť',
                footer: { selectText: 'vybrať', navigateText: 'prejsť', closeText: 'zavrieť' },
              },
            },
          },
        },
      },
    },
    editLink: { pattern: `${repo}/edit/main/docs/:path` },
    footer: {
      message: 'Released under the MIT or Apache-2.0 license.',
      copyright: 'Copyright © 2026 kontext contributors',
    },
  },
  locales: {
    root: {
      label: 'English',
      lang: 'en',
      themeConfig: { nav: nav('en'), sidebar: sidebar('en') },
    },
    sk: {
      label: 'Slovenčina',
      lang: 'sk',
      link: '/sk/',
      description: 'Tímový kontext pre coding agentov: rozhodnutia v gite, jeden MCP server, zásuvné adaptéry.',
      themeConfig: {
        nav: nav('sk'),
        sidebar: sidebar('sk'),
        editLink: { pattern: `${repo}/edit/main/docs/:path`, text: 'Upraviť túto stránku na GitHube' },
        outline: { label: 'Na tejto stránke' },
        docFooter: { prev: 'Predchádzajúca', next: 'Ďalšia' },
        lastUpdated: { text: 'Naposledy upravené' },
        langMenuLabel: 'Jazyk',
        returnToTopLabel: 'Späť hore',
        sidebarMenuLabel: 'Menu',
        darkModeSwitchLabel: 'Vzhľad',
        lightModeSwitchTitle: 'Svetlý režim',
        darkModeSwitchTitle: 'Tmavý režim',
        notFound: { title: 'Stránka nenájdená', quote: 'Táto stránka neexistuje.', linkText: 'Späť na úvod' },
        footer: {
          message: 'Vydané pod licenciou MIT alebo Apache-2.0.',
          copyright: 'Copyright © 2026 prispievatelia projektu kontext',
        },
      },
    },
  },
})
