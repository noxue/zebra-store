import { defineComponent, onMounted, watch } from 'vue'
import { useRoute } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { Plus } from 'lucide-vue-next'
import { Badge, Button, DataTable, Dialog, FormField, IdCell, Input, LocalizedInput, PageHeader, Select, type DataTableColumn } from '@/components/ui'
import { MediaPicker } from '@/components/MediaPicker'
import { getLocalizedText } from '@/utils/format'
import { getImageUrl } from '@/utils/image'
import type { PostCategoryHierarchyItem } from '@/utils/postCategory'
import { usePostCategories } from './usePostCategories'

export default defineComponent({
  name: 'PostCategoriesView',
  setup() {
    const { t } = useI18n()
    const route = useRoute()
    const p = usePostCategories()

    onMounted(() => {
      void p.fetchCategories()
      if (route.query.category_id) void p.openEditById(route.query.category_id)
    })
    watch(
      () => route.query.category_id,
      (id) => id && void p.openEditById(id),
    )

    const columns = (): DataTableColumn<PostCategoryHierarchyItem>[] => [
      { key: 'id', title: t('admin.categories.table.id'), render: (r) => <IdCell value={r.category.id} /> },
      {
        key: 'icon',
        title: t('admin.categories.table.icon'),
        render: (r) =>
          r.category.icon ? (
            <img src={getImageUrl(r.category.icon)} alt={getLocalizedText(r.category.name)} class="h-9 w-9 rounded-zs-sm object-cover" />
          ) : (
            <span class="text-xs text-muted">-</span>
          ),
      },
      {
        key: 'name',
        title: t('admin.categories.table.name'),
        class: 'min-w-[260px]',
        render: (r) => (
          <div class={['space-y-1', r.depth > 0 && 'pl-6']}>
            <div class="flex items-center gap-2">
              <Badge tone={r.depth > 0 ? 'info' : 'secondary'}>{r.depth > 0 ? t('admin.categories.level.child') : t('admin.categories.level.root')}</Badge>
              <span class="break-words font-medium">{getLocalizedText(r.category.name)}</span>
            </div>
            {r.parent && <p class="text-xs text-muted">{t('admin.categories.table.parentPrefix', { name: getLocalizedText(r.parent.name) })}</p>}
          </div>
        ),
      },
      { key: 'slug', title: t('admin.categories.table.slug'), class: 'min-w-[220px] break-all font-mono text-xs text-muted', render: (r) => r.category.slug },
      { key: 'sort', title: t('admin.categories.table.sort'), class: 'zs-num', render: (r) => r.category.sort_order ?? 0 },
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
        <PageHeader title={t('admin.postCategories.title')}>
          {{
            actions: () => (
              <Button variant="primary" onClick={p.openCreate}>
                <Plus class="h-4 w-4" />
                {t('admin.postCategories.create')}
              </Button>
            ),
          }}
        </PageHeader>

        <DataTable
          columns={columns()}
          rows={p.hierarchy.value}
          rowKey={(r) => r.category.id}
          loading={p.loading.value}
          emptyText={t('admin.postCategories.empty')}
          minWidth="860px"
        />

        <Dialog
          v-model={p.modal.showModal.value}
          title={p.modal.isEditing.value ? t('admin.postCategories.modal.editTitle') : t('admin.postCategories.modal.createTitle')}
          size="md"
          closeOnOverlay={false}
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
                <FormField label={t('admin.postCategories.form.name')} required error={p.errors.name}>
                  <LocalizedInput v-model={p.form.name} placeholder={t('admin.categories.form.namePlaceholder')} />
                </FormField>
                <FormField label={t('admin.categories.form.slug')} required error={p.errors.slug}>
                  <Input v-model={p.form.slug} placeholder={t('admin.categories.form.slugPlaceholder')} mono />
                </FormField>
                <FormField
                  label={t('admin.categories.form.parent')}
                  hint={p.canChooseParent.value ? t('admin.postCategories.form.parentTip') : t('admin.postCategories.form.parentLockedTip')}
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
                  {p.modal.isEditing.value ? t('admin.categories.actions.saveChanges') : t('admin.categories.actions.createNow')}
                </Button>
              </>
            ),
          }}
        </Dialog>
      </div>
    )
  },
})
