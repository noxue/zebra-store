import { defineComponent, onMounted, watch } from 'vue'
import { useRoute } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { Plus, Search } from 'lucide-vue-next'
import {
  Badge,
  Button,
  DataTable,
  DateTimeInput,
  Dialog,
  FilterBar,
  FormField,
  IdCell,
  Input,
  ListPagination,
  LocalizedInput,
  PageHeader,
  Select,
  Switch,
  type DataTableColumn,
} from '@/components/ui'
import { MediaPicker } from '@/components/MediaPicker'
import type { AdminBanner } from '@/api/types'
import { getLocalizedText } from '@/utils/format'
import { getImageUrl } from '@/utils/image'
import { stripLangLabel } from './contentUtils'
import { useBanners } from './useBanners'

export default defineComponent({
  name: 'BannersView',
  setup() {
    const { t } = useI18n()
    const route = useRoute()
    const p = useBanners()

    onMounted(() => {
      void p.list.fetchData(1)
      if (route.query.banner_id) void p.openEditById(route.query.banner_id)
    })
    watch(
      () => route.query.banner_id,
      (id) => id && void p.openEditById(id),
    )

    const columns = (): DataTableColumn<AdminBanner>[] => [
      { key: 'id', title: t('admin.banners.table.id'), render: (r) => <IdCell value={r.id} /> },
      {
        key: 'image',
        title: t('admin.banners.table.image'),
        render: (r) => (r.image ? <img src={getImageUrl(r.image)} alt={r.name} class="h-14 w-28 shrink-0 rounded-zs-sm object-cover shadow-zs-sm" /> : <span class="text-xs text-muted">-</span>),
      },
      {
        key: 'name',
        title: t('admin.banners.table.name'),
        class: 'min-w-[140px]',
        render: (r) => (
          <div>
            <div class="break-words font-medium text-fg">{r.name}</div>
            <div class="break-words text-xs text-muted">{getLocalizedText(r.title)}</div>
          </div>
        ),
      },
      { key: 'position', title: t('admin.banners.table.position'), class: 'text-xs text-muted', render: (r) => p.positionLabel(r.position) },
      { key: 'linkType', title: t('admin.banners.table.linkType'), class: 'text-xs text-muted', render: (r) => p.linkTypeLabel(r.link_type) },
      { key: 'sort', title: t('admin.banners.table.sort'), class: 'zs-num text-xs text-muted', render: (r) => r.sort_order || 0 },
      {
        key: 'status',
        title: t('admin.banners.table.status'),
        render: (r) => (
          <Badge tone={r.is_active ? 'success' : 'neutral'} dot>
            {r.is_active ? t('admin.common.enabled') : t('admin.common.disabled')}
          </Badge>
        ),
      },
      {
        key: 'action',
        title: t('admin.banners.table.action'),
        align: 'right',
        render: (r) => (
          <div class="flex flex-wrap justify-end gap-2">
            <Button size="sm" onClick={() => p.openEdit(r)}>
              {t('admin.banners.actions.edit')}
            </Button>
            <Button size="sm" variant="danger" onClick={() => p.remove(r)}>
              {t('admin.banners.actions.delete')}
            </Button>
          </div>
        ),
      },
    ]

    const renderForm = () => {
      const f = p.form
      return (
        <form
          class="grid grid-cols-1 gap-4 md:grid-cols-2 md:gap-5"
          onSubmit={(e: Event) => {
            e.preventDefault()
            p.submit()
          }}
        >
          <FormField label={stripLangLabel(t('admin.banners.form.name'))} required error={p.errors.name}>
            <Input v-model={f.name} placeholder={t('admin.banners.form.namePlaceholder')} />
          </FormField>
          <FormField label={t('admin.banners.form.position')} required error={p.errors.position}>
            <Select v-model={f.position} options={p.positionOptions.value} placeholder={t('admin.banners.form.positionPlaceholder')} />
          </FormField>
          <div class="md:col-span-2">
            <FormField label={stripLangLabel(t('admin.banners.form.title', { lang: '' }))}>
              <LocalizedInput v-model={f.title} placeholder={t('admin.banners.form.titlePlaceholder')} />
            </FormField>
          </div>
          <div class="md:col-span-2">
            <FormField label={stripLangLabel(t('admin.banners.form.subtitle', { lang: '' }))}>
              <LocalizedInput v-model={f.subtitle} placeholder={t('admin.banners.form.subtitlePlaceholder')} />
            </FormField>
          </div>
          <div class="md:col-span-2">
            <FormField label={stripLangLabel(t('admin.banners.form.image'))} required error={p.errors.image}>
              <MediaPicker v-model={f.image} scene="banner" />
            </FormField>
          </div>
          <div class="md:col-span-2">
            <FormField label={t('admin.banners.form.mobileImage')}>
              <div class="space-y-2">
                <MediaPicker v-model={f.mobile_image} scene="banner" />
                <Input v-model={f.mobile_image} placeholder={t('admin.banners.form.mobileImagePlaceholder')} mono />
              </div>
            </FormField>
          </div>
          <FormField label={t('admin.banners.form.linkType')}>
            <Select v-model={f.link_type} options={p.linkTypeOptions.value} placeholder={t('admin.banners.form.linkTypePlaceholder')} />
          </FormField>
          <FormField label={t('admin.banners.form.linkValue')}>
            <Input v-model={f.link_value} disabled={f.link_type === 'none'} placeholder={t('admin.banners.form.linkValuePlaceholder')} />
          </FormField>
          <FormField label={t('admin.banners.form.startAt')}>
            <DateTimeInput v-model={f.start_at} />
          </FormField>
          <FormField label={t('admin.banners.form.endAt')}>
            <DateTimeInput v-model={f.end_at} />
          </FormField>
          <FormField label={t('admin.banners.form.sortOrder')}>
            <Input type="number" v-model={f.sort_order} placeholder="0" />
          </FormField>
          <div class="flex flex-wrap items-center gap-6 md:pt-6">
            <Switch v-model={f.is_active} label={t('admin.banners.form.activeNow')} />
            <Switch v-model={f.open_in_new_tab} label={t('admin.banners.form.openInNewTab')} />
          </div>
          {p.modal.error.value && <p class="text-xs text-danger-text md:col-span-2">{p.modal.error.value}</p>}
        </form>
      )
    }

    return () => (
      <div class="space-y-6">
        <PageHeader title={t('admin.banners.title')}>
          {{
            actions: () => (
              <Button variant="primary" onClick={p.openCreate}>
                <Plus class="h-4 w-4" />
                {t('admin.banners.create')}
              </Button>
            ),
          }}
        </PageHeader>

        <FilterBar cols={3}>
          {{
            default: () => (
              <>
                <Input
                  icon={Search}
                  v-model={p.filters.search}
                  placeholder={t('admin.banners.searchPlaceholder')}
                  onEnter={p.list.handleSearch}
                  onUpdate:modelValue={p.list.debouncedSearch}
                />
                <Select
                  v-model={p.filters.position}
                  options={p.positionOptions.value}
                  placeholder={t('admin.banners.filters.positionPlaceholder')}
                  onChange={p.list.handleSearch}
                />
                <Select
                  v-model={p.filters.isActive}
                  options={p.activeFilterOptions.value}
                  placeholder={t('admin.banners.filters.statusPlaceholder')}
                  onChange={p.list.handleSearch}
                />
              </>
            ),
          }}
        </FilterBar>

        <div>
          <DataTable
            columns={columns()}
            rows={p.list.items.value}
            rowKey={(r) => r.id}
            loading={p.list.loading.value}
            emptyText={t('admin.banners.empty')}
            minWidth="900px"
          />
          <ListPagination pagination={p.list.pagination.value} onChangePage={p.list.changePage} onChangePageSize={p.list.changePageSize} />
        </div>

        <Dialog
          v-model={p.modal.showModal.value}
          title={p.modal.isEditing.value ? t('admin.banners.modal.editTitle') : t('admin.banners.modal.createTitle')}
          size="2xl"
          closeOnOverlay={false}
        >
          {{
            default: renderForm,
            footer: () => (
              <>
                <Button onClick={p.modal.closeModal}>{t('admin.common.cancel')}</Button>
                <Button variant="primary" loading={p.modal.submitting.value} onClick={p.submit}>
                  {p.modal.isEditing.value ? t('admin.banners.actions.saveChanges') : t('admin.banners.actions.createNow')}
                </Button>
              </>
            ),
          }}
        </Dialog>
      </div>
    )
  },
})
