import { defineComponent, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { RefreshCw, RotateCcw, Save, Search, Send } from 'lucide-vue-next'
import {
  Badge,
  Button,
  Card,
  DataTable,
  DateTimeInput,
  FilterBar,
  FormField,
  Input,
  ListPagination,
  PageHeader,
  Select,
  type DataTableColumn,
} from '@/components/ui'
import type { AdminNotificationLog } from '@/api/types'
import { formatDate } from '@/utils/format'
import {
  NOTIFICATION_CHANNELS,
  NOTIFICATION_SCENES,
  NOTIFICATION_SCENE_LABEL_KEYS,
  notificationChannelLabel,
  notificationSceneLabel,
  useNotificationCenter,
  type NotificationChannel,
  type NotificationScene,
} from './settings/useNotificationCenter'
import NotificationSettingsTab from './settings/NotificationSettingsTab'
import { LangSwitcher } from './settings/SettingsUi'

export default defineComponent({
  name: 'NotificationsView',
  setup() {
    const { t } = useI18n()
    const k = (s: string) => t(`admin.settings.notification.${s}`)
    const m = useNotificationCenter()
    onMounted(() => void m.init())

    const channelOptions = () => NOTIFICATION_CHANNELS.map((c) => ({ label: notificationChannelLabel(c), value: c }))
    const sceneOptions = () => NOTIFICATION_SCENES.map((s) => ({ label: t(NOTIFICATION_SCENE_LABEL_KEYS[s]), value: s }))

    const columns = (): DataTableColumn<AdminNotificationLog>[] => [
      { key: 'createdAt', title: k('logs.table.createdAt'), class: 'whitespace-nowrap text-xs text-muted', render: (r) => formatDate(r.created_at) },
      {
        key: 'scene',
        title: k('logs.table.scene'),
        render: (r) => (
          <div class="text-xs">
            <div class="text-fg">{notificationSceneLabel(r.event_type)}</div>
            <div class="mt-1 text-muted">{r.locale || '-'}</div>
          </div>
        ),
      },
      { key: 'channel', title: k('logs.table.channel'), class: 'text-xs', render: (r) => notificationChannelLabel(r.channel) },
      { key: 'recipient', title: k('logs.table.recipient'), class: 'break-all font-mono text-xs', render: (r) => r.recipient || '-' },
      {
        key: 'type',
        title: k('logs.table.type'),
        render: (r) => <Badge tone={r.is_test ? 'info' : 'neutral'}>{r.is_test ? k('logs.type.test') : k('logs.type.live')}</Badge>,
      },
      {
        key: 'status',
        title: k('logs.table.status'),
        render: (r) => (
          <Badge tone={r.status === 'success' ? 'success' : 'danger'} dot>
            {r.status === 'success' ? k('logs.status.success') : k('logs.status.failed')}
          </Badge>
        ),
      },
      {
        key: 'content',
        title: k('logs.table.content'),
        class: 'min-w-[220px] text-xs',
        render: (r) => (
          <div>
            <div class="break-words font-medium text-fg">{r.title || '-'}</div>
            <div class="mt-1 whitespace-pre-line break-words text-muted">{r.body || '-'}</div>
          </div>
        ),
      },
      {
        key: 'error',
        title: k('logs.table.error'),
        class: 'min-w-[160px] whitespace-pre-line break-words text-xs text-danger-text',
        render: (r) => r.error_message || '-',
      },
    ]

    return () => {
      const tf = m.testForm
      const lf = m.logFilters
      return (
        <div class="space-y-6">
          <PageHeader title={k('title')} subtitle={k('subtitle')}>
            {{
              actions: () => (
                <>
                  <LangSwitcher v-model={m.currentLang.value} />
                  <Button variant="primary" loading={m.submitting.value} disabled={m.loading.value || m.submitting.value} onClick={m.save}>
                    {!m.submitting.value && <Save class="h-4 w-4" />}
                    {m.submitting.value ? t('admin.settings.actions.saving') : t('admin.settings.actions.save')}
                  </Button>
                </>
              ),
            }}
          </PageHeader>

          <NotificationSettingsTab model={m} />

          <Card title={k('test.title')} description={k('test.subtitle')}>
            <div class="space-y-4">
              <div class="grid grid-cols-1 gap-4 lg:grid-cols-4">
                <FormField label={k('test.channel')}>
                  <Select
                    modelValue={tf.channel}
                    options={channelOptions()}
                    onUpdate:modelValue={(v) => {
                      tf.channel = String(v) as NotificationChannel
                      m.syncTestTarget(true)
                    }}
                  />
                </FormField>
                <FormField label={k('test.scene')}>
                  <Select modelValue={tf.scene} options={sceneOptions()} onUpdate:modelValue={(v) => (tf.scene = String(v) as NotificationScene)} />
                </FormField>
                <div class="lg:col-span-2">
                  <FormField>
                    {{
                      label: () => (
                        <span class="flex w-full items-center justify-between gap-3">
                          {k('test.target')}
                          {m.currentChannelTargets.value.length > 0 && (
                            <button type="button" class="text-xs font-normal text-accent hover:underline" onClick={() => m.syncTestTarget(true)}>
                              {k('test.useFirstRecipient')}
                            </button>
                          )}
                        </span>
                      ),
                      default: () => <Input v-model={tf.target} placeholder={k('test.targetPlaceholder')} />,
                    }}
                  </FormField>
                </div>
              </div>
              {m.currentChannelTargets.value.length > 0 && (
                <div class="flex flex-wrap gap-2">
                  {m.currentChannelTargets.value.slice(0, 3).map((target) => (
                    <button
                      key={target}
                      type="button"
                      class="rounded-full border border-line bg-surface-muted px-3 py-1 text-xs text-muted hover:border-primary hover:text-primary"
                      onClick={() => (tf.target = target)}
                    >
                      {target}
                    </button>
                  ))}
                </div>
              )}
              <div class="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
                <p class="text-xs text-muted">{k('test.hint')}</p>
                <Button variant="soft" loading={m.testing.value} disabled={m.testing.value} onClick={m.sendTest}>
                  <Send class="h-3.5 w-3.5" />
                  {m.testing.value ? k('test.sending') : k('test.sendAction')}
                </Button>
              </div>
            </div>
          </Card>

          <Card title={k('logs.title')} description={k('logs.subtitle')}>
            <div class="space-y-4">
              <FilterBar cols={4}>
                {{
                  default: () => (
                    <>
                      <Select
                        v-model={lf.channel}
                        onChange={m.logs.handleSearch}
                        options={[{ label: k('logs.filters.allChannels'), value: '__all__' }, ...channelOptions()]}
                      />
                      <Select
                        v-model={lf.status}
                        onChange={m.logs.handleSearch}
                        options={[
                          { label: k('logs.filters.allStatuses'), value: '__all__' },
                          { label: k('logs.status.success'), value: 'success' },
                          { label: k('logs.status.failed'), value: 'failed' },
                        ]}
                      />
                      <Select
                        v-model={lf.eventType}
                        onChange={m.logs.handleSearch}
                        options={[{ label: k('logs.filters.allScenes'), value: '__all__' }, ...sceneOptions()]}
                      />
                      <Select
                        v-model={lf.isTest}
                        onChange={m.logs.handleSearch}
                        options={[
                          { label: k('logs.filters.allTypes'), value: '__all__' },
                          { label: k('logs.type.live'), value: 'false' },
                          { label: k('logs.type.test'), value: 'true' },
                        ]}
                      />
                      <DateTimeInput v-model={lf.createdFrom} placeholder={k('logs.filters.createdFrom')} onUpdate:modelValue={m.logs.debouncedSearch} />
                      <DateTimeInput v-model={lf.createdTo} placeholder={k('logs.filters.createdTo')} onUpdate:modelValue={m.logs.debouncedSearch} />
                    </>
                  ),
                  actions: () => (
                    <>
                      <Button size="sm" onClick={m.resetLogFilters}>
                        <RotateCcw class="h-3.5 w-3.5" />
                        {k('logs.actions.reset')}
                      </Button>
                      <Button size="sm" loading={m.refreshing.value} onClick={m.refreshLogs}>
                        <RefreshCw class="h-3.5 w-3.5" />
                        {k('logs.actions.refresh')}
                      </Button>
                      <Button size="sm" variant="primary" onClick={m.logs.handleSearch}>
                        <Search class="h-3.5 w-3.5" />
                        {k('logs.actions.search')}
                      </Button>
                    </>
                  ),
                }}
              </FilterBar>
              <div>
                <DataTable
                  columns={columns()}
                  rows={m.logs.items.value}
                  rowKey={(r) => r.id}
                  loading={m.logs.loading.value}
                  emptyText={k('logs.empty')}
                  minWidth="940px"
                />
                <ListPagination pagination={m.logs.pagination.value} onChangePage={m.logs.changePage} onChangePageSize={m.logs.changePageSize} />
              </div>
            </div>
          </Card>
        </div>
      )
    }
  },
})
