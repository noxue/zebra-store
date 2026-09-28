import { defineComponent, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { Copy, Pencil, Plus, RotateCcw, Trash2 } from 'lucide-vue-next'
import { Badge, Button, DataTable, Dialog, FormField, IdCell, Input, PageHeader, Textarea, type DataTableColumn } from '@/components/ui'
import { useChannelClients, type ChannelClient } from './useChannelClients'

export default defineComponent({
  name: 'TelegramBotChannelClients',
  setup() {
    const { t } = useI18n()
    const p = useChannelClients()
    onMounted(() => void p.fetchClients())

    const copyable = (value: string, maxW = 'max-w-[180px]') => (
      <div class="flex items-center gap-1">
        <code class={['block break-all rounded-zs-sm bg-surface-muted px-1.5 py-0.5 font-mono text-xs text-fg', maxW]}>{value}</code>
        <Button size="icon-sm" variant="ghost" title={t('admin.common.copy')} onClick={() => void p.copyToClipboard(value)}>
          <Copy class="h-3 w-3" />
        </Button>
      </div>
    )

    const columns = (): DataTableColumn<ChannelClient>[] => [
      { key: 'id', title: 'ID', render: (r) => <IdCell value={r.id} /> },
      {
        key: 'name',
        title: t('telegramBot.channelClients.name'),
        class: 'min-w-[160px]',
        render: (r) => (
          <div>
            <div class="break-words font-medium">{r.name}</div>
            {r.description && <div class="break-words text-xs text-muted">{r.description}</div>}
          </div>
        ),
      },
      { key: 'channelKey', title: t('telegramBot.channelClients.channelKey'), class: 'min-w-[160px]', render: (r) => copyable(r.channel_key) },
      { key: 'channelSecret', title: t('telegramBot.channelClients.channelSecret'), class: 'min-w-[160px]', render: (r) => copyable(r.channel_secret) },
      {
        key: 'botToken',
        title: t('telegramBot.channelClients.botToken'),
        class: 'min-w-[100px]',
        render: (r) =>
          r.bot_token_set ? (
            <Badge tone="success">{t('telegramBot.channelClients.botTokenSet')}</Badge>
          ) : (
            <span class="text-xs text-muted">{t('telegramBot.channelClients.botTokenNotSet')}</span>
          ),
      },
      {
        key: 'callbackUrl',
        title: t('telegramBot.channelClients.callbackUrl'),
        class: 'min-w-[180px]',
        render: (r) =>
          r.callback_url ? copyable(r.callback_url, 'max-w-[220px]') : <span class="text-xs text-muted">{t('telegramBot.channelClients.callbackUrlNotSet')}</span>,
      },
      {
        key: 'status',
        title: t('telegramBot.channelClients.statusLabel'),
        render: (r) => (
          <Badge tone={r.status === 1 ? 'success' : 'neutral'} dot>
            {r.status === 1 ? t('telegramBot.channelClients.active') : t('telegramBot.channelClients.disabled')}
          </Badge>
        ),
      },
      {
        key: 'actions',
        title: t('telegramBot.channelClients.actions'),
        class: 'min-w-[230px]',
        render: (r) => (
          <div class="flex items-center gap-1 whitespace-nowrap">
            <Button size="sm" title={t('telegramBot.channelClients.edit')} onClick={() => p.openEditDialog(r)}>
              <Pencil class="h-3.5 w-3.5" />
            </Button>
            <Button size="sm" onClick={() => void p.handleToggleStatus(r)}>
              {r.status === 1 ? t('telegramBot.channelClients.disable') : t('telegramBot.channelClients.enable')}
            </Button>
            <Button size="sm" title={t('telegramBot.channelClients.resetSecret')} onClick={() => void p.handleResetSecret(r)}>
              <RotateCcw class="h-3.5 w-3.5" />
            </Button>
            <Button size="sm" variant="danger" title={t('telegramBot.channelClients.delete')} onClick={() => void p.handleDelete(r)}>
              <Trash2 class="h-3.5 w-3.5" />
            </Button>
          </div>
        ),
      },
    ]

    return () => (
      <div class="space-y-6">
        <PageHeader title={t('telegramBot.channelClients.title')} subtitle={t('telegramBot.channelClients.subtitle')}>
          {{
            actions: () => (
              <Button variant="primary" onClick={p.openCreate}>
                <Plus class="h-4 w-4" />
                {t('telegramBot.channelClients.create')}
              </Button>
            ),
          }}
        </PageHeader>

        <DataTable
          columns={columns()}
          rows={p.clients.value}
          rowKey={(r) => r.id}
          loading={p.loading.value}
          emptyText={t('telegramBot.channelClients.empty')}
          minWidth="1020px"
        />

        <Dialog v-model={p.showCreateDialog.value} title={t('telegramBot.channelClients.createTitle')} description={t('telegramBot.channelClients.createDesc')} size="lg">
          {{
            default: () => (
              <div class="space-y-4">
                <FormField label={t('telegramBot.channelClients.name')} required>
                  <Input v-model={p.createForm.name} placeholder={t('telegramBot.channelClients.namePlaceholder')} />
                </FormField>
                <FormField label={t('telegramBot.channelClients.botToken')} hint={t('telegramBot.channelClients.botTokenHint')}>
                  <Input v-model={p.createForm.bot_token} placeholder={t('telegramBot.channelClients.botTokenPlaceholder')} mono />
                </FormField>
                <FormField label={t('telegramBot.channelClients.callbackUrl')} hint={t('telegramBot.channelClients.callbackUrlHint')}>
                  <Input v-model={p.createForm.callback_url} placeholder={t('telegramBot.channelClients.callbackUrlPlaceholder')} />
                </FormField>
                <FormField label={t('telegramBot.channelClients.description')}>
                  <Textarea v-model={p.createForm.description} rows={2} placeholder={t('telegramBot.channelClients.descriptionPlaceholder')} />
                </FormField>
              </div>
            ),
            footer: () => (
              <>
                <Button onClick={() => (p.showCreateDialog.value = false)}>{t('telegramBot.channelClients.cancel')}</Button>
                <Button variant="primary" loading={p.creating.value} disabled={!p.createForm.name} onClick={() => void p.handleCreate()}>
                  {t('telegramBot.channelClients.create')}
                </Button>
              </>
            ),
          }}
        </Dialog>

        <Dialog v-model={p.showEditDialog.value} title={t('telegramBot.channelClients.editTitle')} description={t('telegramBot.channelClients.editDesc')} size="lg">
          {{
            default: () => (
              <div class="space-y-4">
                <FormField label={t('telegramBot.channelClients.name')} required>
                  <Input v-model={p.editForm.name} placeholder={t('telegramBot.channelClients.namePlaceholder')} />
                </FormField>
                <FormField
                  label={t('telegramBot.channelClients.botToken')}
                  hint={p.editingClient.value?.bot_token_set ? t('telegramBot.channelClients.botTokenCurrentSet') : t('telegramBot.channelClients.botTokenCurrentNotSet')}
                >
                  <Input v-model={p.editForm.bot_token} placeholder={t('telegramBot.channelClients.botTokenEditPlaceholder')} mono />
                </FormField>
                <FormField label={t('telegramBot.channelClients.callbackUrl')} hint={t('telegramBot.channelClients.callbackUrlHint')}>
                  <Input v-model={p.editForm.callback_url} placeholder={t('telegramBot.channelClients.callbackUrlPlaceholder')} />
                </FormField>
                <FormField label={t('telegramBot.channelClients.description')}>
                  <Textarea v-model={p.editForm.description} rows={2} placeholder={t('telegramBot.channelClients.descriptionPlaceholder')} />
                </FormField>
              </div>
            ),
            footer: () => (
              <>
                <Button onClick={() => (p.showEditDialog.value = false)}>{t('telegramBot.channelClients.cancel')}</Button>
                <Button variant="primary" loading={p.editing.value} disabled={!p.editForm.name} onClick={() => void p.handleEdit()}>
                  {t('telegramBot.channelClients.save')}
                </Button>
              </>
            ),
          }}
        </Dialog>
      </div>
    )
  },
})
