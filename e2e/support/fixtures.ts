import path from 'node:path'
import { fileURLToPath } from 'node:url'
import { test as base, expect, type Page, type Response } from '@playwright/test'

export { expect }

type Matcher = RegExp | ((problem: Problem) => boolean)

export interface Problem {
  kind: 'console' | 'pageerror' | 'api' | 'http'
  text: string
  url?: string
  statusCode?: number
}

/** Collects browser console errors and failed API calls for the whole test. */
export class PageWatch {
  readonly problems: Problem[] = []
  private readonly allowed: Matcher[] = []
  private readonly pending = new Set<Promise<void>>()

  /** Tolerate problems matching `m` (an expected business error, e.g. a wrong password). */
  allow(m: Matcher) {
    this.allowed.push(m)
  }

  isAllowed(p: Problem) {
    return this.allowed.some((m) => (m instanceof RegExp ? m.test(`${p.kind} ${p.url ?? ''} ${p.text}`) : m(p)))
  }

  attach(page: Page) {
    page.on('console', (msg) => {
      if (msg.type() !== 'error') return
      const loc = msg.location()
      this.problems.push({ kind: 'console', text: msg.text(), url: loc.url })
    })
    page.on('pageerror', (err) => this.problems.push({ kind: 'pageerror', text: `${err.name}: ${err.message}` }))
    page.on('response', (res) => {
      const task = this.inspect(res)
      this.pending.add(task)
      void task.finally(() => this.pending.delete(task))
    })
  }

  private async inspect(res: Response) {
    const url = res.url()
    if (!/\/api\/v1\//.test(url)) return
    const method = res.request().method()
    const where = `${method} ${new URL(url).pathname}`
    if (res.status() >= 400) {
      this.problems.push({ kind: 'http', text: `HTTP ${res.status()} ${where}`, url })
      return
    }
    const ct = res.headers()['content-type'] ?? ''
    if (!ct.includes('application/json')) return
    try {
      const body = (await res.json()) as { status_code?: number; msg?: string }
      if (typeof body.status_code === 'number' && body.status_code !== 0) {
        this.problems.push({ kind: 'api', text: `status_code=${body.status_code} msg=${body.msg ?? ''} ${where}`, url, statusCode: body.status_code })
      }
    } catch {
      /* body unavailable (navigation) */
    }
  }

  async settle() {
    await Promise.allSettled([...this.pending])
  }

  unexpected(): Problem[] {
    return this.problems.filter((p) => !this.isAllowed(p))
  }
}

const here = path.dirname(fileURLToPath(import.meta.url))
export const SCREENSHOT_DIR = path.join(here, '..', 'screenshots')

/**
 * Whole-page screenshot into e2e/screenshots/<name>.png. The viewport is grown to the document
 * height first (instead of `fullPage`) so sticky/h-screen layouts render as a user sees them.
 */
export async function shot(page: Page, name: string) {
  await page.waitForLoadState('networkidle').catch(() => undefined)
  await page.waitForTimeout(300)
  const vp = page.viewportSize() ?? { width: 1440, height: 900 }
  const height = await page.evaluate(() => Math.max(document.documentElement.scrollHeight, document.body.scrollHeight))
  await page.setViewportSize({ width: vp.width, height: Math.min(Math.max(height, vp.height), 4000) })
  await page.screenshot({ path: path.join(SCREENSHOT_DIR, `${name}.png`) })
  await page.setViewportSize(vp)
}

export const test = base.extend<{ watch: PageWatch }>({
  watch: [
    async ({ page, context }, use, testInfo) => {
    await context.addInitScript(() => {
      try {
        if (!localStorage.getItem('admin_locale')) localStorage.setItem('admin_locale', 'en-US')
        if (!localStorage.getItem('locale')) localStorage.setItem('locale', 'en-US')
      } catch {
        /* storage unavailable */
      }
    })
    const watch = new PageWatch()
    watch.attach(page)
    context.on('page', (p) => {
      if (p !== page) watch.attach(p)
    })
    await use(watch)
    await watch.settle()
    const bad = watch.unexpected()
    if (bad.length) {
      await testInfo.attach('browser-problems', { body: JSON.stringify(bad, null, 2), contentType: 'application/json' })
    }
    expect(bad, 'browser console errors / failed API calls').toEqual([])
    },
    { auto: true },
  ],
})
