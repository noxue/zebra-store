export default async ({ api, log }) => { const r = await api('GET', '/admin/media?page=1&page_size=3'); log(JSON.stringify(r).slice(0, 800)) }
