import { describe, expect, it } from 'vitest'
import { localeSelectOptions, SUPPORTED_LOCALES } from '@/i18n'

describe('localeSelectOptions', () => {
  it('lists every supported locale with its native name', () => {
    const options = localeSelectOptions()
    expect(options.map((o) => o.value)).toEqual([...SUPPORTED_LOCALES])
    expect(options.map((o) => o.label)).toEqual(['简体中文', '繁體中文', 'English'])
  })
})
