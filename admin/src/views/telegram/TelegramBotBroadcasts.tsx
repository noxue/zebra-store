import { defineComponent, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRouter } from 'vue-router'
import { Eye, RefreshCw, RotateCcw, Search, Send, Trash2 } from 'lucide-vue-next'
import { Badge, Button, DataTable, DateTimeInput, FilterBar, IdCell, Input, ListPagination, PageHeader, Select, type DataTableColumn } from '@/components/ui'
import type { AdminTelegramBroadcast } from '@/api/types'
import { formatDate } from '@/utils/format'
import { broadcastStatusTone, canDeleteBroadcast, formatBroadcastStatus, formatRecipientType } from './telegramUtils'
import { useTelegramBroadcasts } from './useTelegramBroadcasts'

export default defineComponent({
  name: 'TelegramBotBroadcasts',
  setup() {
    const { t } = useI18n()
    const router = useRouter()
    const { filters, list, refreshing, refresh, resetFilters, deletingId, handleDelete } = useTelegramBroadcasts()
    onMounted(() => void list.fetchData(1))

    const columns = (): DataTableColumn<AdminTelegramBroadcast>[] => [
      { key: 'id', title: 'ID', render: (r) => <IdCell value={r.id} /> },
      {
        key: 'title',
        title: t('telegramBot.broadcasts.tableTitle'),
        class: 'min-w-[200px]',
        render: (r) => (
          <div class="space-y-1">
            <div class="break-words font-medium">{r.title}</div>
            {r.last_error && <div class="break-words text-xs text-danger-text">{r.last_error}</div>}
          </div>
        ),
      },
      {
        key: 'recipientType',
        title: t('telegramBot.broadcasts.tableRecipientType'),
        render: (r) => <Badge tone={r.recipient_type === 'specific' ? 'secondary' : 'info'}>{formatRecipientType(t, r.recipient_type)}</Badge>,
      },
      {
        key: 'status',
        title: t('telegramBot.broadcasts.tableStatus'),
        render: (r) => (
          <Badge tone={broadcastStatusTone(r.status)} dot>
            {formatBroadcastStatus(t, r.status)}
          </Badge>
        ),
      },
      { key: 'recipientCount', title: t('telegramBot.broadcasts.tableRecipientCount'), class: 'zs-num', render: (r) => r.recipient_count },
      { key: 'successCount', title: t('telegramBot.broadcasts.tableSuccessCount'), class: 'zs-num text-success-text', render: (r) => r.success_count },
      { key: 'failedCount', title: t('telegramBot.broadcasts.tableFailedCount'), class: 'zs-num', render: (r) => <span class={r.failed_count > 0 ? 'text-danger-text' : ''}>{r.failed_count}</span> },
      { key: 'createdAt', title: t('telegramBot.broadcasts.tableCreatedAt'), class: 'whitespace-nowrap text-xs text-muted', render: (r) => formatDate(r.created_at) || '-' },
      { key: 'completedAt', title: t('telegramBot.broadcasts.tableCompletedAt'), class: 'whitespace-nowrap text-xs text-muted', render: (r) => formatDate(r.completed_at || '') || '-' },
      {
        key: 'actions',
        title: t('telegramBot.broadcasts.tableActions'),
        align: 'right',
        render: (r) => (
          <div class="flex items-center justify-end gap-2">
            <Button size="sm" onClick={() => void router.push(`/telegram-bot/broadcasts/${r.id}`)}>
              <Eye class="h-4 w-4" />
              {t('telegramBot.broadcasts.detail')}
            </Button>
            <Button
              size="sm"
              variant="danger"
              disabled={!canDeleteBroadcast(r.status)}
              loading={deletingId.value === r.id}
              onClick={() => void handleDelete(r)}
            >
              <Trash2 class="h-4 w-4" />
              {t('telegramBot.broadcasts.delete')}
            </Button>
          </div>
        ),
      },
    ]

    return () => (
      <div class="space-y-6">
        <PageHeader title={t('telegramBot.broadcasts.title')} subtitle={t('telegramBot.broadcasts.subtitle')}>
          {{
            actions: () => (
              <Button variant="primary" onClick={() => void router.push('/telegram-bot/broadcasts/create')}>
                <Send class="h-4 w-4" />
                {t('telegramBot.broadcasts.create')}
              </Button>
            ),
          }}
        </PageHeader>

        <FilterBar cols={5}>
          {{
            default: () => (
              <>
                <Input icon={Search} v-model={filters.keyword} placeholder={t('telegramBot.broadcasts.filterTitleKeyword')} onEnter={list.handleSearch} />
                <Select
                  v-model={filters.recipientType}
                  onChange={list.handleSearch}
                  options={[
                    { label: t('telegramBot.broadcasts.filterRecipientTypeAll'), value: '__all__' },
                    { label: t('telegramBot.broadcasts.recipientTypeAll'), value: 'all' },
                    { label: t('telegramBot.broadcasts.recipientTypeSpecific'), value: 'specific' },
                  ]}
                />
                <Select
                  v-model={filters.status}
                  onChange={list.handleSearch}
                  options={[
                    { label: t('telegramBot.broadcasts.filterStatusAll'), value: '__all__' },
                    { label: t('telegramBot.broadcasts.statusPending'), value: 'pending' },
                    { label: t('telegramBot.broadcasts.statusRunning'), value: 'running' },
                    { label: t('telegramBot.broadcasts.statusCompleted'), value: 'completed' },
                    { label: t('telegramBot.broadcasts.statusFailed'), value: 'failed' },
                  ]}
                />
                <DateTimeInput v-model={filters.createdFrom} placeholder={t('telegramBot.zebra.createdFrom')} />
                <DateTimeInput v-model={filters.createdTo} placeholder={t('telegramBot.zebra.createdTo')} />
              </>
            ),
            actions: () => (
              <>
                <Button size="sm" variant="primary" onClick={list.handleSearch}>
                  <Search class="h-3.5 w-3.5" />
                  {t('telegramBot.broadcasts.search')}
                </Button>
                <Button size="sm" onClick={resetFilters}>
                  <RotateCcw class="h-3.5 w-3.5" />
                  {t('telegramBot.broadcasts.resetFilters')}
                </Button>
                <Button size="sm" loading={refreshing.value} onClick={refresh}>
                  <RefreshCw class="h-3.5 w-3.5" />
                  {t('admin.common.refresh')}
                </Button>
              </>
            ),
          }}
        </FilterBar>

        <div>
          <DataTable
            columns={columns()}
            rows={list.items.value}
            rowKey={(r) => r.id}
            loading={list.loading.value}
            emptyText={t('telegramBot.broadcasts.empty')}
            minWidth="980px"
          />
          <ListPagination pagination={list.pagination.value} onChangePage={list.changePage} onChangePageSize={list.changePageSize} />
        </div>
      </div>
    )
  },
})
