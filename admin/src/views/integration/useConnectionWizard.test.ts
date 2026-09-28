// Supplier-adapter UI logic: protocol registry, data-driven connection form, connection code
// parsing and handshake. A third fake protocol with a different field set proves nothing is
// hard-coded to specific protocol identifiers.
import { reactive } from 'vue'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { SiteConnectionHandshakeResult, SiteConnectionProtocolDef } from '@/api/types'
import { ApiError } from '@/api/client'
import i18n from '@/i18n'

const getSiteConnectionProtocols = vi.fn()
const parseSiteConnectionCode = vi.fn()
const handshakeSiteConnection = vi.fn()
const createSiteConnection = vi.fn()
const getSiteConnections = vi.fn()

vi.mock('@/api/admin', () => ({
  adminAPI: {
    getSiteConnectionProtocols: () => getSiteConnectionProtocols(),
    parseSiteConnectionCode: (code: string) => parseSiteConnectionCode(code),
    handshakeSiteConnection: (data: unknown) => handshakeSiteConnection(data),
    createSiteConnection: (data: unknown) => createSiteConnection(data),
    getSiteConnections: (params: unknown) => getSiteConnections(params),
  },
}))
vi.mock('@/utils/notify', () => ({ notifySuccess: vi.fn(), notifyError: vi.fn() }))
vi.mock('@/utils/confirm', () => ({ confirmAction: vi.fn(() => Promise.resolve(true)) }))

const { useConnectionWizard } = await import('./useConnectionWizard')
const { useProtocolRegistry } = await import('./useProtocolRegistry')
const { useSiteConnections } = await import('./useSiteConnections')
const { emptySiteConnectionForm, pickProtocolConfig, validateProtocolConfig, buildSiteConnectionPayload, defaultProtocolId, featureLabelKey } = await import('./integrationUtils')

const t = (key: string, params?: Record<string, unknown>) => (params ? i18n.global.t(key, params) : i18n.global.t(key))
const lt = (s: string) => ({ 'zh-CN': s, 'zh-TW': s, 'en-US': s })
const classic = (id: string, code: boolean, capabilities: string[]): SiteConnectionProtocolDef => ({
  id,
  name: lt(id),
  description: lt(`${id} description`),
  fields: [
    { key: 'base_url', label: lt('Base URL'), kind: 'url', required: true },
    { key: 'api_key', label: lt('API Key'), kind: 'text', required: true },
    { key: 'api_secret', label: lt('API Secret'), kind: 'secret', required: true },
  ],
  capabilities,
  supports_connection_code: code,
})
/** A third, made-up adapter with a completely different field set. */
const acme: SiteConnectionProtocolDef = {
  id: 'acme-shop',
  name: lt('Acme Shop'),
  description: lt('fake adapter'),
  fields: [
    { key: 'endpoint', label: lt('Endpoint'), kind: 'url', required: true },
    { key: 'merchant_id', label: lt('Merchant ID'), kind: 'text', required: true },
    { key: 'token', label: lt('Token'), kind: 'secret', required: true },
    {
      key: 'region',
      label: lt('Region'),
      kind: 'select',
      required: false,
      options: [
        { value: 'eu', label: lt('EU') },
        { value: 'us', label: lt('US') },
      ],
    },
  ],
  capabilities: ['changes', 'teleport'],
  supports_connection_code: false,
}
const REGISTRY = [classic('dujiao-next', false, []), classic('zebra-store', true, ['changes', 'webhooks', 'quote']), acme]
const resolve = (id: string) => REGISTRY.find((p) => p.id === id) ?? REGISTRY[0]!

const handshakeOk = (extra: Partial<SiteConnectionHandshakeResult> = {}): { data: SiteConnectionHandshakeResult } => ({
  data: {
    ok: true,
    protocol: 'zebra-store',
    version: '1.0',
    site: { name: 'Upstream Shop', url: 'https://up.example.com', currency: 'USD' },
    features: ['changes', 'webhooks', 'quote', 'multi_item', 'idempotency', 'encrypted_delivery'],
    limits: { requests_per_minute: 120 },
    account: { balance: '100.00', currency: 'USD' },
    suggested_exchange_rate: '7.100000',
    suggested_callback_url: 'https://me.example.com/api/v1/zs/events',
    ...extra,
  },
})

const flush = () => new Promise((r) => setTimeout(r, 0))

beforeEach(() => {
  vi.clearAllMocks()
  getSiteConnections.mockResolvedValue({ status_code: 0, msg: '', data: [] })
})

describe('useConnectionWizard — connection code', () => {
  it('parses a pasted code, fills the form and handshakes automatically', async () => {
    parseSiteConnectionCode.mockResolvedValue({
      data: { name: 'Supplier A', base_url: 'https://up.example.com', api_key: 'key1', api_secret: 'sec1', protocol: 'zebra-store' },
    })
    handshakeSiteConnection.mockResolvedValue(handshakeOk())
    const form = reactive(emptySiteConnectionForm('dujiao-next'))
    const w = useConnectionWizard(form, resolve)
    w.code.value = '  zsc1_abc\n def '

    expect(await w.parseCode()).toBe(true)
    expect(parseSiteConnectionCode).toHaveBeenCalledWith('zsc1_abcdef')
    expect(form.protocol).toBe('zebra-store')
    expect(form.name).toBe('Supplier A')
    expect(form.config).toEqual({ base_url: 'https://up.example.com', api_key: 'key1', api_secret: 'sec1' })
    expect(handshakeSiteConnection).toHaveBeenCalledWith({
      base_url: 'https://up.example.com',
      api_key: 'key1',
      api_secret: 'sec1',
      protocol: 'zebra-store',
      config: { base_url: 'https://up.example.com', api_key: 'key1', api_secret: 'sec1' },
    })
    expect(w.result.value?.site.currency).toBe('USD')
    expect(w.error.value).toBeNull()
    expect(w.parsing.value).toBe(false)
    expect(w.testing.value).toBe(false)
  })

  it('does not re-parse the same code on blur after paste', async () => {
    parseSiteConnectionCode.mockResolvedValue({ data: { name: '', base_url: 'https://a.io', api_key: 'k', api_secret: 's', protocol: 'zebra-store' } })
    handshakeSiteConnection.mockResolvedValue(handshakeOk())
    const w = useConnectionWizard(reactive(emptySiteConnectionForm()), resolve)
    w.code.value = 'zsc1_x'
    await w.parseCode()
    expect(await w.parseCode()).toBe(false)
    expect(parseSiteConnectionCode).toHaveBeenCalledTimes(1)
  })

  it('reads the clipboard on paste', async () => {
    parseSiteConnectionCode.mockResolvedValue({ data: { name: '', base_url: 'https://a.io', api_key: 'k', api_secret: 's', protocol: 'zebra-store' } })
    handshakeSiteConnection.mockResolvedValue(handshakeOk())
    const w = useConnectionWizard(reactive(emptySiteConnectionForm()), resolve)
    const preventDefault = vi.fn()
    w.onCodePaste({ clipboardData: { getData: () => ' zsc1_pasted ' }, preventDefault } as unknown as ClipboardEvent)
    await flush()
    expect(preventDefault).toHaveBeenCalled()
    expect(w.code.value).toBe('zsc1_pasted')
    expect(parseSiteConnectionCode).toHaveBeenCalledWith('zsc1_pasted')
  })

  it('shows a friendly error with the server detail when the code is rejected', async () => {
    parseSiteConnectionCode.mockRejectedValue(new ApiError('连接码格式错误', 400, 200, false))
    const form = reactive(emptySiteConnectionForm('zebra-store'))
    const w = useConnectionWizard(form, resolve)
    w.code.value = 'garbage'
    expect(await w.parseCode()).toBe(false)
    expect(w.error.value).toEqual({ title: t('siteConnections.wizard.invalidCode'), detail: '连接码格式错误', hint: t('siteConnections.wizard.invalidCodeHint') })
    expect(handshakeSiteConnection).not.toHaveBeenCalled()
    expect(form.config).toEqual({})
  })

  it('merges adapter config returned by parse-code', async () => {
    parseSiteConnectionCode.mockResolvedValue({
      data: { name: 'x', base_url: 'https://a.io', api_key: 'k', api_secret: 's', protocol: 'zebra-store', config: { tier: 2, flag: true, nested: { a: 1 } } },
    })
    handshakeSiteConnection.mockResolvedValue(handshakeOk())
    const form = reactive(emptySiteConnectionForm())
    const w = useConnectionWizard(form, resolve)
    w.code.value = 'zsc1_y'
    await w.parseCode()
    expect(form.config).toEqual({ tier: '2', flag: 'true', base_url: 'https://a.io', api_key: 'k', api_secret: 's' })
  })
})

describe('useConnectionWizard — handshake / 测试连接', () => {
  it('lists the missing required fields of the selected (fake) protocol without calling the API', async () => {
    const form = reactive({ ...emptySiteConnectionForm('acme-shop'), config: { endpoint: 'https://acme.io' } })
    const w = useConnectionWizard(form, resolve)
    expect(await w.handshake()).toBe(false)
    expect(handshakeSiteConnection).not.toHaveBeenCalled()
    expect(w.error.value?.title).toBe(t('siteConnections.wizard.missingTitle'))
    expect(w.error.value?.detail).toBe(t('siteConnections.wizard.missingDetail', { fields: ['Merchant ID', 'Token'].join(t('siteConnections.wizard.fieldSep')) }))
  })

  it('sends only the fake protocol fields as config (stale keys dropped)', async () => {
    handshakeSiteConnection.mockResolvedValue(handshakeOk({ protocol: 'acme-shop' }))
    const form = reactive({
      ...emptySiteConnectionForm('acme-shop'),
      config: { base_url: 'https://old.io', endpoint: ' https://acme.io ', merchant_id: 'm-1', token: 'tok', region: 'eu' },
    })
    const w = useConnectionWizard(form, resolve)
    expect(await w.handshake()).toBe(true)
    expect(handshakeSiteConnection).toHaveBeenCalledWith({
      base_url: '',
      api_key: '',
      api_secret: '',
      protocol: 'acme-shop',
      config: { endpoint: 'https://acme.io', merchant_id: 'm-1', token: 'tok', region: 'eu' },
    })
    expect(form.name).toBe('Upstream Shop')
  })

  it('works for dujiao-next with the current form values and keeps a typed name', async () => {
    handshakeSiteConnection.mockResolvedValue(handshakeOk({ protocol: 'dujiao-next', features: [], suggested_exchange_rate: null, suggested_callback_url: null }))
    const form = reactive({ ...emptySiteConnectionForm('dujiao-next'), name: 'Mine', config: { base_url: 'https://d.io', api_key: 'k', api_secret: 's' } })
    const w = useConnectionWizard(form, resolve)
    expect(await w.handshake()).toBe(true)
    expect(handshakeSiteConnection.mock.calls[0]?.[0]).toMatchObject({ protocol: 'dujiao-next', base_url: 'https://d.io' })
    expect(form.name).toBe('Mine')
    expect(w.suggestedRate()).toBeNull()
  })

  it('renders ok:false as a failure with the backend reason', async () => {
    handshakeSiteConnection.mockResolvedValue({ data: { ...handshakeOk().data, ok: false, error: '签名校验失败（401 unauthorized）' } })
    const form = reactive({ ...emptySiteConnectionForm('zebra-store'), config: { base_url: 'https://a.io', api_key: 'k', api_secret: 'bad' } })
    const w = useConnectionWizard(form, resolve)
    expect(await w.handshake()).toBe(false)
    expect(w.result.value).toBeNull()
    expect(w.error.value).toEqual({
      title: t('siteConnections.wizard.handshakeFailed'),
      detail: '签名校验失败（401 unauthorized）',
      hint: t('siteConnections.wizard.handshakeFailedHint'),
    })
  })

  it('falls back to a helpful hint when the request fails without a message', async () => {
    handshakeSiteConnection.mockRejectedValue(new Error(''))
    const form = reactive({ ...emptySiteConnectionForm('zebra-store'), config: { base_url: 'https://a.io', api_key: 'k', api_secret: 's' } })
    const w = useConnectionWizard(form, resolve)
    await w.handshake()
    expect(w.error.value?.detail).toBe(t('siteConnections.wizard.handshakeFailedHint'))
    expect(w.testing.value).toBe(false)
  })

  it('applies the suggested exchange rate and callback URL', async () => {
    handshakeSiteConnection.mockResolvedValue(handshakeOk())
    const form = reactive({ ...emptySiteConnectionForm('zebra-store'), config: { base_url: 'https://a.io', api_key: 'k', api_secret: 's' } })
    const w = useConnectionWizard(form, resolve)
    await w.handshake()
    expect(w.suggestedRate()).toBe(7.1)
    w.applyExchangeRate()
    w.applyCallbackUrl()
    expect(form.exchange_rate).toBe(7.1)
    expect(form.callback_url).toBe('https://me.example.com/api/v1/zs/events')
  })

  it('ignores a response that arrives after the dialog was reset', async () => {
    let finish: (v: unknown) => void = () => {}
    handshakeSiteConnection.mockReturnValue(new Promise((r) => (finish = r)))
    const form = reactive({ ...emptySiteConnectionForm('zebra-store'), config: { base_url: 'https://a.io', api_key: 'k', api_secret: 's' } })
    const w = useConnectionWizard(form, resolve)
    const pending = w.handshake()
    expect(w.testing.value).toBe(true)
    w.reset()
    finish(handshakeOk())
    expect(await pending).toBe(false)
    expect(w.result.value).toBeNull()
    expect(w.testing.value).toBe(false)
    expect(form.name).toBe('')
  })
})

describe('data-driven connection form', () => {
  const messages = { required: 'REQ', url: 'URL' }

  it('validates required / url / select per protocol definition', () => {
    expect(validateProtocolConfig(acme, {}, messages)).toEqual({ endpoint: 'REQ', merchant_id: 'REQ', token: 'REQ' })
    expect(validateProtocolConfig(acme, { endpoint: 'acme.io', merchant_id: 'm', token: 't', region: 'mars' }, messages)).toEqual({ endpoint: 'URL', region: 'REQ' })
    expect(validateProtocolConfig(acme, { endpoint: 'https://acme.io', merchant_id: 'm', token: 't' }, messages)).toEqual({})
  })

  it('allows a blank secret while editing (keep the stored one)', () => {
    expect(validateProtocolConfig(acme, { endpoint: 'https://acme.io', merchant_id: 'm' }, messages, { editing: true })).toEqual({})
    expect(validateProtocolConfig(resolve('zebra-store'), { base_url: 'https://a.io', api_key: 'k' }, messages, { editing: true })).toEqual({})
  })

  it('builds the payload from the fake protocol fields only', () => {
    const form = { ...emptySiteConnectionForm('acme-shop'), name: 'Acme', config: { api_key: 'stale', endpoint: 'https://acme.io', merchant_id: 'm', token: 't' } }
    const payload = buildSiteConnectionPayload(form, acme)
    expect(payload.config).toEqual({ endpoint: 'https://acme.io', merchant_id: 'm', token: 't', region: '' })
    expect(payload.api_key).toBe('')
    expect(payload.protocol).toBe('acme-shop')
    expect(pickProtocolConfig({ a: '1' }, null)).toEqual({ a: '1' })
  })

  it('maps wire features and registry capability ids to the same localized labels; unknown stay raw', () => {
    expect(featureLabelKey('changes')).toBe('siteConnections.features.changes')
    expect(featureLabelKey('incremental_changes')).toBe('siteConnections.features.changes')
    expect(featureLabelKey('push_events')).toBe('siteConnections.features.webhooks')
    expect(featureLabelKey('encrypted_delivery')).toBe('siteConnections.features.encryptedDelivery')
    expect(featureLabelKey('teleport')).toBeNull()
    for (const locale of ['zh-CN', 'zh-TW', 'en-US'] as const) {
      for (const k of ['changes', 'webhooks', 'quote', 'multiItem', 'idempotency', 'encryptedDelivery', 'categories']) {
        expect(i18n.global.te(`siteConnections.features.${k}`, locale)).toBe(true)
      }
    }
  })

  it('defaults new connections to a protocol offering a connection code', () => {
    expect(defaultProtocolId(REGISTRY)).toBe('zebra-store')
    expect(defaultProtocolId([acme])).toBe('acme-shop')
    expect(defaultProtocolId([])).toBe('')
  })
})

describe('useProtocolRegistry', () => {
  it('loads adapters and resolves unknown ids to the generic URL/Key/Secret fields', async () => {
    getSiteConnectionProtocols.mockResolvedValue({ status_code: 0, msg: '', data: REGISTRY })
    const r = useProtocolRegistry()
    await r.load()
    expect(r.loaded.value).toBe(true)
    expect(r.find('acme-shop')?.fields.map((f) => f.key)).toEqual(['endpoint', 'merchant_id', 'token', 'region'])
    expect(r.defaultId()).toBe('zebra-store')
    const generic = r.resolve('legacy-x')
    expect(generic.id).toBe('legacy-x')
    expect(generic.fields.map((f) => f.key)).toEqual(['base_url', 'api_key', 'api_secret'])
    expect(generic.supports_connection_code).toBe(false)
  })

  it('keeps an error message when the registry cannot be loaded', async () => {
    getSiteConnectionProtocols.mockRejectedValue(new Error('boom'))
    const r = useProtocolRegistry()
    await r.load()
    expect(r.loaded.value).toBe(false)
    expect(r.error.value).toBe('boom')
    expect(r.protocols.value).toEqual([])
  })
})

describe('useSiteConnections with the registry', () => {
  it('renders/validates the fake protocol form and submits its config', async () => {
    getSiteConnectionProtocols.mockResolvedValue({ status_code: 0, msg: '', data: REGISTRY })
    createSiteConnection.mockResolvedValue({ status_code: 0, msg: '', data: {} })
    const p = useSiteConnections()
    await p.registry.load()
    p.openCreate()
    expect(p.form.protocol).toBe('zebra-store')

    p.selectProtocol('acme-shop')
    p.form.name = 'Acme'
    p.setConfigValue('endpoint', 'https://acme.io')
    p.submit()
    expect(createSiteConnection).not.toHaveBeenCalled()
    expect(p.configErrors.value).toEqual({ merchant_id: t('admin.common.required'), token: t('admin.common.required') })

    p.setConfigValue('merchant_id', 'm-9')
    p.setConfigValue('token', 'tok')
    expect(p.configErrors.value.merchant_id).toBe('')
    p.submit()
    await flush()
    expect(createSiteConnection).toHaveBeenCalledTimes(1)
    expect(createSiteConnection.mock.calls[0]?.[0]).toMatchObject({
      name: 'Acme',
      protocol: 'acme-shop',
      config: { endpoint: 'https://acme.io', merchant_id: 'm-9', token: 'tok', region: '' },
    })
  })

  it('switching protocol clears the previous handshake', async () => {
    getSiteConnectionProtocols.mockResolvedValue({ status_code: 0, msg: '', data: REGISTRY })
    handshakeSiteConnection.mockResolvedValue(handshakeOk())
    const p = useSiteConnections()
    await p.registry.load()
    p.openCreate()
    p.form.config = { base_url: 'https://a.io', api_key: 'k', api_secret: 's' }
    await p.wizard.handshake()
    expect(p.wizard.result.value).not.toBeNull()
    p.selectProtocol('dujiao-next')
    expect(p.wizard.result.value).toBeNull()
    expect(p.form.config.base_url).toBe('https://a.io')
  })
})
