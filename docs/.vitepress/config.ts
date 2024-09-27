import { getSidebar } from './sidebar'
import { withMermaid } from 'vitepress-plugin-mermaid'

// https://vitepress.dev/reference/site-config
export default withMermaid({
  title: 'Fedimint Web Sdk',
  description: 'Building Fedimint Ecash into the web',
  ignoreDeadLinks: false,
  lang: 'en-US',
  lastUpdated: true,
  head: [
    [
      'meta',
      {
        name: 'keywords',
        content: 'bitcoin, lightning, ecash, fedimint, typescript, wasm, react',
      },
    ],
    ['link', { rel: 'icon', href: '/favicon.ico' }],
    ['meta', { name: 'theme-color', content: '#346cff' }],
    // Open Graph
    ['meta', { property: 'og:type', content: 'website' }],
    [
      'meta',
      {
        property: 'og:image',
        content: 'https://web.fedimint.org/og.png',
      },
    ],
    ['meta', { property: 'og:url', content: 'https://web.fedimint.org' }],
    // Twitter
    ['meta', { name: 'twitter:card', content: 'summary_large_image' }],
    ['meta', { name: 'twitter:creator', content: '@fedimint' }],
    [
      'meta',
      { name: 'twitter:image', content: 'https://web.fedimint.org/og.png' },
    ],
    ['meta', { name: 'twitter:site', content: 'https://web.fedimint.org' }],
  ],
  themeConfig: {
    editLink: {
      pattern:
        'https://github.com/fedimint/fedimint-web-sdk/edit/main/docs/:path',
      text: 'Suggest changes to this page',
    },
    footer: {
      message:
        'Released under the <a href="https://github.com/fedimint/fedimint-web-sdk/blob/main/LICENSE">MIT License</a>.',
    },
    nav: [
      { text: 'Documentation', link: '/core/getting-started' },
      { text: 'Examples', link: '/examples' },
      {
        text: 'More',
        items: [
          {
            text: 'Contributing',
            link: '/core/dev/contributing',
          },
          {
            text: 'Discussions ',
            link: 'https://chat.fedimint.org',
          },
          {
            text: 'Release Notes ',
            link: 'https://github.com/fedimint/fedimint-web-sdk/releases',
          },
        ],
      },
    ],
    logo: {
      light: '/icon.png',
      dark: '/icon.png',
      alt: 'Fedimint Logo',
    },
    sidebar: getSidebar(),

    socialLinks: [
      {
        icon: 'github',
        link: 'https://github.com/fedimint/fedimint-web-sdk',
      },
      {
        icon: 'npm',
        link: 'https://www.npmjs.com/package/@fedimint/core-web',
      },
      {
        icon: 'discord',
        link: 'https://chat.fedimint.org',
      },
      {
        icon: 'twitter',
        link: 'https://twitter.com/fedimint',
      },
    ],

    outline: [2, 3],
    search: {
      provider: 'local',
      options: {
        _render(src, env, md) {
          const html = md.render(src, env)
          if (env.frontmatter?.search === false) return ''
          if (env.relativePath.startsWith('shared')) return ''
          return html
        },
      },
    },
  },
})
