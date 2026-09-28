import type { BadgeTone } from '@/components/ui'
import type {
  AdminResellerProductSettingDetail,
  AdminResellerProductSettingPayloadItem,
  AdminResellerProductSettingRule,
  AdminResellerProductSettingSKU,
  AdminResellerProfileRef,
  AdminResellerSiteConfig,
  AdminResellerSiteConfigPayload,
} from '@/api/types'
import { blankLocalizedText, normalizeFooterLinksForForm, normalizeLocalizedTextForForm, type ResellerLocalizedText } from '@/utils/resellerSiteConfig'

// ---- constants (ported from the original constants/reseller.ts) ----
export const RESELLER_PROFILE_STATUSES = ['pending_review', 'active', 'rejected', 'disabled'] as const
export const RESELLER_SETTLEMENT_STATUSES = ['normal', 'frozen'] as const
export const RESELLER_DOMAIN_TYPES = ['subdomain', 'custom'] as const
export const RESELLER_DOMAIN_STATUSES = ['pending_review', 'active', 'disabled'] as const
export const RESELLER_DOMAIN_VERIFICATIONS = ['pending', 'verified', 'failed'] as const
export const RESELLER_LEDGER_TYPES = ['order_profit', 'refund_deduct', 'manual_adjust', 'withdraw_lock', 'withdraw_paid'] as const
export const RESELLER_LEDGER_STATUSES = ['pending_confirm', 'available', 'locked', 'withdrawn', 'canceled'] as const
export const RESELLER_WITHDRAW_STATUSES = ['pending', 'rejected', 'paid'] as const
export const RESELLER_BALANCE_STATUSES = ['normal', 'negative_balance', 'frozen_review', 'disabled'] as const
export const RESELLER_PRICING_MODES = ['inherit', 'markup_percent', 'fixed_markup', 'fixed_price'] as const

// ---- tones (original used amber/emerald/rose/zinc/sky chips) ----
const toneOf = (map: Record<string, BadgeTone>, value?: string): BadgeTone => (value && map[value]) || 'neutral'

export const profileStatusTone = (s?: string) =>
  toneOf(
    {
      pending_review: 'warning',
      active: 'success',
      rejected: 'danger',
      disabled: 'neutral',
    },
    s,
  )
export const settlementTone = (s?: string) => toneOf({ normal: 'success', frozen: 'warning' }, s)
export const domainStatusTone = (s?: string) => toneOf({ pending_review: 'warning', active: 'success', disabled: 'neutral' }, s)
export const verificationTone = (s?: string) => toneOf({ pending: 'warning', verified: 'success', failed: 'danger' }, s)
export const ledgerStatusTone = (s?: string) =>
  toneOf(
    {
      pending_confirm: 'warning',
      available: 'success',
      locked: 'info',
      withdrawn: 'neutral',
      canceled: 'neutral',
    },
    s,
  )
export const withdrawStatusTone = (s?: string) => toneOf({ pending: 'warning', rejected: 'neutral', paid: 'success' }, s)
export const balanceStatusTone = (s?: string) =>
  toneOf(
    {
      normal: 'success',
      negative_balance: 'danger',
      frozen_review: 'warning',
      disabled: 'neutral',
    },
    s,
  )

/** Alert box classes for the operations dashboard (token based replacement of buildResellerOperationsAlertClass). */
export const operationsAlertClass = (level?: string) => {
  if (level === 'warning') return 'border-warning/40 bg-warning-soft text-warning-text'
  if (level === 'danger') return 'border-danger/35 bg-danger-soft text-danger-text'
  if (level === 'info') return 'border-accent/40 bg-accent-soft text-info-text'
  return 'border-line bg-surface-muted text-muted'
}

// ---- generic helpers ----
export const queryString = (value: unknown): string => {
  const v = Array.isArray(value) ? value[0] : value
  return typeof v === 'string' ? v.trim() : v === null || v === undefined ? '' : String(v).trim()
}

export const resolveResellerUserId = (row: { profile?: AdminResellerProfileRef }) => Number(row.profile?.user_id || row.profile?.user?.id || 0)

export const amountOrZero = (value?: string | number | null) => {
  const s = String(value ?? '').trim()
  return s || '0.00'
}

/** Positive negative_amount means the account owes money (original isNegative). */
export const isNegativeBalance = (value?: number | string) => Number(value || 0) > 0

export const processorName = (row: { processor?: { username?: string } | string; processed_by?: number }) => {
  if (row.processor && typeof row.processor === 'object') return row.processor.username || (row.processed_by ? String(row.processed_by) : '-')
  return (typeof row.processor === 'string' && row.processor) || (row.processed_by ? String(row.processed_by) : '-')
}

/** Markup validation shared by approve / edit dialogs. Returns an i18n key on error, '' when valid. */
export const validateMarkupRange = (defaultMarkup: string, maxMarkup: string): string => {
  const d = Number(defaultMarkup.trim() || '0')
  const m = Number(maxMarkup.trim() || '0')
  if (!Number.isFinite(d) || !Number.isFinite(m) || d < 0 || m < 0) return 'admin.resellerProfiles.actions.markupInvalid'
  if (m > 0 && d > m) return 'admin.resellerProfiles.actions.markupRangeInvalid'
  return ''
}

// ---- product settings ----
export const pricingModeKey = (mode?: string) => {
  if (mode === 'inherit') return 'inherit'
  if (mode === 'markup_percent') return 'markupPercent'
  if (mode === 'fixed_markup') return 'fixedMarkup'
  if (mode === 'fixed_price') return 'fixedPrice'
  return 'unknown'
}

export const pricingValue = (row: {
  pricing_mode: string
  markup_percent: string | number
  fixed_markup_amount: string | number
  fixed_price_amount: string | number
}) => {
  if (row.pricing_mode === 'markup_percent') return `${row.markup_percent}%`
  if (row.pricing_mode === 'fixed_markup') return `+${row.fixed_markup_amount}`
  if (row.pricing_mode === 'fixed_price') return String(row.fixed_price_amount)
  return '-'
}

export const skuLabel = (sku: Pick<AdminResellerProductSettingSKU, 'id' | 'sku_code' | 'spec_values'>) => {
  const specs = Object.values(sku.spec_values || {})
    .map((v) => String(v).trim())
    .filter(Boolean)
    .join(' / ')
  return specs || sku.sku_code || `#${sku.id}`
}

export const blankSettingForm = (skuId: number): AdminResellerProductSettingPayloadItem => ({
  sku_id: skuId,
  is_listed: true,
  pricing_mode: 'inherit',
  markup_percent: '0.00',
  fixed_markup_amount: '0.00',
  fixed_price_amount: '0.00',
  sort_order: 0,
})

export const settingFormFromRule = (rule: AdminResellerProductSettingRule | undefined, skuId: number): AdminResellerProductSettingPayloadItem => ({
  sku_id: skuId,
  is_listed: rule?.is_listed !== false,
  pricing_mode: String(rule?.pricing_mode || 'inherit'),
  markup_percent: amountOrZero(rule?.markup_percent),
  fixed_markup_amount: amountOrZero(rule?.fixed_markup_amount),
  fixed_price_amount: amountOrZero(rule?.fixed_price_amount),
  sort_order: Number(rule?.sort_order || 0),
})

export interface PreviewEntry {
  effective: string
  valid: boolean
  errorCode: string
}

/** Seed preview with saved effective prices (key 0 = product-level rule, else sku_id). */
export const seedPreviewEntries = (detail: AdminResellerProductSettingDetail): Record<number, PreviewEntry> => {
  const out: Record<number, PreviewEntry> = {}
  const productEffective = String(detail.product_setting?.effective_price_amount ?? '').trim()
  if (productEffective) out[0] = { effective: productEffective, valid: true, errorCode: '' }
  for (const sku of detail.skus || []) {
    const effective = String(sku.effective_price_amount ?? sku.setting?.effective_price_amount ?? '').trim()
    if (effective) out[sku.id] = { effective, valid: true, errorCode: '' }
  }
  return out
}

// ---- site config ----
export const NAV_BUILTIN_KEYS = ['blog', 'notice', 'about'] as const

export interface ResellerSiteConfigForm {
  site_name: string
  logo: string
  favicon: string
  announcement: {
    enabled: boolean
    type: string
    title: ResellerLocalizedText
    content: ResellerLocalizedText
  }
  support: {
    telegram: string
    whatsapp: string
    email: string
    support_url: string
  }
  seo: {
    title: ResellerLocalizedText
    keywords: ResellerLocalizedText
    description: ResellerLocalizedText
    default_og_image: string
  }
  footer_links: Array<{ name: ResellerLocalizedText; url: string }>
  nav_config: { builtin: Record<string, boolean> }
}

export const createBlankSiteConfigForm = (): ResellerSiteConfigForm => ({
  site_name: '',
  logo: '',
  favicon: '',
  announcement: {
    enabled: false,
    type: 'info',
    title: blankLocalizedText(),
    content: blankLocalizedText(),
  },
  support: { telegram: '', whatsapp: '', email: '', support_url: '' },
  seo: {
    title: blankLocalizedText(),
    keywords: blankLocalizedText(),
    description: blankLocalizedText(),
    default_og_image: '',
  },
  footer_links: [],
  nav_config: { builtin: { blog: true, notice: true, about: true } },
})

export const siteConfigToForm = (row: AdminResellerSiteConfig): ResellerSiteConfigForm => {
  const announcement = row.announcement || {}
  const support = row.support || {}
  const seo = row.seo || {}
  const builtin = row.nav_config?.builtin || {}
  return {
    site_name: row.site_name || '',
    logo: row.logo || '',
    favicon: row.favicon || '',
    announcement: {
      enabled: announcement.enabled === true,
      type: String(announcement.type || 'info'),
      title: normalizeLocalizedTextForForm(announcement.title),
      content: normalizeLocalizedTextForForm(announcement.content),
    },
    support: {
      telegram: String(support.telegram || ''),
      whatsapp: String(support.whatsapp || ''),
      email: String(support.email || ''),
      support_url: String(support.support_url || ''),
    },
    seo: {
      title: normalizeLocalizedTextForForm(seo.title),
      keywords: normalizeLocalizedTextForForm(seo.keywords),
      description: normalizeLocalizedTextForForm(seo.description),
      default_og_image: String(seo.default_og_image || ''),
    },
    footer_links: normalizeFooterLinksForForm(row.footer_links),
    nav_config: {
      builtin: {
        blog: builtin.blog !== false,
        notice: builtin.notice !== false,
        about: builtin.about !== false,
      },
    },
  }
}

export const buildSiteConfigPayload = (form: ResellerSiteConfigForm): AdminResellerSiteConfigPayload => ({
  site_name: form.site_name.trim(),
  logo: form.logo.trim(),
  favicon: form.favicon.trim(),
  announcement: {
    enabled: form.announcement.enabled,
    type: form.announcement.type.trim() || 'info',
    title: form.announcement.title,
    content: form.announcement.content,
  },
  support: {
    telegram: form.support.telegram.trim(),
    whatsapp: form.support.whatsapp.trim(),
    email: form.support.email.trim(),
    support_url: form.support.support_url.trim(),
  },
  seo: {
    title: form.seo.title,
    keywords: form.seo.keywords,
    description: form.seo.description,
    default_og_image: form.seo.default_og_image.trim(),
  },
  footer_links: form.footer_links
    .map((item) => ({ name: item.name, url: item.url.trim() }))
    .filter((item) => item.url || Object.values(item.name).some((v) => v.trim())),
  nav_config: {
    builtin: {
      blog: form.nav_config.builtin.blog !== false,
      notice: form.nav_config.builtin.notice !== false,
      about: form.nav_config.builtin.about !== false,
    },
    custom_items: [],
  },
})

export const siteConfigUserLabel = (row: Pick<AdminResellerSiteConfig, 'profile' | 'reseller_id'>) => {
  const user = row.profile?.user
  return user?.email || user?.display_name || `#${row.profile?.user_id || row.reseller_id}`
}
