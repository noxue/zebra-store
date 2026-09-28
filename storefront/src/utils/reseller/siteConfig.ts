import type { ResellerLocalizedText, ResellerSiteConfigData, ResellerSiteConfigPayload } from '@/api/types'

export type ResellerLocale = 'zh-CN' | 'zh-TW' | 'en-US'
export const resellerLocales: ResellerLocale[] = ['zh-CN', 'zh-TW', 'en-US']

type LooseLocalized = Partial<Record<string, unknown>> | null | undefined

export const blankLocalizedText = (): ResellerLocalizedText => ({ 'zh-CN': '', 'zh-TW': '', 'en-US': '' })

export const hasLocalizedText = (value?: LooseLocalized): boolean =>
  !!value && Object.values(value).some((item) => typeof item === 'string' && item.trim() !== '')

export const normalizeLocalizedTextForForm = (value?: LooseLocalized): ResellerLocalizedText => ({
  'zh-CN': String(value?.['zh-CN'] || ''),
  'zh-TW': String(value?.['zh-TW'] || ''),
  'en-US': String(value?.['en-US'] || ''),
})

export const normalizeLinksForForm = (links?: Array<{ name?: LooseLocalized; url?: string }> | null): Array<{ name: ResellerLocalizedText; url: string }> =>
  Array.isArray(links) ? links.map((item) => ({ name: normalizeLocalizedTextForForm(item.name), url: String(item.url || '') })) : []

export const canEditResellerSiteConfig = (snapshot?: { opened?: boolean; can_edit?: boolean } | null): boolean =>
  snapshot?.opened === true && snapshot?.can_edit === true

export const isResellerSiteSeoConfigured = (
  seo?: { title?: LooseLocalized; keywords?: LooseLocalized; description?: LooseLocalized; default_og_image?: string | null } | null,
): boolean => {
  if (!seo) return false
  return hasLocalizedText(seo.title) || hasLocalizedText(seo.keywords) || hasLocalizedText(seo.description) || String(seo.default_og_image || '').trim() !== ''
}

/** Full form model with every nested object present. */
export interface ResellerSiteConfigForm {
  site_name: string
  logo: string
  favicon: string
  announcement: { enabled: boolean; type: string; title: ResellerLocalizedText; content: ResellerLocalizedText }
  support: { telegram: string; whatsapp: string; email: string; support_url: string }
  seo: { title: ResellerLocalizedText; keywords: ResellerLocalizedText; description: ResellerLocalizedText; default_og_image: string }
  footer_links: Array<{ name: ResellerLocalizedText; url: string }>
  nav_config: { builtin: Record<string, boolean>; custom_items: Array<{ name: ResellerLocalizedText; url: string }> }
}

export const createBlankSiteConfigForm = (): ResellerSiteConfigForm => ({
  site_name: '',
  logo: '',
  favicon: '',
  announcement: { enabled: false, type: 'info', title: blankLocalizedText(), content: blankLocalizedText() },
  support: { telegram: '', whatsapp: '', email: '', support_url: '' },
  seo: { title: blankLocalizedText(), keywords: blankLocalizedText(), description: blankLocalizedText(), default_og_image: '' },
  footer_links: [],
  nav_config: { builtin: { blog: true, notice: true, about: true }, custom_items: [] },
})

export const siteConfigToForm = (config?: ResellerSiteConfigData | null): ResellerSiteConfigForm => {
  const next = createBlankSiteConfigForm()
  if (!config) return next
  next.site_name = config.site_name || ''
  next.logo = config.logo || ''
  next.favicon = config.favicon || ''
  next.announcement = {
    enabled: config.announcement?.enabled === true,
    type: config.announcement?.type || 'info',
    title: normalizeLocalizedTextForForm(config.announcement?.title),
    content: normalizeLocalizedTextForForm(config.announcement?.content),
  }
  next.support = {
    telegram: config.support?.telegram || '',
    whatsapp: config.support?.whatsapp || '',
    email: config.support?.email || '',
    support_url: config.support?.support_url || '',
  }
  next.seo = {
    title: normalizeLocalizedTextForForm(config.seo?.title),
    keywords: normalizeLocalizedTextForForm(config.seo?.keywords),
    description: normalizeLocalizedTextForForm(config.seo?.description),
    default_og_image: config.seo?.default_og_image || '',
  }
  next.footer_links = normalizeLinksForForm(config.footer_links)
  next.nav_config = {
    builtin: { blog: true, notice: true, about: true, ...(config.nav_config?.builtin || {}) },
    custom_items: normalizeLinksForForm(config.nav_config?.custom_items),
  }
  return next
}

export const siteConfigFormToPayload = (form: ResellerSiteConfigForm): ResellerSiteConfigPayload => ({
  site_name: form.site_name.trim(),
  logo: form.logo.trim(),
  favicon: form.favicon.trim(),
  announcement: { ...form.announcement },
  support: { ...form.support },
  seo: { ...form.seo },
  footer_links: form.footer_links.filter((l) => l.url.trim() || hasLocalizedText(l.name)),
  nav_config: {
    builtin: { ...form.nav_config.builtin },
    custom_items: form.nav_config.custom_items.filter((l) => l.url.trim() || hasLocalizedText(l.name)),
  },
})

/** Max footer / custom nav links (same as original UI). */
export const MAX_SITE_LINKS = 10
