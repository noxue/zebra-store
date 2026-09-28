import DefaultTheme from 'vitepress/theme'
import type { Theme } from 'vitepress'
import ZsShot from './ZsShot.vue'
import './custom.css'

export default {
  extends: DefaultTheme,
  enhanceApp({ app }) {
    app.component('ZsShot', ZsShot)
  },
} satisfies Theme
