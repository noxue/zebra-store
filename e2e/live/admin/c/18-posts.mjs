import { dialog, field, fieldBox, fillLocalized, selectOption, apiResponse, TS } from './common.mjs'
export default async ({ page, api, shot, log, admin, base }) => {
  // post categories
  await page.goto(`${admin}/posts/categories`)
  await page.waitForLoadState('networkidle')
  for (const [nm, parent] of [[`qa-pcat ${TS}`, null], [`qa-pcat-child ${TS}`, `qa-pcat ${TS}`]]) {
    await page.getByRole('button', { name: 'New Category' }).click()
    const d = dialog(page, 'New Category')
    await fillLocalized(fieldBox(d, 'Name'), nm)
    await field(d, 'Slug').fill(nm.replace(/ /g, '-'))
    if (parent) await selectOption(field(d, 'Parent category', 'select'), new RegExp(parent + '$'))
    const r = await apiResponse(page, 'POST', /\/admin\/post-categories$/, () => d.getByRole('button', { name: 'Create now' }).click())
    log('pcat', r.id, r.slug, r.parent_id)
    await page.waitForTimeout(600)
  }
  await shot(page, 'c-postcat-list')
  const pcats = (await api('GET', '/admin/post-categories')).data
  const child = pcats.find((c) => c.slug === `qa-pcat-child-${TS}`)
  // blog post (published) + draft
  await page.goto(`${admin}/posts/blog`)
  await page.waitForLoadState('networkidle')
  const mk = async (title, publish) => {
    await page.getByRole('button', { name: 'New post' }).click()
    const d = dialog(page, 'New post')
    await fillLocalized(fieldBox(d, 'Title'), title)
    await field(d, 'Slug (URL identifier)').fill(title.replace(/ /g, '-'))
    const cs = field(d, 'Category', 'select')
    log('post cat options', (await cs.locator('option').allTextContents()).join('/'))
    await selectOption(cs, new RegExp(`qa-pcat-child ${TS}`))
    const content = fieldBox(d, 'Content')
    await content.locator('.ProseMirror').click(); await page.keyboard.type(`QA body ${title}`)
    const rp = fieldBox(d, 'Related Products')
    await rp.locator('input').first().fill('qa-product')
    await page.waitForTimeout(1500)
    const add = rp.getByRole('button', { name: 'Add' }).first()
    log('related add buttons', await rp.getByRole('button', { name: 'Add' }).count())
    if (await add.count()) await add.click()
    if (!publish) await d.getByRole('switch').last().click()
    const r = await apiResponse(page, 'POST', /\/admin\/posts$/, () => d.getByRole('button', { name: 'Publish now' }).click())
    log('post', r.id, r.slug, r.is_published, r.category_id)
    await page.waitForTimeout(800)
    return r
  }
  const pub1 = await mk(`qa-post ${TS}`, true)
  const draft = await mk(`qa-draft ${TS}`, false)
  await shot(page, 'c-posts-list')
  const pubList = await page.evaluate(async () => (await (await fetch('/api/v1/public/posts?page=1&page_size=50')).json()))
  const slugs = pubList.data.map((x) => x.slug)
  log('public has published', slugs.includes(pub1.slug), 'has draft', slugs.includes(draft.slug))
  const dd = await page.evaluate(async (s) => (await (await fetch(`/api/v1/public/posts/${s}`)).json()), draft.slug)
  log('draft detail public', dd.status_code, dd.msg)
  const rel = await page.evaluate(async (s) => (await (await fetch(`/api/v1/public/posts/${s}`)).json()), pub1.slug)
  log('published detail related', JSON.stringify(rel.data?.related_products?.map?.((x) => x.slug) ?? Object.keys(rel.data || {})).slice(0, 300))
  const prodPosts = await page.evaluate(async (s) => (await (await fetch(`/api/v1/public/products/${s}`)).json()), `qa-product-${TS}`)
  log('product related posts', JSON.stringify(prodPosts.data?.related_posts?.map?.((x) => x.slug) ?? 'n/a'))
  // notice
  const n = await api('POST', '/admin/posts', { type: 'notice', slug: `qa-notice-${TS}`, title: { 'zh-CN': 'qa-notice', 'zh-TW': 'qa-notice', 'en-US': 'qa-notice' }, content: { 'zh-CN': 'x', 'zh-TW': 'x', 'en-US': 'x' }, is_published: true })
  log('notice', n.status_code, n.data?.id)
  const nl = await page.evaluate(async () => (await (await fetch('/api/v1/public/posts?type=notice&page=1&page_size=50')).json()))
  log('public notices include', nl.data.some((x) => x.slug === `qa-notice-${TS}`))
  // deactivate child category -> hidden publicly
  const st = await api('PATCH', `/admin/post-categories/${child.id}/status`, { is_active: false })
  log('deactivate pcat', st.status_code)
  const pc = await page.evaluate(async () => (await (await fetch('/api/v1/public/post-categories')).json()))
  log('public pcats include child', JSON.stringify(pc.data ?? pc).includes(`qa-pcat-child-${TS}`), 'status', pc.status_code)
  // storefront blog page
  const sp = await page.context().newPage()
  await sp.goto(`${base}/blog/${pub1.slug}`); await sp.waitForLoadState('networkidle'); await sp.waitForTimeout(1500)
  await sp.screenshot({ path: 'live/shots/admin/c-sf-blog.png', fullPage: true })
  log('sf blog text has product', (await sp.locator('body').innerText()).includes('qa-product 0926'))
}
