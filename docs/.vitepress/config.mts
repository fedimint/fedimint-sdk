import { defineConfig, type DefaultTheme } from 'vitepress'
import reactNativeNav from '../react-native/nav.json' with { type: 'json' }

type NavPage = { slug: string; title: string }
type NavGroup = {
  title: string
  pages?: NavPage[]
  sections?: { title: string; pages: NavPage[] }[]
}

// One sidebar per SDK folder, read from that folder's nav.json. tools/build_llms.py reads the
// same file, so the sidebar and llms.txt list the pages in the same order.
function sidebarFrom(
  base: string,
  groups: NavGroup[],
): DefaultTheme.SidebarItem[] {
  const link = (p: NavPage) => ({
    text: p.title,
    link: p.slug === 'index' ? base : `${base}${p.slug}`,
  })
  return groups.map((g) => ({
    text: g.title,
    collapsed: g.sections ? true : false,
    items: [
      ...(g.pages ?? []).map(link),
      ...(g.sections ?? []).map((s) => ({
        text: s.title,
        collapsed: true,
        items: s.pages.map(link),
      })),
    ],
  }))
}

export default defineConfig({
  title: 'Fedimint SDK',
  description: 'Documentation for the Fedimint SDK.',
  cleanUrls: true,
  lastUpdated: false,
  // Only pages are content. Tools, tests, fixtures and their node_modules are not.
  srcExclude: [
    'public/**',
    'tools/**',
    'tests/**',
    'fixtures/**',
    '**/README.md',
    '**/CONVENTIONS.md',
  ],
  head: [
    ['link', { rel: 'preconnect', href: 'https://fonts.googleapis.com' }],
    [
      'link',
      { rel: 'preconnect', href: 'https://fonts.gstatic.com', crossorigin: '' },
    ],
    [
      'link',
      {
        rel: 'stylesheet',
        href: 'https://fonts.googleapis.com/css2?family=Archivo:wght@400;800;900&family=Inter:wght@400;500;600&family=JetBrains+Mono:wght@400;500&display=swap',
      },
    ],
  ],
  // Every page under react-native/ gets the class the scoped Fedimint style keys on.
  transformPageData(page) {
    if (page.relativePath.startsWith('react-native/')) {
      page.frontmatter.pageClass = [
        page.frontmatter.pageClass,
        'fm-react-native',
      ]
        .filter(Boolean)
        .join(' ')
    }
  },
  markdown: {
    config(md) {
      // VitePress shows a fence's [title] only inside a code group. The pages name the file
      // a block is, so show the title above every block that has one.
      const fence = md.renderer.rules.fence!
      md.renderer.rules.fence = (tokens, idx, options, env, self) => {
        const title = tokens[idx].info.match(/\[(.*?)\]/)?.[1]
        const html = fence(tokens, idx, options, env, self)
        if (!title) return html
        return `<div class="fm-code-title">${md.utils.escapeHtml(title)}</div>${html}`
      }
    },
  },
  themeConfig: {
    nav: [{ text: 'React Native', link: '/react-native/' }],
    sidebar: {
      '/react-native/': sidebarFrom('/react-native/', reactNativeNav.groups),
    },
    search: { provider: 'local' },
    outline: [2, 3],
    socialLinks: [
      { icon: 'github', link: 'https://github.com/fedimint/fedimint-sdk' },
    ],
    editLink: {
      pattern: 'https://github.com/fedimint/fedimint-sdk/edit/main/docs/:path',
    },
  },
})
