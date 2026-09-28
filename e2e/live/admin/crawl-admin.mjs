// Visits every admin route, recording errors, load time, raw i18n keys and untranslated CJK.
// usage: node live/crawl-admin.mjs [site=sa] [locale=zh-CN] [mobile]
import { adminContext, browser, log, save, shot, watch, SITES } from './lib.mjs'

const [site = 'sa', locale = 'zh-CN', mode = ''] = process.argv.slice(2)
const mobile = mode === 'mobile'
const ROUTES = [
  '', 'products', 'categories', 'card-secrets', 'card-secret-imports', 'card-secret-exports', 'gift-cards', 'wholesale-prices',
  'orders', 'order-risk-control', 'order-refunds', 'payments', 'payment-channels', 'callback-routes',
  'users', 'user-login-logs', 'wallet-recharges', 'wallet-config', 'member-levels',
  'posts/categories', 'posts/blog', 'posts/notice', 'banners', 'media', 'coupons', 'promotions',
  'affiliates/settings', 'affiliates/users', 'affiliates/commissions', 'affiliates/withdraws',
  'resellers/operations', 'resellers/profiles', 'resellers/domains', 'resellers/site-configs', 'resellers/product-settings',
  'resellers/ledger-entries', 'resellers/balance-accounts', 'resellers/withdraws',
  'site-connections', 'product-mappings', 'procurement-orders', 'reconciliation', 'api-credentials',
  'telegram-bot', 'telegram-bot/settings', 'telegram-bot/help-center', 'telegram-bot/menu', 'telegram-bot/status',
  'telegram-bot/channel-clients', 'telegram-bot/broadcasts', 'telegram-bot/broadcasts/create',
  'settings', 'settings/notifications', 'authz', 'authz-audit-logs', 'security',
]
const only = process.env.ROUTES ? process.env.ROUTES.split(',') : null

const b = await browser()
const ctx = await adminContext(b, { site, locale, mobile, viewport: mobile ? { width: 390, height: 844 } : { width: 1440, height: 900 } })
const page = await ctx.newPage()
const results = []
for (const r of only ?? ROUTES) {
  const sink = watch(page, [])
  const t0 = Date.now()
  await page.goto(`${SITES[site].admin}/${r}`)
  try { await page.waitForLoadState('networkidle', { timeout: 15000 }) } catch { sink.push({ kind: 'slow', text: 'networkidle >15s' }) }
  const ms = Date.now() - t0
  await page.waitForTimeout(400)
  const info = await page.evaluate((loc) => {
    const main = document.querySelector('main') || document.body
    const text = main.innerText
    // raw i18n keys like admin.foo.bar or common.xxx
    const keys = [...new Set(text.match(/\b(?:admin|common|nav|menu|settings|errors?|form|table|status)\.[a-zA-Z0-9_.]+\b/g) || [])]
    let cjk = []
    if (loc === 'en-US') {
      const els = [...main.querySelectorAll('button,label,th,h1,h2,h3,h4,[role=tab],nav a,option,.zs-label,p,span')]
      cjk = [...new Set(els.filter((e) => e.children.length === 0).map((e) => e.textContent.trim()).filter((s) => /[一-鿿]/.test(s) && s.length < 60))].slice(0, 25)
    }
    const hscroll = document.documentElement.scrollWidth > window.innerWidth + 2
    const title = document.querySelector('h1,h2')?.textContent?.trim()
    return { keys, cjk, hscroll, title, url: location.pathname }
  }, locale)
  const name = `crawl-${site}-${locale}${mobile ? '-m' : ''}-${(r || 'dashboard').replace(/\//g, '_')}`
  const file = await shot(page, name, !mobile)
  results.push({ route: r, ms, ...info, problems: sink.splice(0), shot: file })
  log(r || '/', ms + 'ms', info.url, info.title, sink.length, JSON.stringify(results.at(-1).problems).slice(0, 300), info.keys.length ? 'KEYS ' + info.keys.join(' ') : '', info.cjk.length ? 'CJK ' + info.cjk.join(' | ') : '', info.hscroll ? 'HSCROLL' : '')
  page.removeAllListeners('console'); page.removeAllListeners('pageerror'); page.removeAllListeners('response')
}
save(`crawl-${site}-${locale}${mobile ? '-m' : ''}`, results)
await b.close()
