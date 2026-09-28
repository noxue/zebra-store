import { defineComponent, onMounted, watch } from 'vue'
import { useRoute } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { Plus } from 'lucide-vue-next'
import { Badge, Button, DataTable, Dialog, FormField, IdCell, Input, LocalizedInput, PageHeader, Select, type DataTableColumn } from '@/components/ui'
import { MediaPicker } from '@/components/MediaPicker'
import { getLocalizedText } from '@/utils/format'
import { getImageUrl } from '@/utils/image'
import type { AdminCategoryHierarchyItem } from '@/utils/category'
import { useCategories } from './useCategories'

export default defineComponent({
  name: 'CategoriesView',
  setup() {
    const { t } = useI18n()
    const route = useRoute()
    const p = useCategories()

    onMounted(() => {
      void p.fetchCategories()
      if (route.query.category_id) void p.openEditById(route.query.category_id)
    })
    watch(
      () => route.query.category_id,
      (id) => id && void p.openEditById(id),
    )

    const columns = (): DataTableColumn<AdminCategoryHierarchyItem>[] => [
      { key: 'id', title: t('admin.categories.table.id'), render: (r) => <IdCell value={r.category.id} /> },
      {
        key: 'icon',
        title: t('admin.categories.table.icon'),
        render: (r) =>
          r.category.icon ? <img src={getImageUrl(r.category.icon)} alt="" class="h-9 w-9 rounded-[10px] object-cover" /> : <span class="text-xs text-muted">-</span>,
      },
      {
        key: 'name',
        title: t('admin.categories.table.name'),
        class: 'min-w-[260px]',
        render: (r) => (
          <div class={['space-y-1', r.depth > 0 && 'pl-6']}>
            <div class="flex items-center gap-2">
              <Badge tone={r.depth > 0 ? 'info' : 'secondary'}>{r.depth > 0 ? t('admin.categories.level.child') : t('admin.categories.level.root')}</Badge>
              <span class="font-medium">{getLocalizedText(r.category.name)}</span>
            </div>
            {r.parent && <p class="text-xs text-muted">{t('admin.categories.table.parentPrefix', { name: getLocalizedText(r.parent.name) })}</p>}
          </div>
        ),
      },
      { key: 'slug', title: t('admin.categories.table.slug'), class: 'font-mono text-xs text-muted', render: (r) => r.category.slug },
      { key: 'sort', title: t('admin.categories.table.sort'), class: 'zs-num', render: (r) => r.category.sort_order },
      {
        key: 'status',
        title: t('admin.categories.table.status'),
        render: (r) => (
          <button
            type="button"
            title={r.category.is_active ? t('admin.categories.status.clickToDeactivate') : t('admin.categories.status.clickToActivate')}
            onClick={() => p.toggleActive(r.category)}
          >
            <Badge tone={r.category.is_active ? 'success' : 'neutral'} dot>
              {r.category.is_active ? t('admin.categories.status.active') : t('admin.categories.status.inactive')}
            </Badge>
          </button>
        ),
      },
      {
        key: 'action',
        title: t('admin.categories.table.action'),
        align: 'right',
        render: (r) => (
          <div class="flex justify-end gap-2">
            <Button size="sm" onClick={() => p.openEdit(r.category)}>
              {t('admin.categories.actions.edit')}
            </Button>
            <Button size="sm" variant="danger" onClick={() => p.remove(r.category)}>
              {t('admin.categories.actions.delete')}
            </Button>
          </div>
        ),
      },
    ]

    return () => (
      <div class="space-y-6">
        <PageHeader title={t('admin.categories.title')}>
          {{
            actions: () => (
              <Button variant="primary" onClick={p.openCreate}>
                <Plus class="h-4 w-4" />
                {t('admin.categories.create')}
              </Button>
            ),
          }}
        </PageHeader>

        <DataTable
          columns={columns()}
          rows={p.hierarchy.value}
          rowKey={(r) => r.category.id}
          loading={p.loading.value}
          emptyText={t('admin.categories.empty')}
          minWidth="760px"
        />

        <Dialog
          v-model={p.modal.showModal.value}
          title={p.modal.isEditing.value ? t('admin.categories.modal.editTitle') : t('admin.categories.modal.createTitle')}
          size="md"
        >
          {{
            default: () => (
              <form
                class="space-y-5"
                onSubmit={(e: Event) => {
                  e.preventDefault()
                  p.submit()
                }}
              >
                <FormField label={t('admin.categories.table.name')} required error={p.errors.name}>
                  <LocalizedInput v-model={p.form.name} placeholder={t('admin.categories.form.namePlaceholder')} />
                </FormField>
                <FormField label={t('admin.categories.form.slug')} required error={p.errors.slug}>
                  <Input v-model={p.form.slug} placeholder={t('admin.categories.form.slugPlaceholder')} />
                </FormField>
                <FormField
                  label={t('admin.categories.form.parent')}
                  hint={p.canChooseParent.value ? t('admin.categories.form.parentTip') : t('admin.categories.form.parentLockedTip')}
                >
                  <Select v-model={p.form.parent_id} options={p.parentOptions.value} disabled={!p.canChooseParent.value} />
                </FormField>
                <FormField label={t('admin.categories.form.icon')}>
                  <MediaPicker v-model={p.form.icon} scene="category" size="sm" />
                </FormField>
                <FormField label={t('admin.categories.form.sortOrder')} hint={t('admin.categories.form.sortTip')}>
                  <Input type="number" v-model={p.form.sort_order} placeholder="0" />
                </FormField>
                {p.modal.error.value && <p class="text-xs text-danger-text">{p.modal.error.value}</p>}
              </form>
            ),
            footer: () => (
              <>
                <Button onClick={p.modal.closeModal}>{t('admin.common.cancel')}</Button>
                <Button variant="primary" loading={p.modal.submitting.value} onClick={p.submit}>
                  {t('admin.common.save')}
                </Button>
              </>
            ),
          }}
        </Dialog>
      </div>
    )
  },
})
