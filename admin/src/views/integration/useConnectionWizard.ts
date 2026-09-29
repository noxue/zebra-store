import { ref } from 'vue'
import i18n from '@/i18n'
import { adminAPI } from '@/api/admin'
import { errorMessage } from '@/api/client'
import type { SiteConnectionHandshakeResult, SiteConnectionProtocolDef } from '@/api/types'
import { getLocalizedText } from '@/utils/format'
import { missingRequiredFields, normalizeConfig, normalizeConnectionCode, parseSuggestedRate, pickProtocolConfig, type SiteConnectionForm } from './integrationUtils'

export interface WizardError {
  /** Short headline (e.g. "连接码无效"). */
  title: string
  /** Backend detail or a hint on what to do next. */
  detail: string
  /** Extra guidance shown under a backend detail (absent when `detail` already is the hint). */
  hint?: string
}

const withHint = (title: string, detail: string, hint: string): WizardError => (detail && detail !== hint ? { title, detail, hint } : { title, detail: hint })

const handshakeHint = (detail: string): string =>
  /status 30[12378]\b|api_redirected/i.test(detail)
    ? i18n.global.t('siteConnections.wizard.redirectedHint')
    : i18n.global.t('siteConnections.wizard.handshakeFailedHint')

/**
 * 新建/编辑连接时的「连接码 + 握手」逻辑：粘贴连接码 → 后端解析并填表 → 自动握手；
 * 或用当前表单值「测试连接」。与具体协议无关：必填字段来自协议注册表的定义。
 */
export function useConnectionWizard(form: SiteConnectionForm, resolveProtocol: (id: string) => SiteConnectionProtocolDef) {
  const t = i18n.global.t
  const code = ref('')
  const parsing = ref(false)
  const testing = ref(false)
  const result = ref<SiteConnectionHandshakeResult | null>(null)
  const error = ref<WizardError | null>(null)
  let lastParsed = ''
  // Guards against a slow response overwriting a newer request (or a dialog that was reset).
  let seq = 0

  /** Forget the handshake outcome (e.g. after switching protocol) but keep the pasted code. */
  const clearResult = () => {
    seq++
    testing.value = false
    parsing.value = false
    result.value = null
    error.value = null
  }

  const reset = () => {
    clearResult()
    code.value = ''
    lastParsed = ''
  }

  const handshake = async (): Promise<boolean> => {
    const def = resolveProtocol(form.protocol)
    const missing = missingRequiredFields(def, form.config)
    if (missing.length > 0) {
      seq++
      testing.value = false
      result.value = null
      error.value = {
        title: t('siteConnections.wizard.missingTitle'),
        detail: t('siteConnections.wizard.missingDetail', { fields: missing.map((f) => getLocalizedText(f.label) || f.key).join(t('siteConnections.wizard.fieldSep')) }),
      }
      return false
    }
    const mine = ++seq
    testing.value = true
    error.value = null
    result.value = null
    const config = pickProtocolConfig(form.config, def)
    try {
      const res = await adminAPI.handshakeSiteConnection({
        base_url: config.base_url ?? '',
        api_key: config.api_key ?? '',
        api_secret: config.api_secret ?? '',
        protocol: form.protocol,
        config,
      })
      if (mine !== seq) return false
      const data = res.data
      if (!data || !data.ok) {
        const detail = data?.error ?? ''
        error.value = withHint(t('siteConnections.wizard.handshakeFailed'), detail, handshakeHint(detail))
        return false
      }
      result.value = data
      if (!form.name.trim() && data.site?.name) form.name = data.site.name
      return true
    } catch (err) {
      if (mine !== seq) return false
      const detail = errorMessage(err)
      error.value = withHint(t('siteConnections.wizard.handshakeFailed'), detail, handshakeHint(detail))
      return false
    } finally {
      if (mine === seq) testing.value = false
    }
  }

  /** Decode the pasted code, fill the form and immediately handshake. No-op for an unchanged code. */
  const parseCode = async (): Promise<boolean> => {
    const raw = normalizeConnectionCode(code.value)
    if (!raw || raw === lastParsed) return false
    const mine = ++seq
    parsing.value = true
    error.value = null
    result.value = null
    try {
      const data = (await adminAPI.parseSiteConnectionCode(raw)).data
      if (mine !== seq) return false
      lastParsed = raw
      if (data.protocol) form.protocol = data.protocol
      if (data.name) form.name = data.name
      form.config = {
        ...form.config,
        ...normalizeConfig(data.config),
        base_url: data.base_url ?? '',
        api_key: data.api_key ?? '',
        api_secret: data.api_secret ?? '',
      }
    } catch (err) {
      if (mine !== seq) return false
      error.value = withHint(t('siteConnections.wizard.invalidCode'), errorMessage(err), t('siteConnections.wizard.invalidCodeHint'))
      return false
    } finally {
      if (mine === seq) parsing.value = false
    }
    return handshake()
  }

  const onCodePaste = (e: ClipboardEvent) => {
    const text = e.clipboardData?.getData('text')
    if (!text) return
    e.preventDefault()
    code.value = normalizeConnectionCode(text)
    void parseCode()
  }

  const suggestedRate = () => parseSuggestedRate(result.value?.suggested_exchange_rate)

  const applyExchangeRate = () => {
    const rate = suggestedRate()
    if (rate !== null) form.exchange_rate = rate
  }

  const applyCallbackUrl = () => {
    const url = result.value?.suggested_callback_url
    if (url) form.callback_url = url
  }

  return { code, parsing, testing, result, error, reset, clearResult, parseCode, onCodePaste, handshake, suggestedRate, applyExchangeRate, applyCallbackUrl }
}

export type ConnectionWizard = ReturnType<typeof useConnectionWizard>
