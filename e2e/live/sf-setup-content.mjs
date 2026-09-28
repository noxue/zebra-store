// QA-owned content: published blog (with related product), draft blog, notice, home banner.
import fs from 'node:fs'
import { adm } from './sf-lib.mjs'
const L = (zh, tw, en) => ({ 'zh-CN': zh, 'zh-TW': tw, 'en-US': en })
const out = {}
out.blog = await adm('posts', { method: 'POST', body: { slug: 'qa-sf-howto', type: 'blog', title: L('QA 购买教程', 'QA 購買教學', 'QA How to buy'), summary: L('三步完成购买', '三步完成購買', 'Buy in three steps'), content: L('<h2>步骤</h2><p>选择商品 → 结算 → 支付。</p>', '<h2>步驟</h2><p>選擇商品 → 結算 → 支付。</p>', '<h2>Steps</h2><p>Pick → checkout → pay.</p>'), thumbnail: '', is_published: true, product_ids: [7] } })
out.draft = await adm('posts', { method: 'POST', body: { slug: 'qa-sf-draft', type: 'blog', title: L('QA 草稿', 'QA 草稿', 'QA Draft'), summary: L('草稿', '草稿', 'draft'), content: L('<p>draft</p>', '<p>draft</p>', '<p>draft</p>'), thumbnail: '', is_published: false } })
out.notice = await adm('posts', { method: 'POST', body: { slug: 'qa-sf-notice', type: 'notice', title: L('QA 维护公告', 'QA 維護公告', 'QA Maintenance'), summary: L('今晚维护', '今晚維護', 'Tonight'), content: L('<p>今晚 23:00 维护。</p>', '<p>今晚 23:00 維護。</p>', '<p>Maintenance 23:00.</p>'), thumbnail: '', is_published: true } })
out.banner = await adm('banners', { method: 'POST', body: { name: 'QA-SF banner', position: 'home_hero', title: L('QA 横幅', 'QA 橫幅', 'QA Banner'), subtitle: L('测试副标题', '測試副標題', 'Subtitle'), image: '/uploads/product/2026/09/edc5f243-f747-4b08-837b-62471eb1c28c.png', mobile_image: '/uploads/product/2026/09/11afe449-2f2e-451c-931a-b90d7fb50ec1.png', link_type: 'internal', link_value: '/products/google-account', open_in_new_tab: false, is_active: true, start_at: '', end_at: '', sort_order: 0 } })
for (const [k, v] of Object.entries(out)) console.log(k, v.status_code, v.msg, v.data?.id)
const st = JSON.parse(fs.readFileSync('.state/sf-fixtures.json', 'utf8'))
Object.assign(st, { posts: [out.blog.data?.id, out.draft.data?.id, out.notice.data?.id], banner: out.banner.data?.id })
fs.writeFileSync('.state/sf-fixtures.json', JSON.stringify(st))
