import { getSidebar } from './sidebar'
import { transformerTwoslash } from '@shikijs/vitepress-twoslash'
import {
  groupIconMdPlugin,
  groupIconVitePlugin,
} from 'vitepress-plugin-group-icons'
import defineVersionedConfig from 'vitepress-versioning-plugin'
import { Plugin as VitePressPlugin } from 'vitepress'

// https://vitepress.dev/reference/site-config
export default defineVersionedConfig(
  {
    title: 'Fedimint Sdk',
    description: 'Building Fedimint Ecash into Apps',
    ignoreDeadLinks: false,
    lang: 'en-US',
    lastUpdated: true,
    markdown: {
      // TODO: Fix version conflicts
      // @ts-ignore
      codeTransformers: [transformerTwoslash()],
      config: (md) => {
        // @ts-ignore
        md.use(groupIconMdPlugin)
      },
    },
    vite: {
      plugins: [
        {
          ...groupIconVitePlugin(),
          name: 'vitepress-group-icons-compat',
        } as VitePressPlugin,
      ],
    },
    /* prettier-ignore */
    head: [
    ['meta', { name: 'keywords', content: 'bitcoin, lightning, ecash, fedimint, typescript, wasm, react' } ],
    ['link', { rel: 'icon', href: '/favicon.ico' }],
    ['meta', { name: 'theme-color', content: '#346cff' }],

    // Open Graph
    ['meta', { property: 'og:type', content: 'website' }],
    ['meta', { property: 'og:url', content: 'https://sdk.fedimint.org' }],
    ['meta', { property: 'og:title', content: 'Fedimint Sdk | Building Ecash into Apps' } ],
    ['meta', { property: 'og:site_name', content: 'Fedimint Sdk' }],
    ['meta', { property: 'og:image', content: 'https://sdk.fedimint.org/og.png' } ],

    // Twitter
    ['meta', { name: 'twitter:card', content: 'summary' }],
    ['meta', { name: 'twitter:creator', content: '@fedimint' }],
    ['meta', { name: 'twitter:site', content: '@fedimint' }],
  ],
    themeConfig: {
      editLink: {
        pattern:
          'https://github.com/fedimint/fedimint-sdk/edit/main/docs/:path',
        text: 'Suggest changes to this page',
      },
      footer: {
        message:
          'Released under the <a href="https://github.com/fedimint/fedimint-sdk/blob/main/LICENSE">MIT License</a>.',
      },
      nav: [
        { text: 'Documentation', link: '/core/getting-started' },
        { text: 'Examples', link: '/examples/vite-react' },
        {
          text: 'More',
          items: [
            {
              text: 'Contributing',
              link: '/core/dev/contributing',
            },
            {
              text: 'Awesome Fedimint Sdk',
              link: '/core/dev/awesome',
            },
            {
              text: 'Discussions ',
              link: 'https://chat.fedimint.org',
            },
            {
              text: 'Release Notes ',
              link: 'https://github.com/fedimint/fedimint-sdk/releases',
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
          link: 'https://github.com/fedimint/fedimint-sdk',
        },
        {
          icon: 'npm',
          link: 'https://www.npmjs.com/package/@fedimint/core',
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
    versioning: {
      latestVersion: '0.1.x',
    },
  },
  __dirname,
)
