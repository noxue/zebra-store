import { computed, defineComponent, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import type { LucideIcon } from 'lucide-vue-next'
import { CircleCheck, Compass, ExternalLink, LifeBuoy, Lock, Megaphone, Plus, RotateCcw, Save, Store, Trash2 } from 'lucide-vue-next'
import { useResellerSiteConfig, type SiteSection } from '@/composables/reseller/useResellerSiteConfig'
import { RichContent } from '@/components/common/RichContent'
import { ResellerAlert, ResellerPageHeader, ResellerPageState } from '@/components/reseller/ConsoleParts'
import { LocalizedInput, ResellerImageField, ResellerLocaleTabs } from '@/components/reseller/ResellerFormParts'
import { Alert, Badge, Button, Card, Field, Input, Select, Switch, cn } from '@/components/ui'

const sectionIcons: Record<SiteSection, LucideIcon> = { brand: Store, support: LifeBuoy, content: Megaphone, navigation: Compass }

export default defineComponent({
  name: 'ResellerSite',
  setup() {
    const { t } = useI18n()
    const s = useResellerSiteConfig()
    onMounted(() => void s.load())
    const k = (key: string) => t(`personalCenter.reseller.siteConfig.${key}`)
    const sections: SiteSection[] = ['brand', 'support', 'content', 'navigation']
    const announcementTypes = computed(() => ['info', 'success', 'warning'].map((v) => ({ value: v, label: k(`announcementTypes.${v}`) })))
    const supportFields = [
      { key: 'telegram' as const, label: 'fields.telegram', placeholder: 'https://telegram.me/example' },
      { key: 'whatsapp' as const, label: 'fields.whatsapp', placeholder: 'https://wa.me/1234567890' },
      { key: 'email' as const, label: 'fields.email', placeholder: 'support@example.com' },
      { key: 'support_url' as const, label: 'fields.supportUrl', placeholder: 'https://example.com/support' },
    ]
    const disabled = computed(() => !s.canEdit.value || s.saving.value)

    const linkEditor = (list: Array<{ name: Record<'zh-CN' | 'zh-TW' | 'en-US', string>; url: string }>, onRemove: (i: number) => void) =>
      list.length === 0 ? (
        <p class="rounded-zs border border-dashed border-line-strong px-4 py-4 text-center text-sm text-muted">{k('emptyLinks')}</p>
      ) : (
        <div class="space-y-3">
          {list.map((link, i) => (
            <div key={i} class="grid gap-2 rounded-zs border border-line bg-surface-strong p-3 md:grid-cols-[1fr_1.4fr_auto]">
              <LocalizedInput v-model={link.name} locale={s.activeLocale.value} placeholder={k('fields.linkName')} disabled={disabled.value} />
              <Input v-model={link.url} placeholder="https://" disabled={disabled.value} />
              <Button variant="ghost" size="icon" disabled={disabled.value} onClick={() => onRemove(i)}>
                <Trash2 class="size-4 text-danger-text" />
              </Button>
            </div>
          ))}
        </div>
      )

    const brandSection = () => (
      <div class="grid gap-5 md:grid-cols-2">
        <div class="md:col-span-2">
          <Field label={k('fields.siteName')}>
            <Input v-model={s.form.site_name} disabled={disabled.value} maxlength={60} />
          </Field>
        </div>
        <Field label={k('fields.logo')} hint={k('imageHint')}>
          <ResellerImageField v-model={s.form.logo} disabled={disabled.value} />
        </Field>
        <Field label={k('fields.favicon')} hint={k('imageHint')}>
          <ResellerImageField v-model={s.form.favicon} disabled={disabled.value} />
        </Field>
      </div>
    )

    const supportSection = () => (
      <div class="grid gap-5 md:grid-cols-2">
        {supportFields.map((f) => (
          <Field key={f.key} label={k(f.label)}>
            <Input v-model={s.form.support[f.key]} placeholder={f.placeholder} disabled={disabled.value} />
          </Field>
        ))}
      </div>
    )

    const contentSection = () => (
      <div class="space-y-6">
        <div class="flex flex-wrap items-center justify-between gap-3">
          <label class="inline-flex items-center gap-3 text-sm font-bold text-fg">
            <Switch v-model={s.form.announcement.enabled} disabled={disabled.value} label={k('fields.announcementEnabled')} />
            {k('fields.announcementEnabled')}
            <Badge tone={s.form.announcement.enabled ? 'success' : 'neutral'} size="xs">
              {s.form.announcement.enabled ? k('announcementPanel.enabledStatus') : k('announcementPanel.disabledStatus')}
            </Badge>
          </label>
          <ResellerLocaleTabs v-model={s.activeLocale.value} />
        </div>
        <div class="grid gap-5 lg:grid-cols-2">
          <div class="space-y-4">
            <Field label={k('fields.announcementType')}>
              <Select v-model={s.form.announcement.type} options={announcementTypes.value} disabled={disabled.value} />
            </Field>
            <Field label={k('fields.announcementTitle')}>
              <LocalizedInput v-model={s.form.announcement.title} locale={s.activeLocale.value} disabled={disabled.value} />
            </Field>
            <Field label={k('fields.announcementContent')} hint={t('zsReseller.htmlHint')}>
              <LocalizedInput v-model={s.form.announcement.content} locale={s.activeLocale.value} disabled={disabled.value} multiline rows={6} placeholder={k('editor.placeholder')} />
            </Field>
          </div>
          <div>
            <div class="mb-2 text-sm font-bold text-fg">{k('announcementPanel.livePreview')}</div>
            <div class="rounded-zs-lg border border-line bg-surface-strong p-5">
              {!s.form.announcement.enabled ? (
                <div class="text-center">
                  <div class="zs-title text-fg">{k('announcementPanel.disabledPreviewTitle')}</div>
                  <p class="mt-1 text-xs text-muted">{k('announcementPanel.disabledPreviewDescription')}</p>
                </div>
              ) : s.form.announcement.title[s.activeLocale.value] || s.form.announcement.content[s.activeLocale.value] ? (
                <Alert tone={s.form.announcement.type === 'success' ? 'success' : s.form.announcement.type === 'warning' ? 'warning' : 'info'} title={s.form.announcement.title[s.activeLocale.value]}>
                  <RichContent html={s.form.announcement.content[s.activeLocale.value]} />
                </Alert>
              ) : (
                <div class="text-center">
                  <div class="zs-title text-fg">{k('announcementPanel.emptyPreviewTitle')}</div>
                  <p class="mt-1 text-xs text-muted">{k('announcementPanel.emptyPreviewDescription')}</p>
                </div>
              )}
              <p class="mt-3 text-xs text-muted">{k('announcementPanel.previewHint')}</p>
            </div>
          </div>
        </div>
        <div class="zs-divider" />
        <div>
          <h3 class="zs-title text-base text-fg">{k('sections.seo')}</h3>
          <p class="mb-4 text-xs text-muted">{k('descriptions.seo')}</p>
          <div class="grid gap-4 md:grid-cols-2">
            <Field label={k('fields.seoTitle')}>
              <LocalizedInput v-model={s.form.seo.title} locale={s.activeLocale.value} disabled={disabled.value} />
            </Field>
            <Field label={k('fields.seoKeywords')}>
              <LocalizedInput v-model={s.form.seo.keywords} locale={s.activeLocale.value} disabled={disabled.value} />
            </Field>
            <Field label={k('fields.seoDescription')}>
              <LocalizedInput v-model={s.form.seo.description} locale={s.activeLocale.value} disabled={disabled.value} multiline rows={3} />
            </Field>
            <Field label={k('fields.ogImage')} hint={k('imageHint')}>
              <ResellerImageField v-model={s.form.seo.default_og_image} disabled={disabled.value} />
            </Field>
          </div>
        </div>
      </div>
    )

    const navigationSection = () => (
      <div class="space-y-6">
        <div class="flex justify-end">
          <ResellerLocaleTabs v-model={s.activeLocale.value} />
        </div>
        <div>
          <div class="mb-3 flex items-center justify-between gap-2">
            <div>
              <h3 class="text-sm font-bold text-fg">{k('fields.footerLinks')}</h3>
              <p class="text-xs text-muted">{k('descriptions.footer')}</p>
            </div>
            <Button size="sm" variant="soft" disabled={disabled.value} onClick={s.addFooterLink}>
              <Plus class="size-4" />
              {k('actions.addLink')}
            </Button>
          </div>
          {linkEditor(s.form.footer_links, s.removeFooterLink)}
        </div>
        <div class="zs-divider" />
        <div>
          <h3 class="text-sm font-bold text-fg">{k('fields.navVisibility')}</h3>
          <p class="mb-3 text-xs text-muted">{k('descriptions.nav')}</p>
          <div class="flex flex-wrap gap-4">
            {['blog', 'notice', 'about'].map((key) => (
              <label key={key} class="inline-flex items-center gap-2 rounded-full border border-line bg-surface-strong px-4 py-2 text-sm font-bold text-fg">
                <Switch v-model={s.form.nav_config.builtin[key]} disabled={disabled.value} label={k(`nav.${key}`)} />
                {k(`nav.${key}`)}
              </label>
            ))}
          </div>
        </div>
        <div>
          <div class="mb-3 flex items-center justify-between gap-2">
            <h3 class="text-sm font-bold text-fg">{t('zsReseller.navCustom')}</h3>
            <Button size="sm" variant="soft" disabled={disabled.value} onClick={s.addCustomNav}>
              <Plus class="size-4" />
              {t('zsReseller.addNav')}
            </Button>
          </div>
          {linkEditor(s.form.nav_config.custom_items, s.removeCustomNav)}
        </div>
      </div>
    )

    const renderSection = () => {
      switch (s.activeSection.value) {
        case 'support':
          return supportSection()
        case 'content':
          return contentSection()
        case 'navigation':
          return navigationSection()
        default:
          return brandSection()
      }
    }

    return () => (
      <div class="space-y-5">
        <ResellerPageHeader title={t('resellerConsole.site.title')} description={t('resellerConsole.site.description')}>
          {{
            actions: () =>
              s.profile.primaryDomainUrl.value ? (
                <Button variant="secondary" size="sm" href={s.profile.primaryDomainUrl.value} target="_blank">
                  <ExternalLink class="size-4" />
                  {t('resellerConsole.site.previewStore')}
                </Button>
              ) : null,
          }}
        </ResellerPageHeader>
        <ResellerAlert alert={s.alert.value} />
        {s.loading.value ? (
          <ResellerPageState loading title={t('resellerConsole.common.loading')} />
        ) : (
          <>
            <div class="grid gap-4 md:grid-cols-3">
              <Card padding="sm">
                <div class="text-xs font-bold text-muted">{t('resellerConsole.site.cards.brand')}</div>
                <div class="zs-title mt-1 truncate text-lg text-fg">{s.config.value?.site_name || t('resellerConsole.site.cards.noSiteName')}</div>
                <p class="mt-1 text-xs text-muted">{t('resellerConsole.site.cards.brandDescription')}</p>
              </Card>
              <Card padding="sm">
                <div class="text-xs font-bold text-muted">{t('resellerConsole.site.readiness.title')}</div>
                <div class="mt-2 flex flex-wrap gap-1.5">
                  {s.readiness.value.map((r) => (
                    <Badge key={r.key} tone={r.done ? 'success' : 'neutral'} size="xs">
                      {r.done && <CircleCheck class="size-3" />}
                      {r.label}
                    </Badge>
                  ))}
                </div>
                <p class="mt-2 text-xs text-muted">{t('resellerConsole.site.readiness.description')}</p>
              </Card>
              <Card padding="sm">
                <div class="text-xs font-bold text-muted">{t('resellerConsole.site.cards.domain')}</div>
                <div class="mt-1 truncate font-mono text-sm font-bold text-fg">{s.profile.primaryDomain.value || t('resellerConsole.site.cards.noPrimaryDomain')}</div>
                <p class="mt-1 text-xs text-muted">{t('resellerConsole.site.cards.domainDescription')}</p>
              </Card>
            </div>

            {!s.canEdit.value && (
              <Alert tone="warning" title={t('zsReseller.readOnly')}>
                <span class="inline-flex items-center gap-1.5">
                  <Lock class="size-3.5" />
                  {k('inactive')}
                </span>
              </Alert>
            )}

            <Card padding="none">
              <div class="zs-scroll-x border-b border-line px-3 pt-3">
                <div class="flex min-w-max gap-1">
                  {sections.map((sec) => {
                    const Icon = sectionIcons[sec]
                    const active = s.activeSection.value === sec
                    return (
                      <button
                        key={sec}
                        type="button"
                        class={cn(
                          'flex items-center gap-2 rounded-t-zs border-b-2 px-4 py-3 text-left transition',
                          active ? 'border-primary bg-primary-soft text-primary-text' : 'border-transparent text-muted hover:text-fg',
                        )}
                        onClick={() => (s.activeSection.value = sec)}
                      >
                        <Icon class="size-4" />
                        <span>
                          <span class="block text-sm font-bold">{k(`tabs.${sec}`)}</span>
                          <span class="hidden text-[11px] sm:block">{k(`tabs.${sec}Description`)}</span>
                        </span>
                      </button>
                    )
                  })}
                </div>
              </div>
              <div class="p-5 sm:p-6">{renderSection()}</div>
              <div class="flex flex-wrap items-center justify-end gap-3 border-t border-line bg-surface-muted px-5 py-4">
                {s.dirty.value && <span class="mr-auto text-xs font-bold text-warning-text">{k('unsavedHint')}</span>}
                <Button variant="secondary" disabled={!s.dirty.value || disabled.value} onClick={s.reset}>
                  <RotateCcw class="size-4" />
                  {t('zsReseller.unsavedReset')}
                </Button>
                <Button loading={s.saving.value} disabled={!s.canEdit.value} onClick={() => void s.save()}>
                  <Save class="size-4" />
                  {s.saving.value ? k('saving') : k('save')}
                </Button>
              </div>
            </Card>
          </>
        )}
      </div>
    )
  },
})
