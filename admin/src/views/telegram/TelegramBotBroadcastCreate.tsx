import { defineComponent } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRouter } from 'vue-router'
import { ArrowLeft, ImageIcon, Paperclip, RotateCcw, Search, Send, Trash2, Users } from 'lucide-vue-next'
import {
  Button,
  Card,
  DataTable,
  DateTimeInput,
  FileInput,
  FormField,
  Input,
  ListPagination,
  PageHeader,
  RadioGroup,
  type DataTableColumn,
} from '@/components/ui'
import { MediaPicker } from '@/components/MediaPicker'
import { RichEditor } from '@/components/RichEditor'
import type { AdminTelegramBroadcastUser } from '@/api/types'
import { toRowSelection } from '@/composables/useSelection'
import { formatDate } from '@/utils/format'
import { useTelegramBroadcastCreate, type RecipientType } from './useTelegramBroadcastCreate'

export default defineComponent({
  name: 'TelegramBotBroadcastCreate',
  setup() {
    const { t } = useI18n()
    const router = useRouter()
    const p = useTelegramBroadcastCreate()

    const userColumns = (): DataTableColumn<AdminTelegramBroadcastUser>[] => [
      { key: 'displayName', title: t('telegramBot.broadcasts.userDisplayName'), class: 'min-w-[180px] break-words', render: (r) => r.display_name || '-' },
      {
        key: 'tgUsername',
        title: t('telegramBot.broadcasts.userTelegramUsername'),
        class: 'min-w-[180px] break-all',
        render: (r) => (r.telegram_username ? <span class="text-accent">@{r.telegram_username}</span> : '-'),
      },
      { key: 'tgId', title: t('telegramBot.broadcasts.userTelegramId'), class: 'min-w-[160px] break-all font-mono text-xs', render: (r) => r.telegram_user_id },
      { key: 'email', title: t('telegramBot.broadcasts.userEmail'), class: 'min-w-[220px] break-all', render: (r) => r.user_email || '-' },
      { key: 'boundAt', title: t('telegramBot.broadcasts.userBoundAt'), class: 'min-w-[160px] whitespace-nowrap text-xs text-muted', render: (r) => formatDate(r.bound_at) || '-' },
    ]

    const attachmentBlock = () => (
      <div class="rounded-zs border border-dashed border-line-strong p-4">
        {p.form.attachment_url ? (
          <div class="flex flex-wrap items-center justify-between gap-3">
            <div class="flex min-w-0 items-start gap-2">
              <Paperclip class="mt-0.5 h-4 w-4 shrink-0 text-primary" />
              <div class="min-w-0 space-y-1">
                <div class="font-medium text-fg">{p.form.attachment_name || t('telegramBot.broadcasts.attachmentUploaded')}</div>
                <div class="break-all text-xs text-muted">{p.form.attachment_url}</div>
              </div>
            </div>
            <Button size="sm" onClick={p.clearAttachment}>
              <Trash2 class="h-4 w-4" />
              {t('telegramBot.broadcasts.removeAttachment')}
            </Button>
          </div>
        ) : (
          <div class="flex flex-wrap items-center gap-3">
            <div class="w-full sm:w-64">
              <FileInput
                disabled={p.uploading.value}
                placeholder={p.uploading.value ? t('admin.common.loading') : t('telegramBot.broadcasts.uploadAttachment')}
                onChange={(f: File | null) => void p.handleAttachmentChange(f)}
              />
            </div>
            <Button onClick={() => (p.pickerOpen.value = true)}>
              <ImageIcon class="h-4 w-4" />
              {t('admin.mediaPicker.selectFromLibrary')}
            </Button>
            <span class="text-sm text-muted">{t('telegramBot.broadcasts.attachmentHint')}</span>
          </div>
        )}
        <MediaPicker
          dialogOnly
          open={p.pickerOpen.value}
          onUpdate:open={(v: boolean) => (p.pickerOpen.value = v)}
          modelValue=""
          scene="telegram"
          onUpdate:modelValue={p.handleMediaSelected}
        />
      </div>
    )

    const userSelector = () => (
      <Card title={t('telegramBot.broadcasts.userSelectorTitle')} description={t('telegramBot.broadcasts.userSelectorDesc')}>
        {{
          extra: () => (
            <span class="inline-flex items-center gap-2 rounded-full border border-line bg-primary-soft px-3 py-1 text-sm text-primary">
              <Users class="h-4 w-4" />
              {t('telegramBot.broadcasts.selectedCount', { count: p.selectedCount.value })}
            </span>
          ),
          default: () => (
            <div class="space-y-4">
              <div class="grid gap-3 md:grid-cols-2 xl:grid-cols-3">
                <Input icon={Search} v-model={p.filters.keyword} placeholder={t('telegramBot.broadcasts.filterKeyword')} onEnter={() => p.fetchUsers(1)} />
                <Input v-model={p.filters.display_name} placeholder={t('telegramBot.broadcasts.filterDisplayName')} onEnter={() => p.fetchUsers(1)} />
                <Input v-model={p.filters.telegram_username} placeholder={t('telegramBot.broadcasts.filterTelegramUsername')} onEnter={() => p.fetchUsers(1)} />
                <Input v-model={p.filters.telegram_user_id} placeholder={t('telegramBot.broadcasts.filterTelegramUserId')} onEnter={() => p.fetchUsers(1)} />
                <DateTimeInput v-model={p.filters.created_from} placeholder={t('telegramBot.zebra.createdFrom')} />
                <DateTimeInput v-model={p.filters.created_to} placeholder={t('telegramBot.zebra.createdTo')} />
              </div>
              <div class="flex flex-wrap gap-2">
                <Button size="sm" variant="primary" disabled={p.users.loading.value} onClick={() => p.fetchUsers(1)}>
                  <Search class="h-3.5 w-3.5" />
                  {t('telegramBot.broadcasts.searchUsers')}
                </Button>
                <Button size="sm" disabled={p.users.loading.value} onClick={p.resetFilters}>
                  <RotateCcw class="h-3.5 w-3.5" />
                  {t('telegramBot.broadcasts.resetFilters')}
                </Button>
                {p.selectedCount.value > 0 && (
                  <Button size="sm" variant="ghost" onClick={p.selection.clear}>
                    {t('telegramBot.zebra.clearSelection')}
                  </Button>
                )}
              </div>
              <div>
                <DataTable
                  columns={userColumns()}
                  rows={p.users.items.value}
                  rowKey={(r) => r.user_id}
                  loading={p.users.loading.value}
                  selection={toRowSelection(p.selection)}
                  emptyText={t('telegramBot.broadcasts.userEmpty')}
                  minWidth="920px"
                />
                <ListPagination pagination={p.users.pagination.value} onChangePage={p.users.changePage} onChangePageSize={p.users.changePageSize} />
              </div>
            </div>
          ),
        }}
      </Card>
    )

    return () => (
      <div class="space-y-6">
        <PageHeader title={t('telegramBot.broadcasts.createTitle')} subtitle={t('telegramBot.broadcasts.createSubtitle')}>
          {{
            actions: () => (
              <Button onClick={() => void router.push('/telegram-bot/broadcasts')}>
                <ArrowLeft class="h-4 w-4" />
                {t('telegramBot.broadcasts.backToList')}
              </Button>
            ),
          }}
        </PageHeader>

        <Card title={t('telegramBot.broadcasts.basicInfo')} description={t('telegramBot.broadcasts.basicInfoDesc')}>
          <div class="space-y-5">
            <FormField label={t('telegramBot.broadcasts.fieldTitle')} required>
              <Input v-model={p.form.title} placeholder={t('telegramBot.broadcasts.fieldTitlePlaceholder')} />
            </FormField>
            <FormField label={t('telegramBot.broadcasts.fieldRecipientType')} required>
              <RadioGroup
                variant="cards"
                modelValue={p.form.recipient_type}
                onUpdate:modelValue={(v) => (p.form.recipient_type = v as RecipientType)}
                options={[
                  { label: t('telegramBot.broadcasts.recipientTypeAll'), value: 'all', description: t('telegramBot.broadcasts.recipientTypeAllDesc') },
                  { label: t('telegramBot.broadcasts.recipientTypeSpecific'), value: 'specific', description: t('telegramBot.broadcasts.recipientTypeSpecificDesc') },
                ]}
              />
            </FormField>
            <FormField label={t('telegramBot.broadcasts.fieldAttachment')}>{attachmentBlock()}</FormField>
            <FormField label={t('telegramBot.broadcasts.fieldMessageHtml')} required hint={`${t('telegramBot.broadcasts.htmlHint')} ${t('telegramBot.zebra.richHint')}`}>
              <RichEditor v-model={p.form.message_html} placeholder={t('telegramBot.zebra.messagePlaceholder')} minHeight="220px" scene="telegram" />
            </FormField>
          </div>
        </Card>

        {p.form.recipient_type === 'specific' && userSelector()}

        <div class="flex justify-end">
          <Button variant="primary" size="lg" loading={p.submitting.value} disabled={p.uploading.value} onClick={() => void p.handleSubmit()}>
            {!p.submitting.value && <Send class="h-4 w-4" />}
            {t('telegramBot.broadcasts.submit')}
          </Button>
        </div>
      </div>
    )
  },
})
