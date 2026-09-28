// F-040 cart: add two products, change qty (min/max), delete + undo, persistence.
import { launch, open, go, shot, record, S, sleep, noAnnouncement } from './sf-lib.mjs'
const b = await launch()
const dump = (p) => p.evaluate(() => [...document.querySelectorAll('input,button')].filter((e) => e.offsetParent !== null).map((e) => `${e.tagName}:${e.getAttribute('aria-label') || ''}:${(e.value || e.innerText || '').trim().slice(0, 25)}`).join(' | '))
for (const mobile of [false, true]) {
  const m = mobile ? 'm' : 'd'
  const { ctx, page, diag } = await open(b, { mobile, storage: `.state/sf-${m}.json` }); await noAnnouncement(ctx)
  const toasts = []
  await go(page, S + '/products/google-account', diag); await sleep(800)
  await page.getByRole('button', { name: /加入购物车/ }).first().click(); await sleep(1200)
  toasts.push((await page.locator('[role=status], [class*=toast]').allInnerTexts()).join(' / '))
  const sAdd = await shot(page, `F040-added-toast-${m}`)
  await go(page, S + '/products/claude-pro', diag); await sleep(800)
  await page.getByRole('button', { name: /加入购物车/ }).first().click(); await sleep(1000)
  await go(page, S + '/cart', diag); await sleep(1000)
  const s1 = await shot(page, `F040-cart-${m}`, true)
  const els = await dump(page)
  const ls = await page.evaluate(() => localStorage.getItem('cart_items'))
  // increase qty of google-account up to beyond max (5)
  const incs = page.getByRole('button', { name: /增加|加|\+|increase/i })
  let qtyVals = []
  const qtyInputs = page.locator('input[type=number], input[inputmode=numeric]')
  for (let i = 0; i < 6; i++) { await incs.first().click().catch(() => {}); await sleep(400) }
  qtyVals.push(await qtyInputs.first().inputValue().catch(() => '?'))
  const s2 = await shot(page, `F040-cart-max-${m}`)
  // type an out-of-range value
  await qtyInputs.first().fill('99'); await qtyInputs.first().blur(); await sleep(800)
  qtyVals.push(await qtyInputs.first().inputValue().catch(() => '?'))
  await qtyInputs.first().fill('0'); await qtyInputs.first().blur(); await sleep(800)
  qtyVals.push(await qtyInputs.first().inputValue().catch(() => '?'))
  const s3 = await shot(page, `F040-cart-typed-${m}`)
  // delete + undo
  const del = page.getByRole('button', { name: /删除|移除|remove/i }).first()
  const hasDel = await del.isVisible().catch(() => false)
  let undoSeen = false, afterUndo = ''
  if (hasDel) {
    await del.click(); await sleep(700)
    const s4 = await shot(page, `F040-cart-deleted-${m}`)
    const undo = page.getByRole('button', { name: /撤销|undo/i }).first()
    undoSeen = await undo.isVisible().catch(() => false)
    if (undoSeen) { await undo.click(); await sleep(700) }
    afterUndo = await page.evaluate(() => localStorage.getItem('cart_items'))
  }
  await page.reload(); await sleep(1500)
  const afterReload = await page.evaluate(() => localStorage.getItem('cart_items'))
  const s5 = await shot(page, `F040-cart-reload-${m}`, true)
  record({ flow: 'F-040', mobile, toasts, els, ls, qtyVals, hasDel, undoSeen, afterUndo, afterReload, diag, shots: [sAdd, s1, s2, s3, s5] })
  await ctx.storageState({ path: `.state/sf-${m}.json` })
  await ctx.close()
}
await b.close()
