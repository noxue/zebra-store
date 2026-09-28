export default async ({ api, log }) => {
  for (const p of (process.env.P || '').split(',')) { const r = await api(process.env.M || 'GET', p, process.env.B ? JSON.parse(process.env.B) : undefined); log(p, JSON.stringify(r).slice(0, +(process.env.N || 1500))) }
}
