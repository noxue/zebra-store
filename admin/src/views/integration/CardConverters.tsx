import { computed, defineComponent, onMounted, ref, watch } from 'vue'
import { RefreshCw, TestTube2, Unplug } from 'lucide-vue-next'
import { Badge, Button, FormField, Input, PageHeader, Select } from '@/components/ui'
import { adminAPI } from '@/api/admin'
import type { AdminCardConverter, AdminCardConverterBinding, AdminProduct, AdminProductMapping } from '@/api/types'
import { getLocalizedText } from '@/utils/format'
import CardConverterExtraEditor from './CardConverterExtraEditor'

export default defineComponent({
  name: 'CardConvertersView',
  setup() {
    const converters = ref<AdminCardConverter[]>([])
    const products = ref<AdminProduct[]>([])
    const mappings = ref<AdminProductMapping[]>([])
    const bindings = ref<AdminCardConverterBinding[]>([])
    const busy = ref(false)
    const testing = ref(0)
    const editing = ref(0)
    const query = ref('')
    const form = ref({ name: '', base_url: '', token: '', enabled: true })
    const binding = ref<AdminCardConverterBinding>({ id: 0, product_id: 0, sku_id: 0, converter_id: 0, extra_template: {} })
    const extraEditor = ref<{ validate: () => Record<string, unknown> | null } | null>(null)
    const editorRevision = ref(0)
    const productOptions = computed(() => products.value.map((p) => {
      const upstream = mappings.value.filter((m) => m.local_product_id === p.id).map((m) => m.connection_name || `#${m.connection_id}`)
      const distinct = [...new Set(upstream)]
      const title = getLocalizedText(p.title) || p.slug
      return { label: `${title} (#${p.id}) · ${p.fulfillment_type === 'upstream' ? `上游：${distinct.join('、') || '未关联'}` : '本地商品'}`, value: p.id }
    }))
    const selectedProduct = computed(() => products.value.find((p) => p.id === binding.value.product_id))
    const requestPreview = computed(() => ({
      order_no: '<订单号>',
      items: [{ index: 0, upstream_card: '<订单获得的原始卡密>' }],
      extra: binding.value.extra_template,
    }))
    const skuOptions = computed(() => [{ label: '默认（适用于全部 SKU）', value: 0 }, ...(selectedProduct.value?.skus || []).map((sku) => ({ label: `${sku.sku_code || `SKU #${sku.id}`} (#${sku.id})`, value: sku.id }))])
    const converterOptions = computed(() => converters.value.map((c) => ({ label: `${c.name} · ${c.health}`, value: c.id })))
    const sourceOptions = computed(() => {
      const rows = mappings.value.filter((row) => row.local_product_id === binding.value.product_id)
      return [...new Map(rows.map((row) => [row.connection_id, { id: row.connection_id, name: row.connection_name || `上游 #${row.connection_id}` }])).values()]
    })

    const load = async () => {
      busy.value = true
      try {
        const [c, p, m, b] = await Promise.all([
          adminAPI.getCardConverters(),
          adminAPI.getProducts({ page: 1, page_size: 100, search: query.value || undefined }),
          adminAPI.getProductMappings({ page: 1, page_size: 200 }),
          adminAPI.getCardConverterBindings(),
        ])
        converters.value = c.data
        products.value = p.data
        const allMappings = [...m.data]
        for (let page = 2; page <= (m.pagination?.total_page ?? 1); page++) {
          const next = await adminAPI.getProductMappings({ page, page_size: 200 })
          allMappings.push(...next.data)
        }
        mappings.value = allMappings
        bindings.value = b.data
      } finally {
        busy.value = false
      }
    }

    const resetConverter = () => {
      editing.value = 0
      form.value = { name: '', base_url: '', token: '', enabled: true }
    }
    const editConverter = (c: AdminCardConverter) => {
      editing.value = c.id
      form.value = { name: c.name, base_url: c.base_url, token: '', enabled: c.enabled }
    }
    const saveConverter = async () => {
      const data = { ...form.value }
      if (editing.value) await adminAPI.updateCardConverter(editing.value, data)
      else await adminAPI.createCardConverter(data)
      resetConverter()
      await load()
    }
    const testConverter = async (c: AdminCardConverter) => {
      testing.value = c.id
      try {
        await adminAPI.testCardConverter(c.id)
        await load()
      } finally {
        testing.value = 0
      }
    }
    const saveBinding = async () => {
      const extra = extraEditor.value?.validate()
      if (!extra) return
      await adminAPI.saveCardConverterBinding({ ...binding.value, extra_template: extra })
      bindings.value = (await adminAPI.getCardConverterBindings()).data
      binding.value = { id: 0, product_id: 0, sku_id: 0, converter_id: 0, extra_template: {} }
      editorRevision.value++
    }
    const removeBinding = async (row: AdminCardConverterBinding) => {
      await adminAPI.deleteCardConverterBinding(row.product_id, row.sku_id)
      bindings.value = (await adminAPI.getCardConverterBindings()).data
    }
    const duplicateBinding = (row: AdminCardConverterBinding) => {
      binding.value = { ...row, id: 0, extra_template: { ...row.extra_template } }
      editorRevision.value++
    }
    const editBinding = (row: AdminCardConverterBinding) => {
      binding.value = { ...row, extra_template: { ...row.extra_template } }
      editorRevision.value++
    }

    watch(query, () => { void load() })
    onMounted(() => { void load() })

    return () => <div class="space-y-6">
      <PageHeader title="卡密转换器">
        {{ actions: () => <Button loading={busy.value} onClick={() => void load()}><RefreshCw class="h-4 w-4" />刷新</Button> }}
      </PageHeader>
      <section class="rounded-zs-lg border border-line bg-surface p-5 space-y-4">
        <h2 class="font-semibold">{editing.value ? '编辑转换器程序' : '添加转换器程序'}</h2>
        <p class="text-sm text-muted">一个第三方程序配置一次，可供多个商品共用。套餐类型、上游来源等参数在商品的 extra 中配置；Token 请使用专用低权限凭证。</p>
        <div class="grid gap-3 md:grid-cols-2">
          <FormField label="名称"><Input v-model={form.value.name} placeholder="例如：AI Top-up" /></FormField>
          <FormField label="HTTPS 地址"><Input v-model={form.value.base_url} mono placeholder="https://converter.example.com" /></FormField>
          <FormField label={editing.value ? '专用 Token（留空保持不变）' : '专用 Token'}><Input v-model={form.value.token} type="password" autocomplete="new-password" /></FormField>
          <label class="flex items-center gap-2 text-sm"><input v-model={form.value.enabled} type="checkbox" />启用转换器</label>
        </div>
        <div class="flex gap-2"><Button variant="primary" onClick={() => void saveConverter()}>保存转换器</Button>{editing.value > 0 && <Button onClick={resetConverter}>取消编辑</Button>}</div>
      </section>
      <section class="rounded-zs-lg border border-line bg-surface p-5 space-y-3">
        <h2 class="font-semibold">已配置程序</h2>
        {converters.value.map((c) => <article class="flex flex-wrap items-center gap-3 border-b border-line py-3 last:border-0" key={c.id}>
          <div class="min-w-48 flex-1"><div class="font-medium">{c.name} <Badge tone={c.health === 'healthy' ? 'success' : c.health === 'unhealthy' ? 'danger' : 'warning'}>{c.health}</Badge></div><div class="text-xs text-muted break-all">{c.base_url}</div>{c.last_error && <div class="text-xs text-danger-text">{c.last_error}</div>}</div>
          <Button size="sm" onClick={() => editConverter(c)}>编辑</Button><Button size="sm" loading={testing.value === c.id} onClick={() => void testConverter(c)}><TestTube2 class="h-3.5 w-3.5" />测试</Button><Button size="sm" variant="danger" onClick={() => void adminAPI.deleteCardConverter(c.id).then(load)}>删除</Button>
        </article>)}
      </section>
      <section class="rounded-zs-lg border border-line bg-surface p-5 space-y-4">
        <h2 class="font-semibold">商品绑定</h2>
        <p class="text-sm text-muted">选择商品和转换器后，直接在 extra 中配置程序需要的字段。可以按商品固定套餐，也可按上游来源或卡密格式设置条件。每次打开或刷新本页都会重新检查绑定；商品、SKU、上游或转换器变更后，问题会显示在下方。订单与站点等变量在交付时取实时值。</p>
        <div class="grid gap-3 md:grid-cols-2">
          <FormField label="搜索商品"><Input v-model={query.value} placeholder="商品名称或关键词" /></FormField>
          <FormField label="本地商品（显示上游来源）"><Select v-model={binding.value.product_id} options={productOptions.value} placeholder="选择商品" /></FormField>
          <FormField label="SKU"><Select v-model={binding.value.sku_id} options={skuOptions.value} /></FormField>
          <FormField label="转换器程序"><Select modelValue={binding.value.converter_id} options={converterOptions.value} onUpdate:modelValue={(value) => { binding.value.converter_id = Number(value) }} placeholder="选择程序" /></FormField>
        </div>
        <CardConverterExtraEditor key={editorRevision.value} ref={extraEditor} modelValue={binding.value.extra_template} sources={sourceOptions.value} onUpdate:modelValue={(value) => { binding.value.extra_template = value }} />
        {binding.value.product_id > 0 && binding.value.converter_id > 0 && <details class="rounded-zs-md border border-line bg-surface-strong p-3"><summary class="cursor-pointer text-sm font-medium">预览转换请求模板</summary><pre class="mt-3 max-h-80 overflow-auto rounded-zs-sm bg-surface p-3 text-xs leading-relaxed">{JSON.stringify(requestPreview.value, null, 2)}</pre><p class="mt-2 text-xs text-muted">交付时会先按真实订单和卡密求值，将条件规则转换成普通 JSON，再发送请求。</p></details>}
        <Button variant="primary" disabled={!binding.value.product_id || !binding.value.converter_id} onClick={() => void saveBinding()}>保存商品绑定</Button>
        <div class="space-y-2 pt-3">{bindings.value.map((row) => {
          const p = products.value.find((item) => item.id === row.product_id)
          const c = converters.value.find((item) => item.id === row.converter_id)
          const title = p ? getLocalizedText(p.title) || p.slug : `商品 #${row.product_id}`
          const source = mappings.value.filter((m) => m.local_product_id === row.product_id).map((m) => m.connection_name || `上游 #${m.connection_id}`)
          return <article key={`${row.product_id}-${row.sku_id}`} class="flex flex-wrap items-center gap-3 rounded-zs-md border border-line p-3"><div class="min-w-48 flex-1"><div class="font-medium">{title} {row.sku_id ? `· SKU #${row.sku_id}` : '· 默认 SKU'} {row.issues?.length ? <Badge tone="warning">需检查 extra</Badge> : null}</div><div class="text-xs text-muted">{source.length ? `上游：${[...new Set(source)].join('、')}` : '本地商品'} · {c?.name || `程序 #${row.converter_id}`}</div>{row.issues?.map((issue) => <div key={issue} class="mt-1 text-xs text-warning-text">⚠ {issue}</div>)}</div><Button size="sm" onClick={() => editBinding(row)}>编辑</Button><Button size="sm" onClick={() => duplicateBinding(row)}>复制配置</Button><Button size="sm" variant="danger" onClick={() => void removeBinding(row)}><Unplug class="h-3.5 w-3.5" />解绑</Button></article>
        })}</div>
      </section>
    </div>
  },
})
