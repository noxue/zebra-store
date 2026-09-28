// Live QA 2026-09-26 I-8: plain-JS custom scripts must execute (QA-A08).
// Needs `runScripts: 'dangerously'` (vite.config.ts); scripts signal via the DOM.
import { afterEach, describe, expect, it } from 'vitest'
import { applyCustomScripts, clearCustomScripts } from '@/utils/customScripts'

const counter = () => Number(document.documentElement.dataset.qaA08 || 0)
const bump = 'document.documentElement.dataset.qaA08 = String(Number(document.documentElement.dataset.qaA08 || 0) + 1)'

describe('custom scripts', () => {
  afterEach(() => {
    clearCustomScripts()
    delete document.documentElement.dataset.qaA08
    delete document.documentElement.dataset.qaA08Html
  })

  it('QA-A08 wraps plain JS in a live <script> that executes', () => {
    applyCustomScripts([{ name: 'plain', enabled: true, position: 'head', code: bump }])
    const node = document.head.querySelector('script[data-site-script-managed="1"]')
    expect(node?.textContent).toContain('qaA08')
    expect(counter()).toBe(1)
  })

  it('QA-A08 re-creates <script> tags from HTML snippets and keeps other elements', () => {
    applyCustomScripts([
      { name: 'html', enabled: 'true', position: 'body_end', code: '<meta name="qa" content="1"><script>document.documentElement.dataset.qaA08Html = "ran"</script>' },
    ])
    expect(document.documentElement.dataset.qaA08Html).toBe('ran')
    expect(document.body.querySelectorAll('[data-site-script-managed="1"]')).toHaveLength(2)
  })

  it('QA-A08 clears old managed nodes when the list changes and skips unchanged lists', () => {
    const first = [{ name: 'a', enabled: true, position: 'head', code: bump }]
    applyCustomScripts(first)
    applyCustomScripts(first)
    expect(counter()).toBe(1)
    applyCustomScripts([{ name: 'b', enabled: true, position: 'head', code: 'void 0' }])
    const managed = document.querySelectorAll('[data-site-script-managed="1"]')
    expect(managed).toHaveLength(1)
    expect(managed[0]?.getAttribute('data-site-script-name')).toBe('b')
    applyCustomScripts([{ name: 'off', enabled: false, code: bump }])
    expect(document.querySelectorAll('[data-site-script-managed="1"]')).toHaveLength(0)
    expect(counter()).toBe(1)
  })
})
