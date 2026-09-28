// F-005 list template, F-011 theme primary colour, F-084 contact links; site_config restored field-by-field.
import { launch, open, go, shot, record, S, sleep, noAnnouncement, api, adm } from './sf-lib.mjs'
const orig = (await adm('settings?key=site_config')).data
const patch = async (fn) => { const cur = (await adm('settings?key=site_config')).data; const next = fn(structuredClone(cur)); return adm('settings', { method: 'PUT', body: { key: 'site_config', value: next } }) }
const waitCfg = async (pred) => { for (let i = 0; i < 20; i++) { if (pred((await api('public/config')).data)) return true; await sleep(1500) } return false }
const b = await launch(); const R = {}
try {
  R.p1 = (await patch((c) => { c.template_mode = 'list'; c.theme.primary_color = '#16a34a'; c.contact = { telegram: 'https://t.me/qa_sf_test', whatsapp: 'https://wa.me/10000000000' }; return c })).status_code
  R.applied = await waitCfg((c) => c.template_mode === 'list' && c.contact.telegram)
  for (const mobile of [false, true]) {
    const m = mobile ? 'm' : 'd'
    const { ctx, page, diag } = await open(b, { mobile }); await noAnnouncement(ctx)
    await go(page, S + '/', diag); await sleep(2000)
    const primary = await page.evaluate(() => getComputedStyle(document.documentElement).getPropertyValue('--zs-primary') || getComputedStyle(document.documentElement).getPropertyValue('--color-primary'))
    const s1 = await shot(page, `F005-list-home-${m}`, true)
    await go(page, S + '/products', diag); await sleep(1500)
    const s2 = await shot(page, `F005-list-products-${m}`, true)
    const foot = (await page.locator('footer').innerText().catch(() => '')).replace(/\s+/g, ' ')
    const links = await page.evaluate(() => [...document.querySelectorAll('footer a, a')].map((a) => a.href).filter((h) => /t\.me|wa\.me/.test(h)))
    await go(page, S + '/about', diag); await sleep(1500)
    const s3 = await shot(page, `F084-about-contact-${m}`, true)
    const aboutLinks = await page.evaluate(() => [...document.querySelectorAll('a')].map((a) => a.href).filter((h) => /t\.me|wa\.me/.test(h)))
    R[m] = { primary, foot: foot.slice(0, 300), links, aboutLinks, diag, shots: [s1, s2, s3] }
    await ctx.close()
  }
} catch (e) { R.error = e.message.slice(0, 300) } finally {
  R.restore = (await patch((c) => { c.template_mode = orig.template_mode; c.theme.primary_color = orig.theme.primary_color; c.contact = orig.contact; return c })).status_code
  R.after = (await adm('settings?key=site_config')).data
  R.afterCheck = [R.after.template_mode, R.after.theme.primary_color, JSON.stringify(R.after.contact)]
  delete R.after
  record({ flow: 'F-005/F-011/F-084', R })
}
await b.close()
