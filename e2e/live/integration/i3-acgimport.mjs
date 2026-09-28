import { acgAdmin, load, save, note } from './lib.mjs'
const st = load(); const s = await acgAdmin()
const shopId = Number(process.argv[2]); const codes = process.argv.slice(3)
const r = await s.post(`/admin/api/store/addItem?storeId=${shopId}`, { category_mode: 0, category_id: 3, 'item_codes[]': codes, premium: 1, premium_type: 0, shelves: 1, shared_sync: 0, shared_amount_sync: 0, shared_config_sync: 0, image_download: 0, resume_import: 1 })
note('I-031', `ACG import codes ${codes} from shop #${shopId}: ${JSON.stringify(r).slice(0, 200)}`)
const rows = (await s.post('/admin/api/commodity/data', { page: 1, limit: 50, 'equal-shared_id': shopId })).data.list
for (const x of rows) note('I-031', `ACG commodity #${x.id} "${x.name}" price=${x.price} user_price=${x.user_price} shared_code=${x.shared_code} config=${String(x.config).replace(/\n/g, '\\n').slice(0, 200)} stock=${x.stock}`)
st.acgImported = { ...(st.acgImported || {}), [shopId]: rows.map((x) => ({ id: x.id, name: x.name, price: x.user_price })) }; save(st)
