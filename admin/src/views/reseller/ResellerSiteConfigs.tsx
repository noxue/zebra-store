import { defineComponent, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { CircleAlert, CircleCheck, Info, Plus, Search, Trash2 } from 'lucide-vue-next'
import {
  Button,
  DataTable,
  Dialog,
  FilterBar,
  FormField,
  IdCell,
  Input,
  ListPagination,
  PageHeader,
  Switch,
  Tabs,
  Textarea,
  cn,
  type DataTableColumn,
} from '@/components/ui'
import { MediaPicker } from '@/components/MediaPicker'
import { RichEditor } from '@/components/RichEditor'
import type { AdminResellerSiteConfig } from '@/api/types'
import { formatDate } from '@/utils/format'
import { getImageUrl } from '@/utils/image'
import { canEditResellerSiteConfig, getLocalizedText } from '@/utils/resellerSiteConfig'
import { LocaleSwitch, RESELLER_LOCALE_LABELS } from './components/LocaleSwitch'
import { RefreshButton } from './components/RefreshButton'
import { NAV_BUILTIN_KEYS, siteConfigUserLabel } from './resellerUtils'
import { useResellerSiteConfigs, type SiteConfigTab } from './useResellerSiteConfigs'

const ANNOUNCEMENT_TYPES = [
  { value: 'info', icon: Info, iconClass: 'text-accent' },
  { value: 'success', icon: CircleCheck, iconClass: 'text-success-text' },
  { value: 'warning', icon: CircleAlert, iconClass: 'text-warning-text' },
] as const

const TABS: SiteConfigTab[] = ['brand', 'announcement', 'support', 'seo', 'footer', 'nav']

export default defineComponent({
  name: 'ResellerSiteConfigsView',
  setup() {
    const { t, locale } = useI18n()
    const s = useResellerSiteConfigs()
    const { filters, list, form } = s
    onMounted(() => void list.fetchData(1))

    const thumb = (src: string, alt: string) =>
      src ? (
        <img src={getImageUrl(src)} class="h-9 w-9 rounded-zs-sm border border-line bg-surface-muted object-contain" alt={alt} />
      ) : (
        <span class="text-xs text-muted">-</span>
      )

    const columns = (): DataTableColumn<AdminResellerSiteConfig>[] => [
      {
        key: 'id',
        title: t('admin.resellerSiteConfigs.table.id'),
        render: (r) => <IdCell value={r.id} />,
      },
      {
        key: 'reseller',
        title: t('admin.resellerSiteConfigs.table.reseller'),
        render: (r) => (
          <div class="min-w-[170px]">
            <div class="font-mono text-sm font-medium">R#{r.reseller_id}</div>
            <div class="break-all text-xs text-muted">{siteConfigUserLabel(r)}</div>
          </div>
        ),
      },
      {
        key: 'siteName',
        title: t('admin.resellerSiteConfigs.table.siteName'),
        class: 'min-w-[150px] text-sm',
        render: (r) => r.site_name || '-',
      },
      {
        key: 'logo',
        title: t('admin.resellerSiteConfigs.table.logo'),
        render: (r) => thumb(r.logo, 'logo'),
      },
      {
        key: 'favicon',
        title: t('admin.resellerSiteConfigs.table.favicon'),
        render: (r) => thumb(r.favicon, 'favicon'),
      },
      {
        key: 'updatedAt',
        title: t('admin.resellerSiteConfigs.table.updatedAt'),
        class: 'text-xs text-muted whitespace-nowrap',
        render: (r) => formatDate(r.updated_at),
      },
      {
        key: 'action',
        title: t('admin.resellerSiteConfigs.table.action'),
        align: 'right',
        render: (r) => {
          const busy = s.operatingId.value === r.reseller_id
          return (
            <div class="flex justify-end gap-2">
              <Button size="xs" disabled={busy || !canEditResellerSiteConfig(r.profile)} onClick={() => s.openEditor(r)}>
                {t('admin.resellerSiteConfigs.actions.edit')}
              </Button>
              <Button size="xs" variant="ghost" class="text-danger-text" disabled={busy} onClick={() => s.reset(r)}>
                {t('admin.resellerSiteConfigs.actions.reset')}
              </Button>
            </div>
          )
        },
      },
    ]

    const languageRow = () => (
      <div class="flex items-center justify-between gap-3">
        <span class="text-xs font-medium text-fg/85">{t('admin.resellerSiteConfigs.editor.language')}</span>
        <LocaleSwitch v-model={s.activeLocale.value} />
      </div>
    )

    const renderTab = () => {
      const loc = s.activeLocale.value
      switch (s.activeTab.value) {
        case 'brand':
          return (
            <div class="space-y-4">
              <FormField label={t('admin.resellerSiteConfigs.fields.siteName')}>
                <Input v-model={form.site_name} />
              </FormField>
              <div class="grid gap-4 sm:grid-cols-2">
                <FormField label={t('admin.resellerSiteConfigs.fields.logo')}>
                  <MediaPicker v-model={form.logo} scene="reseller" />
                </FormField>
                <FormField label={t('admin.resellerSiteConfigs.fields.favicon')}>
                  <MediaPicker v-model={form.favicon} scene="reseller" />
                </FormField>
              </div>
            </div>
          )
        case 'announcement':
          return (
            <div class="space-y-4">
              <div class="flex flex-wrap items-center justify-between gap-3 rounded-zs border border-line bg-surface-muted/60 p-3">
                <Switch v-model={form.announcement.enabled} label={t('admin.resellerSiteConfigs.fields.announcementEnabled')} />
                <div class="flex flex-wrap items-center gap-2">
                  <span class="text-xs text-muted">{t('admin.resellerSiteConfigs.fields.announcementType')}</span>
                  {ANNOUNCEMENT_TYPES.map((opt) => {
                    const Icon = opt.icon
                    const active = form.announcement.type === opt.value
                    return (
                      <button
                        key={opt.value}
                        type="button"
                        class={cn(
                          'inline-flex items-center gap-1.5 rounded-full border px-3 py-1 text-xs font-medium transition-colors',
                          active ? 'border-primary bg-primary-soft text-primary' : 'border-line bg-surface-solid text-muted hover:text-fg',
                        )}
                        onClick={() => (form.announcement.type = opt.value)}
                      >
                        <Icon class={cn('h-3.5 w-3.5', opt.iconClass)} />
                        {t(`admin.resellerSiteConfigs.announcementTypes.${opt.value}`)}
                      </button>
                    )
                  })}
                </div>
              </div>
              {languageRow()}
              <FormField label={t('admin.resellerSiteConfigs.fields.announcementTitle')}>
                <Input v-model={form.announcement.title[loc]} />
              </FormField>
              <FormField label={t('admin.resellerSiteConfigs.fields.announcementContent')}>
                <RichEditor key={`ann-${loc}`} v-model={form.announcement.content[loc]} minHeight="180px" />
              </FormField>
            </div>
          )
        case 'support':
          return (
            <div class="grid gap-4 sm:grid-cols-2">
              <FormField label={t('admin.resellerSiteConfigs.fields.telegram')}>
                <Input v-model={form.support.telegram} />
              </FormField>
              <FormField label={t('admin.resellerSiteConfigs.fields.whatsapp')}>
                <Input v-model={form.support.whatsapp} />
              </FormField>
              <FormField label={t('admin.resellerSiteConfigs.fields.email')}>
                <Input v-model={form.support.email} />
              </FormField>
              <FormField label={t('admin.resellerSiteConfigs.fields.supportUrl')}>
                <Input v-model={form.support.support_url} />
              </FormField>
            </div>
          )
        case 'seo':
          return (
            <div class="space-y-4">
              {languageRow()}
              <FormField label={t('admin.resellerSiteConfigs.fields.seoTitle')}>
                <Input v-model={form.seo.title[loc]} />
              </FormField>
              <FormField label={t('admin.resellerSiteConfigs.fields.seoKeywords')}>
                <Input v-model={form.seo.keywords[loc]} />
              </FormField>
              <FormField label={t('admin.resellerSiteConfigs.fields.seoDescription')}>
                <Textarea v-model={form.seo.description[loc]} rows={3} />
              </FormField>
              <FormField label={t('admin.resellerSiteConfigs.fields.ogImage')}>
                <MediaPicker v-model={form.seo.default_og_image} scene="reseller" />
              </FormField>
            </div>
          )
        case 'footer':
          return (
            <div class="space-y-4">
              <div class="flex flex-wrap items-center justify-between gap-3">
                <LocaleSwitch v-model={s.activeLocale.value} />
                <Button size="sm" variant="soft" onClick={s.addFooterLink}>
                  <Plus class="h-3.5 w-3.5" />
                  {t('admin.resellerSiteConfigs.actions.addLink')}
                </Button>
              </div>
              {form.footer_links.length ? (
                <div class="space-y-3">
                  {form.footer_links.map((link, index) => (
                    <div key={index} class="grid gap-3 rounded-zs border border-line p-3 sm:grid-cols-[1fr_1fr_auto] sm:items-end">
                      <FormField label={`${t('admin.resellerSiteConfigs.fields.linkName')} · ${RESELLER_LOCALE_LABELS[loc]}`}>
                        <Input v-model={link.name[loc]} />
                      </FormField>
                      <FormField label={t('admin.resellerSiteConfigs.fields.linkUrl')}>
                        <Input v-model={link.url} />
                      </FormField>
                      <Button size="sm" variant="ghost" class="text-danger-text" onClick={() => s.removeFooterLink(index)}>
                        <Trash2 class="h-3.5 w-3.5" />
                        {t('admin.resellerSiteConfigs.actions.removeLink')}
                      </Button>
                    </div>
                  ))}
                </div>
              ) : (
                <p class="rounded-zs border border-dashed border-line px-4 py-6 text-center text-sm text-muted">{t('admin.resellerSiteConfigs.emptyLinks')}</p>
              )}
            </div>
          )
        case 'nav':
          return (
            <div class="space-y-3">
              {NAV_BUILTIN_KEYS.map((key) => (
                <div key={key} class="flex items-center justify-between rounded-zs border border-line px-4 py-2.5">
                  <span class="text-sm">{t(`admin.resellerSiteConfigs.navItems.${key}`)}</span>
                  <Switch v-model={form.nav_config.builtin[key]} />
                </div>
              ))}
            </div>
          )
        default:
          return null
      }
    }

    return () => {
      const sel = s.selected.value
      return (
        <div class="space-y-6">
          <PageHeader title={t('admin.resellerSiteConfigs.title')} subtitle={t('admin.resellerSiteConfigs.subtitle')} />
          <FilterBar cols={4}>
            {{
              default: () => (
                <>
                  <Input
                    icon={Search}
                    v-model={filters.keyword}
                    placeholder={t('admin.resellerSiteConfigs.filters.keyword')}
                    onUpdate:modelValue={list.debouncedSearch}
                  />
                  <Input
                    v-model={filters.resellerId}
                    placeholder={t('admin.resellerSiteConfigs.filters.resellerId')}
                    onUpdate:modelValue={list.debouncedSearch}
                  />
                </>
              ),
              actions: () => <RefreshButton loading={s.refreshing.value} onClick={s.refresh} />,
            }}
          </FilterBar>
          <div>
            <DataTable
              columns={columns()}
              rows={list.items.value}
              rowKey={(r) => r.id}
              loading={list.loading.value}
              emptyText={t('admin.resellerSiteConfigs.empty')}
              minWidth="1000px"
            />
            <ListPagination pagination={list.pagination.value} onChangePage={list.changePage} onChangePageSize={list.changePageSize} />
          </div>

          <Dialog
            modelValue={s.showEditor.value}
            onUpdate:modelValue={(v: boolean) => !v && s.closeEditor()}
            title={t('admin.resellerSiteConfigs.editor.title', {
              id: sel?.reseller_id ?? '-',
            })}
            size="2xl"
            closeOnOverlay={false}
          >
            {{
              default: () => (
                <div class="space-y-5">
                  <Tabs
                    v-model={s.activeTab.value}
                    items={TABS.map((key) => ({
                      key,
                      label: t(`admin.resellerSiteConfigs.editor.sections.${key}`),
                    }))}
                  />
                  {renderTab()}
                  {sel && (
                    <div class="rounded-zs-sm bg-surface-muted px-3 py-2 text-xs text-muted">
                      {t('admin.resellerSiteConfigs.previewLabel')}:{' '}
                      {sel.site_name || getLocalizedText(sel.seo?.title, locale.value) || siteConfigUserLabel(sel)}
                    </div>
                  )}
                </div>
              ),
              footer: () => (
                <>
                  <Button disabled={s.saving.value} onClick={s.closeEditor}>
                    {t('admin.common.cancel')}
                  </Button>
                  <Button variant="primary" loading={s.saving.value} onClick={s.save}>
                    {s.saving.value ? t('admin.resellerSiteConfigs.actions.saving') : t('admin.resellerSiteConfigs.actions.save')}
                  </Button>
                </>
              ),
            }}
          </Dialog>
        </div>
      )
    }
  },
})
