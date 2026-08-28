import DefaultTheme from 'vitepress/theme'
import FullWidthLayout from './FullWidthLayout.vue'
import TwoslashFloatingVue from '@shikijs/vitepress-twoslash/client'
import type { EnhanceAppContext } from 'vitepress'

import './custom.css'
import 'virtual:group-icons.css'

import '@shikijs/vitepress-twoslash/style.css'

export default {
  ...DefaultTheme,
  Layout: DefaultTheme.Layout,
  enhanceApp({ app }: EnhanceAppContext) {
    app.component('FullWidthLayout', FullWidthLayout)
    // TODO: Fix version conflicts
    // @ts-ignore
    app.use(TwoslashFloatingVue)
  },
}
