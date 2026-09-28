import { expect, it } from 'vitest'
import i18n from './index'
it('FE-25 renders notification template braces literally in all languages', () => {
  const before = i18n.global.locale.value
  for (const locale of ['zh-CN', 'zh-TW', 'en-US'] as const) {
    i18n.global.locale.value = locale
    const messages = i18n.global.getLocaleMessage(locale)
    let checked = 0
    const walk = (node: unknown, prefix = '') => {
      if (!node || typeof node !== 'object') return
      for (const [key, value] of Object.entries(node)) {
        const path = prefix ? `${prefix}.${key}` : key
        if (typeof value === 'string' && value.includes('{{customer_label}}')) {
          checked += 1
          expect(i18n.global.t(path)).toContain('{{customer_label}}')
        } else walk(value, path)
      }
    }
    walk(messages)
    expect(checked).toBeGreaterThan(0)
  }
  i18n.global.locale.value = before
})
