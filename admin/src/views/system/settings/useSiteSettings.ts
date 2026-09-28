import { reactive, ref, shallowRef } from 'vue'
import { adminAPI } from '@/api/admin'
import type { SiteTheme } from '@/api/types'
import { applyThemeOverrides, useAppStore } from '@/stores/app'
import { applySiteIcon } from '@/utils/favicon'
import { notifyError, notifySuccess } from '@/utils/notify'
import { notifyFailure, tr } from './common'
import {
  ABOUT_SERVICES_MAX,
  DEFAULT_THEME_COLORS,
  FOOTER_LINKS_MAX,
  SITE_SCRIPTS_MAX,
  asRecord,
  asString,
  buildThemePayload,
  clampNumber,
  createFooterLinkItem,
  createLocalizedField,
  createSiteScriptItem,
  createThemeForm,
  invalidThemeColors,
  isLocalizedFieldNotEmpty,
  joinAllowedEmailDomains,
  normalizeCurrency,
  normalizeFooterLinks,
  normalizeLocalizedField,
  normalizeSiteScripts,
  normalizeTheme,
  splitAllowedEmailDomains,
  type FooterLinkItem,
  type LocalizedField,
  type SiteScriptItem,
} from './settingsUtils'

/**
 * site_config + registration_config + order_config — shared by the basic / template / about / legal tabs
 * (the original saves all three whenever one of those tabs is active).
 */
export function useSiteSettings() {
  const submitting = ref(false)
  /** Last loaded raw site_config: every key we don't edit is written back untouched. */
  const rawSiteConfig = shallowRef<Record<string, unknown>>({})

  const registration = reactive({
    registration_enabled: true,
    email_verification_enabled: true,
    email_domain_allowlist_enabled: false,
    allowed_email_domains_text: '',
  })
  const order = reactive({ payment_expire_minutes: 15 as number | '', max_refund_days: 30 as number | '' })

  const form = reactive({
    brand: {
      site_name: '',
      site_url: '',
      // site_icon 是浏览器 favicon；site_logo 是前台页面 Logo，二者必须独立保存和清除。
      site_icon: '',
      site_logo: '',
      site_description: createLocalizedField(),
    },
    currency: 'CNY',
    contact: { telegram: '', whatsapp: '' },
    seo: { title: createLocalizedField(), keywords: createLocalizedField(), description: createLocalizedField() },
    about: {
      hero: { title: createLocalizedField(), subtitle: createLocalizedField() },
      introduction: createLocalizedField(),
      services: { title: createLocalizedField(), items: [] as LocalizedField[] },
      contact: { title: createLocalizedField(), text: createLocalizedField() },
    },
    legal: { terms: createLocalizedField(), privacy: createLocalizedField() },
    scripts: [] as SiteScriptItem[],
    footer_links: [] as FooterLinkItem[],
    storefront_template: 'classic' as 'classic' | 'vault',
    template_mode: 'card' as 'card' | 'list',
    theme: createThemeForm(),
  })

  const loadSite = (raw: unknown) => {
    const data = asRecord(raw)
    rawSiteConfig.value = data
    const brand = asRecord(data.brand)
    form.brand.site_name = asString(brand.site_name)
    form.brand.site_url = asString(brand.site_url)
    form.brand.site_icon = asString(brand.site_icon)
    form.brand.site_logo = asString(brand.site_logo)
    form.brand.site_description = normalizeLocalizedField(brand.site_description)
    form.currency = normalizeCurrency(data.currency)
    const contact = asRecord(data.contact)
    form.contact.telegram = asString(contact.telegram)
    form.contact.whatsapp = asString(contact.whatsapp)
    const seo = asRecord(data.seo)
    form.seo.title = normalizeLocalizedField(seo.title)
    form.seo.keywords = normalizeLocalizedField(seo.keywords)
    form.seo.description = normalizeLocalizedField(seo.description)
    const about = asRecord(data.about)
    const hero = asRecord(about.hero)
    form.about.hero.title = normalizeLocalizedField(hero.title)
    form.about.hero.subtitle = normalizeLocalizedField(hero.subtitle)
    form.about.introduction = normalizeLocalizedField(about.introduction)
    const services = asRecord(about.services)
    form.about.services.title = normalizeLocalizedField(services.title)
    form.about.services.items = Array.isArray(services.items)
      ? services.items.map(normalizeLocalizedField).filter(isLocalizedFieldNotEmpty).slice(0, ABOUT_SERVICES_MAX)
      : []
    const aboutContact = asRecord(about.contact)
    form.about.contact.title = normalizeLocalizedField(aboutContact.title)
    form.about.contact.text = normalizeLocalizedField(aboutContact.text)
    const legal = asRecord(data.legal)
    form.legal.terms = normalizeLocalizedField(legal.terms)
    form.legal.privacy = normalizeLocalizedField(legal.privacy)
    form.scripts = normalizeSiteScripts(data.scripts)
    form.footer_links = normalizeFooterLinks(data.footer_links)
    form.template_mode = String(data.template_mode || 'card').trim() === 'list' ? 'list' : 'card'
    form.storefront_template = String(data.storefront_template || 'classic').trim() === 'vault' ? 'vault' : 'classic'
    form.theme = normalizeTheme(data.theme)
  }

  const loadOrder = (raw: unknown) => {
    const data = asRecord(raw)
    order.max_refund_days = clampNumber(data.max_refund_days, 0, 3650, 30)
    order.payment_expire_minutes = clampNumber(data.payment_expire_minutes, 1, 10080, 15)
  }

  const loadRegistration = (raw: unknown) => {
    const data = asRecord(raw)
    registration.registration_enabled = data.registration_enabled !== false
    registration.email_verification_enabled = data.email_verification_enabled !== false
    registration.email_domain_allowlist_enabled = data.email_domain_allowlist_enabled === true
    registration.allowed_email_domains_text = joinAllowedEmailDomains(data.allowed_email_domains)
  }

  // ---- list helpers ----
  const addAboutServiceItem = () => {
    if (form.about.services.items.length >= ABOUT_SERVICES_MAX) {
      notifyError(tr('admin.settings.about.maxServicesHint'))
      return
    }
    form.about.services.items.push(createLocalizedField())
  }
  const removeAboutServiceItem = (i: number) => form.about.services.items.splice(i, 1)
  const addSiteScriptItem = () => {
    if (form.scripts.length >= SITE_SCRIPTS_MAX) {
      notifyError(tr('admin.settings.scripts.maxScriptsHint', { max: SITE_SCRIPTS_MAX }))
      return
    }
    form.scripts.push(createSiteScriptItem())
  }
  const removeSiteScriptItem = (i: number) => form.scripts.splice(i, 1)
  const addFooterLinkItem = () => {
    if (form.footer_links.length >= FOOTER_LINKS_MAX) {
      notifyError(tr('admin.settings.footerLinks.maxHint', { max: FOOTER_LINKS_MAX }))
      return
    }
    form.footer_links.push(createFooterLinkItem())
  }
  const removeFooterLinkItem = (i: number) => form.footer_links.splice(i, 1)
  const resetThemeColors = () => Object.assign(form.theme, DEFAULT_THEME_COLORS)

  // ---- save ----
  const saveRegistration = () =>
    adminAPI.updateSettings({
      key: 'registration_config',
      value: {
        registration_enabled: registration.registration_enabled,
        email_verification_enabled: registration.email_verification_enabled,
        email_domain_allowlist_enabled: registration.email_domain_allowlist_enabled,
        allowed_email_domains: splitAllowedEmailDomains(registration.allowed_email_domains_text),
      },
    })

  const saveOrder = () => {
    const maxRefundDays = clampNumber(order.max_refund_days, 0, 3650, 30)
    const paymentExpire = clampNumber(order.payment_expire_minutes, 1, 10080, 15)
    order.max_refund_days = maxRefundDays
    order.payment_expire_minutes = paymentExpire
    return adminAPI.updateSettings({
      key: 'order_config',
      value: { payment_expire_minutes: paymentExpire, max_refund_days: maxRefundDays },
    })
  }

  const buildSitePayload = (): Record<string, unknown> => {
    const raw = rawSiteConfig.value
    return {
      ...raw,
      brand: { ...asRecord(raw.brand), ...form.brand },
      currency: normalizeCurrency(form.currency),
      contact: { ...asRecord(raw.contact), ...form.contact },
      seo: form.seo,
      about: form.about,
      legal: form.legal,
      scripts: form.scripts,
      footer_links: form.footer_links,
      storefront_template: form.storefront_template,
      template_mode: form.template_mode,
      theme: buildThemePayload(form.theme, raw.theme),
    }
  }

  const saveSite = async () => {
    const value = buildSitePayload()
    await adminAPI.updateSettings({ key: 'site_config', value })
    rawSiteConfig.value = value
    applySiteIcon(form.brand.site_icon)
    await useAppStore().loadConfig(true)
    applyThemeOverrides(value.theme as SiteTheme)
  }

  const save = async () => {
    if (invalidThemeColors(form.theme).length > 0) {
      notifyError(tr('admin.systemSettings.invalidColor'))
      return
    }
    submitting.value = true
    try {
      await saveRegistration()
      await saveOrder()
      await saveSite()
      notifySuccess(tr('admin.settings.alerts.saveSuccess'))
    } catch (err) {
      notifyFailure(err)
    } finally {
      submitting.value = false
    }
  }

  return {
    submitting,
    registration,
    order,
    form,
    loadSite,
    loadOrder,
    loadRegistration,
    addAboutServiceItem,
    removeAboutServiceItem,
    addSiteScriptItem,
    removeSiteScriptItem,
    addFooterLinkItem,
    removeFooterLinkItem,
    resetThemeColors,
    buildSitePayload,
    save,
  }
}

export type SiteSettingsModel = ReturnType<typeof useSiteSettings>
