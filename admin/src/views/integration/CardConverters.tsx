import { computed, defineComponent, onMounted, ref, watch } from 'vue'
import { RefreshCw, TestTube2, Unplug } from 'lucide-vue-next'
import { Badge, Button, FormField, Input, PageHeader, Select } from '@/components/ui'
import { adminAPI } from '@/api/admin'
import type { AdminCardConverter, AdminCardConverterBinding, AdminProduct, AdminProductMapping } from '@/api/types'
import { getLocalizedText } from '@/utils/format'

const standardFields = ['order.order_no', 'order.quantity', 'order.currency', 'site.domain', 'site.name', 'product.id', 'product.slug', 'product.title', 'product.variables', 'sku.id', 'sku.code', 'sku.specifications']

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
    const binding = ref<AdminCardConverterBinding>({ id: 0, product_id: 0, sku_id: 0, converter_id: 0, type_id: '', fields: [...standardFields], extra_template: {} })
    const productOptions = computed(() => products.value.map((p) => {
      const upstream = mappings.value.filter((m) => m.local_product_id === p.id).map((m) => m.connection_name || `#${m.connection_id}`)
      const distinct = [...new Set(upstream)]
      const title = getLocalizedText(p.title) || p.slug
      return { label: `${title} (#${p.id}) · ${p.fulfillment_type === 'upstream' ? `上游：${distinct.join('、') || '未关联'}` : '本地商品'}`, value: p.id }
    }))
    const selectedProduct = computed(() => products.value.find((p) => p.id === binding.value.product_id))
    const selectedSku = computed(() => selectedProduct.value?.skus?.find((sku) => sku.id === binding.value.sku_id))
    const selectedConverter = computed(() => converters.value.find((c) => c.id === binding.value.converter_id))
    const selectedType = computed(() => selectedConverter.value?.types.find((type) => type.id === binding.value.type_id))
    const requestPreview = computed(() => ({
      protocol_version: '1',
      type_id: binding.value.type_id || '<选择类型>',
      order: { order_no: 'ZS-EXAMPLE-ORDER', quantity: 1, currency: 'CNY' },
      site: { domain: '<当前店铺域名>', name: '<当前店铺名称>' },
      product: {
        id: selectedProduct.value?.id ?? 0,
        slug: selectedProduct.value?.slug ?? '',
        title: selectedProduct.value ? getLocalizedText(selectedProduct.value.title) : '',
        variables: '<商品变量快照>',
      },
      sku: {
        id: selectedSku.value?.id ?? 0,
        code: selectedSku.value?.sku_code ?? '',
        specifications: selectedSku.value?.spec_values ?? {},
      },
      items: [{ index: 0, upstream_card: '<订单获得的原始卡密>' }],
      extra: binding.value.extra_template,
    }))
    const skuOptions = computed(() => [{ label: '默认（适用于全部 SKU）', value: 0 }, ...(selectedProduct.value?.skus || []).map((sku) => ({ label: `${sku.sku_code || `SKU #${sku.id}`} (#${sku.id})`, value: sku.id }))])
    const converterOptions = computed(() => converters.value.map((c) => ({ label: `${c.name} · ${c.health}`, value: c.id })))
    const typeOptions = computed(() => (selectedConverter.value?.types || []).map((type) => ({ label: `${type.name} (${type.id})`, value: type.id })))

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
        mappings.value = m.data
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
    const refreshTypes = async (c: AdminCardConverter) => {
      testing.value = c.id
      try {
        await adminAPI.refreshCardConverterTypes(c.id)
        await load()
      } finally {
        testing.value = 0
      }
    }
    const saveBinding = async () => {
      await adminAPI.saveCardConverterBinding(binding.value)
      bindings.value = (await adminAPI.getCardConverterBindings()).data
      binding.value = { id: 0, product_id: 0, sku_id: 0, converter_id: 0, type_id: '', fields: [...standardFields], extra_template: {} }
    }
    const removeBinding = async (row: AdminCardConverterBinding) => {
      await adminAPI.deleteCardConverterBinding(row.product_id, row.sku_id)
      bindings.value = (await adminAPI.getCardConverterBindings()).data
    }
    const duplicateBinding = (row: AdminCardConverterBinding) => {
      binding.value = { ...row, id: 0, extra_template: { ...row.extra_template } }
    }
    const setExtra = (key: string, value: string) => {
      binding.value.extra_template = { ...binding.value.extra_template, [key]: value }
    }

    watch(() => binding.value.converter_id, () => { binding.value.type_id = '' })
    watch(query, () => { void load() })
    onMounted(() => { void load() })

    return () => <div class="space-y-6">
      <PageHeader title="卡密转换器">
        {{ actions: () => <Button loading={busy.value} onClick={() => void load()}><RefreshCw class="h-4 w-4" />刷新</Button> }}
      </PageHeader>
      <section class="rounded-zs-lg border border-line bg-surface p-5 space-y-4">
        <h2 class="font-semibold">{editing.value ? '编辑转换器程序' : '添加转换器程序'}</h2>
        <p class="text-sm text-muted">一个第三方程序配置一次，可供多个商品共用。类型会从程序接口读取；Token 请使用专用低权限凭证。</p>
        <div class="grid gap-3 md:grid-cols-2">
          <FormField label="名称"><Input v-model={form.value.name} placeholder="例如：AI Top-up" /></FormField>
          <FormField label="HTTPS 地址"><Input v-model={form.value.base_url} mono placeholder="https://converter.example.com" /></FormField>
          <FormField label={editing.value ? '专用 Token（留空保持不变）' : '专用 Token'}><Input v-model={form.value.token} type="password" autocomplete="new-password" /></FormField>
          <label class="flex items-center gap-2 text-sm"><input v-model={form.value.enabled} type="checkbox" />启用转换器</label>
        </div>
        <div class="flex gap-2"><Button variant="primary" onClick={() => void saveConverter()}>保存并读取类型</Button>{editing.value > 0 && <Button onClick={resetConverter}>取消编辑</Button>}</div>
      </section>
      <section class="rounded-zs-lg border border-line bg-surface p-5 space-y-3">
        <h2 class="font-semibold">已配置程序</h2>
        {converters.value.map((c) => <article class="flex flex-wrap items-center gap-3 border-b border-line py-3 last:border-0" key={c.id}>
          <div class="min-w-48 flex-1"><div class="font-medium">{c.name} <Badge tone={c.health === 'healthy' ? 'success' : c.health === 'unhealthy' ? 'danger' : 'warning'}>{c.health}</Badge></div><div class="text-xs text-muted break-all">{c.base_url} · {c.types.length} 种类型</div>{c.last_error && <div class="text-xs text-danger-text">{c.last_error}</div>}</div>
          <Button size="sm" onClick={() => editConverter(c)}>编辑</Button><Button size="sm" loading={testing.value === c.id} onClick={() => void refreshTypes(c)}>刷新类型</Button><Button size="sm" loading={testing.value === c.id} onClick={() => void testConverter(c)}><TestTube2 class="h-3.5 w-3.5" />测试</Button><Button size="sm" variant="danger" onClick={() => void adminAPI.deleteCardConverter(c.id).then(load)}>删除</Button>
        </article>)}
      </section>
      <section class="rounded-zs-lg border border-line bg-surface p-5 space-y-4">
        <h2 class="font-semibold">商品绑定</h2>
        <p class="text-sm text-muted">选好商品后，直接从程序返回的类型列表中选择。上游来源只用于辨认商品；手动导入卡密不需要额外标记。</p>
        <div class="grid gap-3 md:grid-cols-2">
          <FormField label="搜索商品"><Input v-model={query.value} placeholder="商品名称或关键词" /></FormField>
          <FormField label="本地商品（显示上游来源）"><Select v-model={binding.value.product_id} options={productOptions.value} placeholder="选择商品" /></FormField>
          <FormField label="SKU"><Select v-model={binding.value.sku_id} options={skuOptions.value} /></FormField>
          <FormField label="转换器程序"><Select v-model={binding.value.converter_id} options={converterOptions.value} placeholder="选择程序" /></FormField>
          <FormField label="程序提供的卡密类型"><Select v-model={binding.value.type_id} options={typeOptions.value} placeholder="先添加程序并读取类型" /></FormField>
        </div>
        {selectedType.value?.description && <p class="text-xs text-muted">{selectedType.value.description}</p>}
        {selectedType.value?.fields?.length ? <div class="grid gap-3 md:grid-cols-2">{selectedType.value.fields.map((field) => <FormField key={field.key} label={`${field.label}${field.required ? ' *' : ''}`} hint={field.description}><Input modelValue={String(binding.value.extra_template[field.key] ?? '')} onUpdate:modelValue={(v: string | number) => setExtra(field.key, String(v))} placeholder={field.required ? '必填' : '可选'} /></FormField>)}</div> : null}
        <div class="space-y-2"><div class="text-sm font-medium">自动传递的标准信息</div><div class="flex flex-wrap gap-2">{['订单号和数量', '站点域名', '商品/SKU信息', '上游卡密'].map((x) => <Badge key={x} tone="info">{x}</Badge>)}</div><p class="text-xs text-muted">邮箱和手机号默认不传。商品名、站点域名等信息无需重复填写。</p></div>
        {binding.value.product_id > 0 && binding.value.converter_id > 0 && <details class="rounded-zs-md border border-line bg-surface-strong p-3"><summary class="cursor-pointer text-sm font-medium">预览转换请求</summary><pre class="mt-3 max-h-80 overflow-auto rounded-zs-sm bg-surface p-3 text-xs leading-relaxed">{JSON.stringify(requestPreview.value, null, 2)}</pre><p class="mt-2 text-xs text-muted">示例值仅供核对字段结构；实际请求会使用订单快照和可信站点设置。</p></details>}
        <Button variant="primary" disabled={!binding.value.product_id || !binding.value.converter_id || !binding.value.type_id} onClick={() => void saveBinding()}>保存商品绑定</Button>
        <div class="space-y-2 pt-3">{bindings.value.map((row) => {
          const p = products.value.find((item) => item.id === row.product_id)
          const c = converters.value.find((item) => item.id === row.converter_id)
          const title = p ? getLocalizedText(p.title) || p.slug : `商品 #${row.product_id}`
          const type = c?.types.find((item) => item.id === row.type_id)
          const source = mappings.value.filter((m) => m.local_product_id === row.product_id).map((m) => m.connection_name || `上游 #${m.connection_id}`)
          return <article key={`${row.product_id}-${row.sku_id}`} class="flex flex-wrap items-center gap-3 rounded-zs-md border border-line p-3"><div class="min-w-48 flex-1"><div class="font-medium">{title} {row.sku_id ? `· SKU #${row.sku_id}` : '· 默认 SKU'}</div><div class="text-xs text-muted">{source.length ? `上游：${[...new Set(source)].join('、')}` : '本地商品'} · {c?.name || `程序 #${row.converter_id}`} · {type?.name || row.type_id}</div></div><Button size="sm" onClick={() => duplicateBinding(row)}>复制配置</Button><Button size="sm" variant="danger" onClick={() => void removeBinding(row)}><Unplug class="h-3.5 w-3.5" />解绑</Button></article>
        })}</div>
      </section>
    </div>
  },
})
