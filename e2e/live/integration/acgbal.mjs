// node acgbal.mjs <member> <target>
import { acgAdmin, acgUser, note } from './lib.mjs'
const [name, target] = process.argv.slice(2); const s = await acgAdmin()
const row = await acgUser(s, name); const diff = Number(target) - Number(row.balance)
if (Math.abs(diff) > 0.001) note('ACG', `recharge ${name} ${diff}: ` + JSON.stringify(await s.post('/admin/api/user/recharge', { id: row.id, action: diff > 0 ? 1 : 0, amount: Math.abs(diff).toFixed(2), log: 'qa', total: 0 })).slice(0, 80))
note('ACG', `${name} balance now ${(await acgUser(s, name)).balance}`)
