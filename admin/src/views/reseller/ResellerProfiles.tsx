import { defineComponent, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRouter } from 'vue-router'
import { Search } from 'lucide-vue-next'
import {
  Badge,
  Button,
  DataTable,
  Dialog,
  FilterBar,
  FormField,
  IdCell,
  Input,
  ListPagination,
  PageHeader,
  Select,
  Textarea,
  type DataTableColumn,
} from '@/components/ui'
import type { AdminResellerProfile } from '@/api/types'
import { adminUrl } from '@/utils/adminBase'
import { formatDate } from '@/utils/format'
import { getResellerProfileActionState, getResellerProfileStatusKey } from '@/utils/resellerManagement'
import { CreatedRange } from './components/CreatedRange'
import { ProfileEditDialog } from './components/ProfileEditDialog'
import { RefreshButton } from './components/RefreshButton'
import { RESELLER_PROFILE_STATUSES, RESELLER_SETTLEMENT_STATUSES, profileStatusTone, settlementTone } from './resellerUtils'
import { useResellerProfiles } from './useResellerProfiles'

export default defineComponent({
  name: 'ResellerProfilesView',
  setup() {
    const { t } = useI18n()
    const router = useRouter()
    const p = useResellerProfiles()
    const { filters, list } = p
    onMounted(() => void list.fetchData(1))

    const statusLabel = (s?: string) => t(`admin.resellerProfiles.status.${getResellerProfileStatusKey(s)}`)
    const settlementLabel = (s?: string) =>
      s && (RESELLER_SETTLEMENT_STATUSES as readonly string[]).includes(s) ? t(`admin.resellerProfiles.settlement.${s}`) : s || '-'

    const columns = (): DataTableColumn<AdminResellerProfile>[] => [
      {
        key: 'id',
        title: t('admin.resellerProfiles.table.id'),
        render: (r) => <IdCell value={r.id} />,
      },
      {
        key: 'user',
        title: t('admin.resellerProfiles.table.user'),
        render: (r) => (
          <div class="min-w-[180px] text-xs text-muted">
            <div class="break-words text-sm text-fg">{r.user?.display_name || '-'}</div>
            {r.user?.email && <div class="mt-0.5 break-all">{r.user.email}</div>}
            {r.user_id > 0 && (
              <a href={adminUrl(`/users/${r.user_id}`)} target="_blank" rel="noopener" class="mt-0.5 inline-block font-mono text-accent hover:underline">
                #{r.user_id}
              </a>
            )}
          </div>
        ),
      },
      {
        key: 'status',
        title: t('admin.resellerProfiles.table.status'),
        render: (r) => (
          <Badge tone={profileStatusTone(r.status)} dot>
            {statusLabel(r.status)}
          </Badge>
        ),
      },
      {
        key: 'settlement',
        title: t('admin.resellerProfiles.table.settlement'),
        render: (r) => <Badge tone={settlementTone(r.settlement_status)}>{settlementLabel(r.settlement_status)}</Badge>,
      },
      {
        key: 'defaultMarkup',
        title: t('admin.resellerProfiles.table.defaultMarkup'),
        class: 'zs-num text-sm',
        render: (r) => r.default_markup_percent || '0.00',
      },
      {
        key: 'maxMarkup',
        title: t('admin.resellerProfiles.table.maxMarkup'),
        class: 'zs-num text-sm',
        render: (r) => r.max_markup_percent || '0.00',
      },
      {
        key: 'applyReason',
        title: t('admin.resellerProfiles.table.applyReason'),
        class: 'min-w-[160px] break-words text-xs text-muted',
        render: (r) => r.apply_reason || '-',
      },
      {
        key: 'rejectReason',
        title: t('admin.resellerProfiles.table.rejectReason'),
        class: 'min-w-[160px] break-words text-xs text-muted',
        render: (r) => r.reject_reason || '-',
      },
      {
        key: 'reviewedAt',
        title: t('admin.resellerProfiles.table.reviewedAt'),
        class: 'text-xs text-muted whitespace-nowrap',
        render: (r) => formatDate(r.reviewed_at) || '-',
      },
      {
        key: 'createdAt',
        title: t('admin.resellerProfiles.table.createdAt'),
        class: 'text-xs text-muted whitespace-nowrap',
        render: (r) => formatDate(r.created_at),
      },
      {
        key: 'action',
        title: t('admin.resellerProfiles.table.action'),
        align: 'right',
        render: (r) => {
          const state = getResellerProfileActionState(r.status)
          const busy = p.operatingId.value === r.id
          return (
            <div class="flex min-w-[220px] flex-wrap items-center justify-end gap-1.5">
              <Button size="xs" variant="soft" onClick={() => router.push(`/resellers/profiles/${r.id}`)}>
                {t('admin.resellerProfiles.actions.detail')}
              </Button>
              <Button size="xs" disabled={busy} onClick={() => p.openEdit(r)}>
                {t('admin.common.edit')}
              </Button>
              <Button size="xs" variant="primary" disabled={busy || !state.canApprove} onClick={() => p.openApprove(r)}>
                {t('admin.resellerProfiles.actions.approve')}
              </Button>
              <Button size="xs" disabled={busy || !state.canReject} onClick={() => p.openReason(r, 'reject')}>
                {t('admin.resellerProfiles.actions.reject')}
              </Button>
              <Button size="xs" disabled={busy || !state.canDisable} onClick={() => p.openReason(r, 'disable')}>
                {t('admin.resellerProfiles.actions.disable')}
              </Button>
              <Button size="xs" disabled={busy || !state.canRestore} onClick={() => p.restore(r)}>
                {t('admin.resellerProfiles.actions.restore')}
              </Button>
            </div>
          )
        },
      },
    ]

    return () => {
      const sel = p.selected.value
      const selId = sel?.id ?? '-'
      const busy = !!sel && p.operatingId.value === sel.id
      const isReject = p.reasonAction.value === 'reject'
      return (
        <div class="space-y-6">
          <PageHeader title={t('admin.resellerProfiles.title')} />
          <FilterBar cols={4}>
            {{
              default: () => (
                <>
                  <Input
                    icon={Search}
                    v-model={filters.keyword}
                    placeholder={t('admin.resellerProfiles.filters.keyword')}
                    onUpdate:modelValue={list.debouncedSearch}
                  />
                  <Input v-model={filters.userId} placeholder={t('admin.resellerProfiles.filters.userId')} onUpdate:modelValue={list.debouncedSearch} />
                  <Select
                    v-model={filters.status}
                    onChange={list.handleSearch}
                    options={[
                      {
                        label: t('admin.resellerProfiles.filters.statusAll'),
                        value: '__all__',
                      },
                      ...RESELLER_PROFILE_STATUSES.map((v) => ({
                        label: statusLabel(v),
                        value: v,
                      })),
                    ]}
                  />
                  <Select
                    v-model={filters.settlementStatus}
                    onChange={list.handleSearch}
                    options={[
                      {
                        label: t('admin.resellerProfiles.filters.settlementAll'),
                        value: '__all__',
                      },
                      ...RESELLER_SETTLEMENT_STATUSES.map((v) => ({
                        label: settlementLabel(v),
                        value: v,
                      })),
                    ]}
                  />
                  <CreatedRange
                    label={t('admin.resellerProfiles.filters.createdRange')}
                    v-model:from={filters.createdFrom}
                    v-model:to={filters.createdTo}
                    onChange={list.handleSearch}
                  />
                </>
              ),
              actions: () => <RefreshButton loading={p.refreshing.value} onClick={p.refresh} />,
            }}
          </FilterBar>
          <div>
            <DataTable
              columns={columns()}
              rows={list.items.value}
              rowKey={(r) => r.id}
              loading={list.loading.value}
              emptyText={t('admin.resellerProfiles.empty')}
              minWidth="1280px"
            />
            <ListPagination pagination={list.pagination.value} onChangePage={list.changePage} onChangePageSize={list.changePageSize} />
          </div>

          <Dialog
            v-model={p.showApproveDialog.value}
            title={t('admin.resellerProfiles.actions.approveDialogTitle', {
              id: selId,
            })}
            size="sm"
          >
            {{
              default: () => (
                <div class="space-y-4">
                  <FormField label={t('admin.resellerProfiles.table.defaultMarkup')}>
                    <Input v-model={p.approveForm.defaultMarkup} placeholder="0.00" mono />
                  </FormField>
                  <FormField label={t('admin.resellerProfiles.table.maxMarkup')} hint={t('admin.resellerProfiles.actions.maxMarkupZeroHint')}>
                    <Input v-model={p.approveForm.maxMarkup} placeholder="0.00" mono />
                  </FormField>
                </div>
              ),
              footer: () => (
                <>
                  <Button onClick={() => (p.showApproveDialog.value = false)}>{t('admin.common.cancel')}</Button>
                  <Button variant="primary" loading={busy} onClick={p.submitApprove}>
                    {t('admin.resellerProfiles.actions.approveConfirm')}
                  </Button>
                </>
              ),
            }}
          </Dialog>

          <Dialog
            v-model={p.showReasonDialog.value}
            title={
              isReject
                ? t('admin.resellerProfiles.actions.rejectDialogTitle', {
                    id: selId,
                  })
                : t('admin.resellerProfiles.actions.disableDialogTitle', {
                    id: selId,
                  })
            }
            size="sm"
          >
            {{
              default: () => (
                <FormField
                  label={isReject ? t('admin.resellerProfiles.actions.rejectReasonPrompt') : t('admin.resellerProfiles.actions.disableReasonPrompt')}
                  required={isReject}
                >
                  <Textarea v-model={p.reasonForm.reason} rows={4} />
                </FormField>
              ),
              footer: () => (
                <>
                  <Button onClick={() => (p.showReasonDialog.value = false)}>{t('admin.common.cancel')}</Button>
                  <Button variant={isReject ? 'primary' : 'danger'} loading={busy} onClick={p.submitReason}>
                    {isReject ? t('admin.resellerProfiles.actions.confirmReject') : t('admin.resellerProfiles.actions.confirmDisable')}
                  </Button>
                </>
              ),
            }}
          </Dialog>

          <ProfileEditDialog v-model={p.showEditDialog.value} profile={sel} saving={busy} onSubmit={p.submitEdit} />
        </div>
      )
    }
  },
})
