// Creates QA-owned products: manual+form+instructions+affiliate, and a 1-card auto product.
import fs from 'node:fs'
import { adm } from './sf-lib.mjs'
const L = (zh, en) => ({ 'zh-CN': zh, 'zh-TW': zh, 'en-US': en })
const manual = await adm('products', { method: 'POST', body: {
  category_id: 4, slug: 'qa-sf-manual', title: L('QA-SF 人工代充', 'QA-SF Manual Top-up'), description: L('QA 人工发货测试', 'QA manual test'),
  content: L('<p>QA 测试商品</p>', '<p>QA test product</p>'), instructions: L('<p><b>使用说明</b>：登录后在设置里查看。</p>', '<p><b>How to use</b>: see settings.</p>'),
  manual_form_schema: { fields: [
    { key: 'account', type: 'email', required: true, label: L('充值账号', 'Account'), placeholder: L('请输入邮箱账号', 'Email') },
    { key: 'region', type: 'select', required: true, label: L('区服', 'Region'), options: ['亚服', '美服'] },
    { key: 'uid', type: 'text', required: false, label: L('角色ID', 'UID'), regex: '^[0-9]{5,10}$' } ] },
  price_amount: '6.00', purchase_type: 'member', min_purchase_quantity: 1, max_purchase_quantity: 3, stock_display_mode: 'exact', fulfillment_type: 'manual',
  skus: [{ sku_code: 'std', spec_values: L('标准', 'Standard'), price_amount: '6.00', manual_stock_total: 10, is_active: true }],
  is_affiliate_enabled: true, is_active: true, sort_order: 0, tags: ['QA'] } })
const auto = await adm('products', { method: 'POST', body: {
  category_id: 4, slug: 'qa-sf-lastcard', title: L('QA-SF 最后一张卡', 'QA-SF Last Card'), description: L('QA 并发测试', 'QA concurrency'),
  content: L('<p>QA</p>', '<p>QA</p>'), instructions: L('<p>自动发货使用说明 QA</p>', '<p>Auto instructions QA</p>'),
  price_amount: '1.00', purchase_type: 'member', min_purchase_quantity: 1, max_purchase_quantity: 5, stock_display_mode: 'exact', fulfillment_type: 'auto',
  skus: [{ sku_code: 'one', spec_values: L('单张', 'Single'), price_amount: '1.00', is_active: true }],
  is_affiliate_enabled: true, is_active: true, sort_order: 0, tags: ['QA'] } })
console.log('manual', manual.status_code, manual.msg, manual.data?.id, JSON.stringify(manual.data?.skus?.map((s) => s.id)))
console.log('auto', auto.status_code, auto.msg, auto.data?.id, JSON.stringify(auto.data?.skus?.map((s) => s.id)))
if (auto.data?.id) {
  const cs = await adm('card-secrets/batch', { method: 'POST', body: { product_id: auto.data.id, sku_id: auto.data.skus[0].id, secrets: ['QA-SF-LAST-0001'], note: 'QA-SF', deduplicate: true } })
  console.log('cards', cs.status_code, cs.msg, JSON.stringify(cs.data).slice(0, 200))
}
const st = JSON.parse(fs.readFileSync('.state/sf-fixtures.json', 'utf8'))
Object.assign(st, { manual: manual.data?.id, manualSku: manual.data?.skus?.[0]?.id, auto: auto.data?.id, autoSku: auto.data?.skus?.[0]?.id, giftBatch: 2 })
fs.writeFileSync('.state/sf-fixtures.json', JSON.stringify(st))
