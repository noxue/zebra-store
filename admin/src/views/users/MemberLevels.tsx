import { defineComponent, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { Plus, RefreshCw, UsersRound } from 'lucide-vue-next'
import {
  Badge,
  Button,
  DataTable,
  Dialog,
  FormField,
  IdCell,
  Input,
  ListPagination,
  LocalizedInput,
  PageHeader,
  RadioGroup,
  Switch,
  type DataTableColumn,
} from '@/components/ui'
import { MediaPicker } from '@/components/MediaPicker'
import type { AdminMemberLevel } from '@/api/types'
import { formatDate, getLocalizedText } from '@/utils/format'
import { getImageUrl } from '@/utils/image'
import { isImagePath } from './usersUtils'
import { useMemberLevels, type IconMode } from './useMemberLevels'

export default defineComponent({
  name: 'MemberLevelsView',
  setup() {
    const { t } = useI18n()
    const p = useMemberLevels()
    onMounted(() => void p.list.fetchData(1))

    const columns = (): DataTableColumn<AdminMemberLevel>[] => [
      { key: 'id', title: t('admin.memberLevels.table.id'), render: (r) => <IdCell value={r.id} /> },
      {
        key: 'icon',
        title: t('admin.memberLevels.table.icon'),
        render: (r) =>
          r.icon && isImagePath(r.icon) ? (
            <img
              src={getImageUrl(r.icon)}
              alt={getLocalizedText(r.name)}
              class="h-8 w-8 shrink-0 rounded-[10px] object-cover"
              onError={(e: Event) => ((e.target as HTMLImageElement).style.display = 'none')}
            />
          ) : r.icon ? (
            <span class="text-lg">{r.icon}</span>
          ) : (
            <span class="text-xs text-muted">-</span>
          ),
      },
      {
        key: 'name',
        title: t('admin.memberLevels.table.name'),
        class: 'min-w-[120px] font-medium',
        render: (r) => (
          <div class="flex items-center gap-2">
            <span class="break-words">{getLocalizedText(r.name)}</span>
            {r.is_default && <Badge tone="info">{t('admin.memberLevels.default')}</Badge>}
          </div>
        ),
      },
      { key: 'slug', title: t('admin.memberLevels.table.slug'), class: 'font-mono text-xs text-muted break-all', render: (r) => r.slug },
      { key: 'discountRate', title: t('admin.memberLevels.table.discountRate'), class: 'zs-num', render: (r) => `${r.discount_rate}%` },
      { key: 'rechargeThreshold', title: t('admin.memberLevels.table.rechargeThreshold'), class: 'zs-num text-xs text-muted', render: (r) => Number(r.recharge_threshold) || '-' },
      { key: 'spendThreshold', title: t('admin.memberLevels.table.spendThreshold'), class: 'zs-num text-xs text-muted', render: (r) => Number(r.spend_threshold) || '-' },
      { key: 'sortOrder', title: t('admin.memberLevels.table.sortOrder'), class: 'zs-num text-xs text-muted', render: (r) => r.sort_order },
      {
        key: 'isActive',
        title: t('admin.memberLevels.table.isActive'),
        render: (r) => (
          <Badge tone={r.is_active ? 'success' : 'neutral'} dot>
            {r.is_active ? t('admin.memberLevels.status.active') : t('admin.memberLevels.status.inactive')}
          </Badge>
        ),
      },
      { key: 'createdAt', title: t('admin.memberLevels.table.createdAt'), class: 'text-xs text-muted whitespace-nowrap', render: (r) => formatDate(r.created_at) },
      {
        key: 'action',
        title: t('admin.memberLevels.table.action'),
        align: 'right',
        render: (r) => (
          <div class="flex justify-end gap-2">
            <Button size="sm" onClick={() => p.openEdit(r)}>
              {t('admin.common.edit')}
            </Button>
            <Button size="sm" variant="danger" onClick={() => void p.remove(r)}>
              {t('admin.common.delete')}
            </Button>
          </div>
        ),
      },
    ]

    const numberField = (label: string, hint: string, key: 'discount_rate' | 'recharge_threshold' | 'spend_threshold' | 'sort_order', extra: { step?: string; max?: string; placeholder: string }) => (
      <FormField label={label} hint={hint}>
        <Input type="number" v-model={p.form[key]} min="0" step={extra.step} max={extra.max} placeholder={extra.placeholder} mono />
      </FormField>
    )

    return () => (
      <div class="space-y-6">
        <PageHeader title={t('admin.memberLevels.title')} subtitle={t('admin.memberLevels.subtitle')}>
          {{
            actions: () => (
              <>
                <Button loading={p.backfilling.value} onClick={p.backfill}>
                  <UsersRound class="h-4 w-4" />
                  {p.backfilling.value ? t('admin.memberLevels.backfilling') : t('admin.memberLevels.backfill')}
                </Button>
                <Button loading={p.refreshing.value} onClick={p.refresh}>
                  <RefreshCw class="h-4 w-4" />
                  {t('admin.common.refresh')}
                </Button>
                <Button variant="primary" onClick={p.openCreate}>
                  <Plus class="h-4 w-4" />
                  {t('admin.memberLevels.create')}
                </Button>
              </>
            ),
          }}
        </PageHeader>

        <div>
          <DataTable
            columns={columns()}
            rows={p.list.items.value}
            rowKey={(r) => r.id}
            loading={p.list.loading.value}
            emptyText={t('admin.memberLevels.empty')}
            minWidth="1000px"
          />
          <ListPagination hideWhenEmpty pagination={p.list.pagination.value} onChangePage={p.list.changePage} onChangePageSize={p.list.changePageSize} />
        </div>

        <Dialog
          v-model={p.modal.showModal.value}
          title={p.modal.isEditing.value ? t('admin.memberLevels.form.editTitle') : t('admin.memberLevels.form.createTitle')}
          size="2xl"
          closeOnOverlay={false}
        >
          {{
            default: () => (
              <form
                class="grid grid-cols-1 gap-4 md:grid-cols-2"
                onSubmit={(e: Event) => {
                  e.preventDefault()
                  p.submit()
                }}
              >
                <div class="md:col-span-2">
                  <FormField label={t('admin.memberLevels.form.name')} required>
                    <LocalizedInput v-model={p.form.name} placeholder={t('admin.memberLevels.form.namePlaceholder')} />
                  </FormField>
                </div>
                <FormField label={t('admin.memberLevels.form.slug')} required error={p.errors.slug}>
                  <Input v-model={p.form.slug} mono placeholder={t('admin.memberLevels.form.slugPlaceholder')} />
                </FormField>
                <FormField label={t('admin.memberLevels.form.icon')}>
                  <div class="space-y-2">
                    <RadioGroup
                      modelValue={p.iconMode.value}
                      onUpdate:modelValue={(v) => p.switchIconMode(v as IconMode)}
                      options={[
                        { label: 'Emoji', value: 'emoji' },
                        { label: t('admin.memberLevels.form.iconImage'), value: 'image' },
                      ]}
                    />
                    {p.iconMode.value === 'emoji' ? (
                      <Input v-model={p.form.icon} placeholder={t('admin.memberLevels.form.iconPlaceholder')} />
                    ) : (
                      <MediaPicker v-model={p.form.icon} scene="common" size="sm" />
                    )}
                  </div>
                </FormField>
                {numberField(t('admin.memberLevels.form.discountRate'), t('admin.memberLevels.form.discountRateHint'), 'discount_rate', { step: '0.01', max: '100', placeholder: '100' })}
                {numberField(t('admin.memberLevels.form.rechargeThreshold'), t('admin.memberLevels.form.rechargeThresholdHint'), 'recharge_threshold', { step: '0.01', placeholder: '0' })}
                {numberField(t('admin.memberLevels.form.spendThreshold'), t('admin.memberLevels.form.spendThresholdHint'), 'spend_threshold', { step: '0.01', placeholder: '0' })}
                {numberField(t('admin.memberLevels.form.sortOrder'), t('admin.memberLevels.form.sortOrderHint'), 'sort_order', { placeholder: '0' })}
                <div class="flex flex-wrap items-center gap-6 md:col-span-2">
                  <Switch v-model={p.form.is_default} label={t('admin.memberLevels.form.isDefault')} />
                  <Switch v-model={p.form.is_active} label={t('admin.memberLevels.form.isActive')} />
                </div>
                {p.modal.error.value && <p class="rounded-zs-sm bg-danger-soft px-3 py-2 text-xs text-danger-text md:col-span-2">{p.modal.error.value}</p>}
                <button type="submit" class="hidden" />
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
