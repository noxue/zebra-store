import { defineComponent, ref, type PropType } from 'vue'
import { useI18n } from 'vue-i18n'
import { Plus, Trash2 } from 'lucide-vue-next'
import { Button, Card, FormField, Input, Select, Switch, Textarea } from '@/components/ui'
import { MediaPicker } from '@/components/MediaPicker'
import { getImageUrl } from '@/utils/image'
import { buildCurrencyOptions, type LangCode } from './settingsUtils'
import type { SiteSettingsModel } from './useSiteSettings'
import { EmptyHint, LangTag, ToggleRow } from './SettingsUi'

/** Brand image slot: thumbnail + select + delete, with a dialog-only MediaPicker (icon and logo are independent). */
const BrandImageField = defineComponent({
  name: 'BrandImageField',
  props: {
    modelValue: { type: String, default: '' },
    placeholder: { type: String, required: true },
    selectText: { type: String, required: true },
    tip: { type: String, required: true },
    testId: String,
  },
  emits: { 'update:modelValue': (_v: string) => true },
  setup(props, { emit }) {
    const { t } = useI18n()
    const open = ref(false)
    return () => (
      <div class="space-y-1.5" data-testid={props.testId}>
        <div class="flex items-center gap-3">
          <button
            type="button"
            title={props.tip}
            class="flex h-10 w-10 shrink-0 items-center justify-center overflow-hidden rounded-zs-sm border border-line-strong bg-surface-muted transition-colors hover:border-primary"
            onClick={() => (open.value = true)}
          >
            {props.modelValue ? (
              <img src={getImageUrl(props.modelValue)} alt="" class="h-full w-full object-contain" />
            ) : (
              <span class="text-[10px] font-semibold text-muted">{props.placeholder}</span>
            )}
          </button>
          <Button size="sm" data-action="select" onClick={() => (open.value = true)}>
            {props.selectText}
          </Button>
          {props.modelValue && (
            <Button size="sm" variant="ghost" onClick={() => emit('update:modelValue', '')}>
              {t('admin.common.delete')}
            </Button>
          )}
        </div>
        <p class="text-xs text-muted">{props.tip}</p>
        <MediaPicker
          dialogOnly
          scene="common"
          modelValue={props.modelValue}
          open={open.value}
          onUpdate:open={(v: boolean) => (open.value = v)}
          onUpdate:modelValue={(v: string | string[]) => emit('update:modelValue', Array.isArray(v) ? (v[0] ?? '') : v)}
        />
      </div>
    )
  },
})

export default defineComponent({
  name: 'SettingsBasicTab',
  props: {
    model: { type: Object as PropType<SiteSettingsModel>, required: true },
    lang: { type: String as PropType<LangCode>, required: true },
    langName: { type: String, required: true },
  },
  setup(props) {
    const { t } = useI18n()
    const currencyOptions = buildCurrencyOptions().map((c) => ({ label: c, value: c }))

    return () => {
      const m = props.model
      const f = m.form
      const reg = m.registration
      const lang = props.lang
      return (
        <div class="space-y-6">
          <Card title={t('admin.settings.registration.title')} description={t('admin.settings.registration.subtitle')}>
            <div class="space-y-3">
              <ToggleRow
                v-model={reg.registration_enabled}
                label={t('admin.settings.registration.registrationEnabled')}
                description={t('admin.settings.registration.registrationEnabledDesc')}
              />
              <ToggleRow
                v-model={reg.email_verification_enabled}
                label={t('admin.settings.registration.emailVerificationEnabled')}
                description={t('admin.settings.registration.emailVerificationEnabledDesc')}
              />
              <ToggleRow
                v-model={reg.email_domain_allowlist_enabled}
                label={t('admin.settings.registration.emailDomainAllowlistEnabled')}
                description={t('admin.settings.registration.emailDomainAllowlistEnabledDesc')}
              />
              {reg.email_domain_allowlist_enabled && (
                <div class="rounded-zs border border-line bg-surface-muted/60 px-4 py-3">
                  <FormField label={t('admin.settings.registration.allowedEmailDomains')} hint={t('admin.settings.registration.allowedEmailDomainsDesc')}>
                    <Textarea v-model={reg.allowed_email_domains_text} rows={4} placeholder={t('admin.settings.registration.allowedEmailDomainsPlaceholder')} />
                  </FormField>
                </div>
              )}
            </div>
          </Card>

          <Card title={t('admin.settings.order.title')} description={t('admin.settings.order.subtitle')}>
            <div class="grid grid-cols-1 gap-5 md:grid-cols-2">
              <FormField label={t('admin.settings.order.paymentExpireMinutes')} hint={t('admin.settings.order.paymentExpireMinutesTip')}>
                <Input type="number" min={1} max={10080} v-model={m.order.payment_expire_minutes} placeholder={t('admin.settings.order.paymentExpireMinutesPlaceholder')} />
              </FormField>
              <FormField label={t('admin.settings.order.maxRefundDays')} hint={t('admin.settings.order.maxRefundDaysTip')}>
                <Input type="number" min={0} max={3650} v-model={m.order.max_refund_days} placeholder={t('admin.settings.order.maxRefundDaysPlaceholder')} />
              </FormField>
            </div>
          </Card>

          <Card title={t('admin.settings.brand.title')} description={t('admin.settings.brand.subtitle')}>
            <div class="grid grid-cols-1 gap-5 md:grid-cols-2">
              <FormField label={t('admin.settings.brand.siteName')}>
                <Input v-model={f.brand.site_name} name="site_name" placeholder={t('admin.settings.brand.siteNamePlaceholder')} />
              </FormField>
              <FormField label={t('admin.settings.brand.currency')} hint={t('admin.settings.brand.currencyTip')}>
                <Select v-model={f.currency} options={currencyOptions} />
              </FormField>
              <FormField label={t('admin.settings.brand.siteUrl')}>
                <Input v-model={f.brand.site_url} placeholder={t('admin.settings.brand.siteUrlPlaceholder')} />
              </FormField>
              <FormField label={t('admin.settings.brand.siteIcon')}>
                <BrandImageField
                  v-model={f.brand.site_icon}
                  testId="site-icon"
                  placeholder="ICO"
                  selectText={t('admin.settings.brand.siteIconSelect')}
                  tip={t('admin.settings.brand.siteIconTip')}
                />
              </FormField>
              {/* Logo 使用独立的 picker 和清除动作，避免误操作影响浏览器 favicon。 */}
              <FormField label={t('admin.settings.brand.siteLogo')}>
                <BrandImageField
                  v-model={f.brand.site_logo}
                  testId="site-logo"
                  placeholder="LOGO"
                  selectText={t('admin.settings.brand.siteLogoSelect')}
                  tip={t('admin.settings.brand.siteLogoTip')}
                />
              </FormField>
              <div class="md:col-span-2">
                <FormField hint={t('admin.settings.brand.siteDescriptionTip')}>
                  {{
                    label: () => (
                      <span class="flex w-full items-center justify-between">
                        {t('admin.settings.brand.siteDescription')}
                        <LangTag lang={lang} />
                      </span>
                    ),
                    default: () => <Input v-model={f.brand.site_description[lang]} placeholder={t('admin.settings.brand.siteDescriptionPlaceholder')} />,
                  }}
                </FormField>
              </div>
            </div>
          </Card>

          <Card title={t('admin.settings.seo.title')} description={t('admin.settings.seo.subtitle', { lang: props.langName })}>
            {{
              extra: () => <LangTag lang={lang} />,
              default: () => (
                <div class="space-y-5">
                  <FormField label={t('admin.settings.seo.siteTitle')}>
                    <Input v-model={f.seo.title[lang]} placeholder={t('admin.settings.seo.siteTitlePlaceholder')} />
                  </FormField>
                  <FormField label={t('admin.settings.seo.keywords')}>
                    <Input v-model={f.seo.keywords[lang]} placeholder={t('admin.settings.seo.keywordsPlaceholder')} />
                  </FormField>
                  <FormField label={t('admin.settings.seo.description')}>
                    <Textarea v-model={f.seo.description[lang]} rows={3} placeholder={t('admin.settings.seo.descriptionPlaceholder')} />
                  </FormField>
                </div>
              ),
            }}
          </Card>

          <Card title={t('admin.settings.contact.title')} description={t('admin.settings.contact.subtitle')}>
            <div class="grid grid-cols-1 gap-5 md:grid-cols-2">
              <FormField label={t('admin.settings.contact.telegram')}>
                <Input v-model={f.contact.telegram} placeholder={t('admin.settings.contact.telegramPlaceholder')} />
              </FormField>
              <FormField label={t('admin.settings.contact.whatsapp')}>
                <Input v-model={f.contact.whatsapp} placeholder={t('admin.settings.contact.whatsappPlaceholder')} />
              </FormField>
            </div>
          </Card>

          <Card title={t('admin.settings.footerLinks.title')} description={t('admin.settings.footerLinks.subtitle')}>
            {{
              extra: () => (
                <Button size="sm" onClick={m.addFooterLinkItem}>
                  <Plus class="h-3.5 w-3.5" />
                  {t('admin.settings.footerLinks.add')}
                </Button>
              ),
              default: () => (
                <div class="space-y-3">
                  {f.footer_links.length === 0 && <EmptyHint text={t('admin.settings.footerLinks.empty')} />}
                  {f.footer_links.map((link, index) => (
                    <div key={`footer-link-${index}`} class="flex flex-col gap-3 sm:flex-row sm:items-center">
                      <div class="grid flex-1 grid-cols-1 gap-3 md:grid-cols-2">
                        <Input v-model={link.name} placeholder={t('admin.settings.footerLinks.namePlaceholder')} />
                        <Input v-model={link.url} placeholder={t('admin.settings.footerLinks.urlPlaceholder')} />
                      </div>
                      <Button size="sm" variant="danger" onClick={() => m.removeFooterLinkItem(index)}>
                        <Trash2 class="h-3.5 w-3.5" />
                        {t('admin.common.delete')}
                      </Button>
                    </div>
                  ))}
                </div>
              ),
            }}
          </Card>

          <Card title={t('admin.settings.scripts.title')} description={t('admin.settings.scripts.subtitle')}>
            {{
              extra: () => (
                <Button size="sm" onClick={m.addSiteScriptItem}>
                  <Plus class="h-3.5 w-3.5" />
                  {t('admin.settings.scripts.addScript')}
                </Button>
              ),
              default: () => (
                <div class="space-y-4">
                  <p class="rounded-zs-sm border border-line bg-surface-muted/60 px-3 py-2 text-xs text-muted">{t('admin.settings.scripts.injectTip')}</p>
                  {f.scripts.length === 0 && <EmptyHint text={t('admin.settings.scripts.empty')} />}
                  {f.scripts.map((script, index) => (
                    <div key={`site-script-${index}`} class="space-y-4 rounded-zs border border-line bg-surface-muted/40 p-4">
                      <div class="flex items-center justify-between gap-3">
                        <h4 class="text-sm font-semibold text-fg">{t('admin.settings.scripts.scriptItem', { index: index + 1 })}</h4>
                        <Button size="sm" variant="danger" onClick={() => m.removeSiteScriptItem(index)}>
                          {t('admin.common.delete')}
                        </Button>
                      </div>
                      <div class="grid grid-cols-1 gap-4 md:grid-cols-3">
                        <div class="md:col-span-2">
                          <FormField label={t('admin.settings.scripts.name')}>
                            <Input v-model={script.name} placeholder={t('admin.settings.scripts.namePlaceholder')} />
                          </FormField>
                        </div>
                        <FormField label={t('admin.settings.scripts.position')}>
                          <Select
                            v-model={script.position}
                            options={[
                              { label: t('admin.settings.scripts.positionHead'), value: 'head' },
                              { label: t('admin.settings.scripts.positionBodyEnd'), value: 'body_end' },
                            ]}
                          />
                        </FormField>
                      </div>
                      <Switch v-model={script.enabled} label={t('admin.settings.scripts.enabled')} />
                      <FormField label={t('admin.settings.scripts.code')}>
                        <Textarea v-model={script.code} rows={7} mono placeholder={t('admin.settings.scripts.codePlaceholder')} />
                      </FormField>
                    </div>
                  ))}
                </div>
              ),
            }}
          </Card>
        </div>
      )
    }
  },
})
