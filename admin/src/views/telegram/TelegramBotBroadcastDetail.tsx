import { defineComponent, onMounted, type FunctionalComponent } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRouter } from 'vue-router'
import { ArrowLeft, Paperclip } from 'lucide-vue-next'
import { Badge, Button, Card, EmptyState, Loader, PageHeader } from '@/components/ui'
import { formatDate } from '@/utils/format'
import { getImageUrl } from '@/utils/image'
import { broadcastStatusTone, formatBroadcastStatus, formatRecipientType, toTelegramHtml } from './telegramUtils'
import { useTelegramBroadcastDetail } from './useTelegramBroadcastDetail'

const Field: FunctionalComponent<{ label: string }> = (props, { slots }) => (
  <div>
    <dt class="text-xs font-medium text-muted">{props.label}</dt>
    <dd class="mt-1 text-sm text-fg">{slots.default?.()}</dd>
  </div>
)

export default defineComponent({
  name: 'TelegramBotBroadcastDetail',
  setup() {
    const { t } = useI18n()
    const router = useRouter()
    const { loading, broadcast, fetchBroadcast } = useTelegramBroadcastDetail()
    onMounted(() => void fetchBroadcast())

    const body = () => {
      if (loading.value) {
        return (
          <div class="flex justify-center py-20">
            <Loader />
          </div>
        )
      }
      const b = broadcast.value
      if (!b) return <EmptyState title={t('telegramBot.broadcasts.detailLoadFailed')} />
      return (
        <>
          <Card title={t('telegramBot.broadcasts.detailBasicInfo')}>
            <dl class="grid grid-cols-1 gap-4 sm:grid-cols-2">
              <Field label="ID">
                <span class="zs-num">#{b.id}</span>
              </Field>
              <Field label={t('telegramBot.broadcasts.fieldTitle')}>{b.title}</Field>
              <Field label={t('telegramBot.broadcasts.tableRecipientType')}>
                <Badge tone={b.recipient_type === 'specific' ? 'secondary' : 'info'}>{formatRecipientType(t, b.recipient_type)}</Badge>
              </Field>
              <Field label={t('telegramBot.broadcasts.tableCreatedAt')}>{formatDate(b.created_at) || '-'}</Field>
            </dl>
          </Card>

          <Card title={t('telegramBot.broadcasts.detailExecution')}>
            <dl class="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3">
              <Field label={t('telegramBot.broadcasts.tableStatus')}>
                <Badge tone={broadcastStatusTone(b.status)} dot size="md">
                  {formatBroadcastStatus(t, b.status)}
                </Badge>
              </Field>
              <Field label={t('telegramBot.broadcasts.tableRecipientCount')}>
                <span class="zs-num">{b.recipient_count}</span>
              </Field>
              <Field label={t('telegramBot.broadcasts.tableSuccessCount')}>
                <span class="zs-num text-success-text">{b.success_count}</span>
              </Field>
              <Field label={t('telegramBot.broadcasts.tableFailedCount')}>
                <span class={['zs-num', b.failed_count > 0 && 'text-danger-text']}>{b.failed_count}</span>
              </Field>
              <Field label={t('telegramBot.broadcasts.fieldStartedAt')}>{formatDate(b.started_at || '') || '-'}</Field>
              <Field label={t('telegramBot.broadcasts.fieldCompletedAt')}>{formatDate(b.completed_at || '') || '-'}</Field>
            </dl>
            {b.last_error && (
              <div class="mt-4">
                <p class="text-xs font-medium text-muted">{t('telegramBot.broadcasts.fieldLastError')}</p>
                <p class="mt-1 rounded-zs bg-danger-soft p-3 text-sm text-danger-text">{b.last_error}</p>
              </div>
            )}
          </Card>

          <Card title={t('telegramBot.broadcasts.detailMessageContent')}>
            <div class="rounded-zs border border-line bg-surface-strong p-4">
              <div class="zs-prose max-w-none whitespace-pre-wrap break-words text-sm" innerHTML={toTelegramHtml(b.message_html)} />
            </div>
          </Card>

          {b.attachment_url && (
            <Card title={t('telegramBot.broadcasts.detailAttachment')}>
              <div class="flex items-center gap-3">
                <Paperclip class="h-4 w-4 text-muted" />
                <a href={getImageUrl(b.attachment_url)} target="_blank" rel="noopener noreferrer" class="break-all text-sm text-primary underline underline-offset-4">
                  {b.attachment_name || b.attachment_url}
                </a>
              </div>
            </Card>
          )}
        </>
      )
    }

    return () => (
      <div class="space-y-6">
        <PageHeader title={t('telegramBot.broadcasts.detailTitle')} subtitle={t('telegramBot.broadcasts.detailSubtitle')}>
          {{
            actions: () => (
              <Button onClick={() => void router.push('/telegram-bot/broadcasts')}>
                <ArrowLeft class="h-4 w-4" />
                {t('telegramBot.broadcasts.backToList')}
              </Button>
            ),
          }}
        </PageHeader>
        {body()}
      </div>
    )
  },
})
