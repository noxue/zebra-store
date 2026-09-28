import { reactive, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { adminAPI } from '@/api/admin'
import { copyText } from '@/utils/clipboard'
import { confirmAction } from '@/utils/confirm'
import { notifySuccess } from '@/utils/notify'

/** Bot channel client (the API types it as a plain JSON object). */
export interface ChannelClient {
  id: number
  name: string
  channel_type: string
  channel_key: string
  channel_secret: string
  bot_token_set: boolean
  callback_url: string
  status: number
  description: string
  last_used_at: string | null
  created_at: string
}

const s = (v: unknown) => (typeof v === 'string' ? v : '')

export const parseChannelClient = (raw: Record<string, unknown>): ChannelClient => ({
  id: Number(raw.id) || 0,
  name: s(raw.name),
  channel_type: s(raw.channel_type),
  channel_key: s(raw.channel_key),
  channel_secret: s(raw.channel_secret),
  bot_token_set: raw.bot_token_set === true,
  callback_url: s(raw.callback_url),
  status: Number(raw.status) || 0,
  description: s(raw.description),
  last_used_at: typeof raw.last_used_at === 'string' ? raw.last_used_at : null,
  created_at: s(raw.created_at),
})

/** Build the update payload: bot_token is only sent when the user typed one (empty = keep). */
export const buildChannelClientUpdate = (form: { name: string; description: string; bot_token: string; callback_url: string }) => {
  const data: { name: string; description: string; callback_url: string; bot_token?: string } = {
    name: form.name,
    description: form.description,
    callback_url: form.callback_url,
  }
  if (form.bot_token !== '') data.bot_token = form.bot_token
  return data
}

const emptyCreateForm = () => ({ name: '', channel_type: 'telegram_bot', description: '', bot_token: '', callback_url: '' })

export function useChannelClients() {
  const { t } = useI18n()
  const loading = ref(false)
  const clients = ref<ChannelClient[]>([])

  const showCreateDialog = ref(false)
  const creating = ref(false)
  const createForm = reactive(emptyCreateForm())

  const showEditDialog = ref(false)
  const editing = ref(false)
  const editingClient = ref<ChannelClient | null>(null)
  const editForm = reactive({ name: '', description: '', bot_token: '', callback_url: '' })

  const fetchClients = async () => {
    loading.value = true
    try {
      const res = await adminAPI.getChannelClients()
      clients.value = Array.isArray(res.data) ? res.data.map(parseChannelClient) : []
    } catch {
      clients.value = []
    } finally {
      loading.value = false
    }
  }

  const openCreate = () => {
    Object.assign(createForm, emptyCreateForm())
    showCreateDialog.value = true
  }

  const handleCreate = async () => {
    if (!createForm.name) return
    creating.value = true
    try {
      await adminAPI.createChannelClient({ ...createForm })
      notifySuccess(t('telegramBot.channelClients.createSuccess'))
      showCreateDialog.value = false
      Object.assign(createForm, emptyCreateForm())
      void fetchClients()
    } catch {
      /* already notified */
    } finally {
      creating.value = false
    }
  }

  const openEditDialog = (client: ChannelClient) => {
    editingClient.value = client
    Object.assign(editForm, { name: client.name, description: client.description || '', bot_token: '', callback_url: client.callback_url || '' })
    showEditDialog.value = true
  }

  const handleEdit = async () => {
    if (!editingClient.value || !editForm.name) return
    editing.value = true
    try {
      await adminAPI.updateChannelClient(editingClient.value.id, buildChannelClientUpdate(editForm))
      notifySuccess(t('telegramBot.channelClients.editSuccess'))
      showEditDialog.value = false
      void fetchClients()
    } catch {
      /* already notified */
    } finally {
      editing.value = false
    }
  }

  const handleToggleStatus = async (client: ChannelClient) => {
    try {
      await adminAPI.updateChannelClientStatus(client.id, { status: client.status === 1 ? 0 : 1 })
      notifySuccess(t('telegramBot.channelClients.statusUpdated'))
      void fetchClients()
    } catch {
      /* already notified */
    }
  }

  const handleResetSecret = async (client: ChannelClient) => {
    const ok = await confirmAction({
      title: t('telegramBot.channelClients.resetSecretTitle'),
      description: t('telegramBot.channelClients.resetSecretDesc'),
      confirmText: t('telegramBot.channelClients.confirm'),
      cancelText: t('telegramBot.channelClients.cancel'),
    })
    if (!ok) return
    try {
      await adminAPI.resetChannelClientSecret(client.id)
      notifySuccess(t('telegramBot.channelClients.resetSecretSuccess'))
      void fetchClients()
    } catch {
      /* already notified */
    }
  }

  const handleDelete = async (client: ChannelClient) => {
    const ok = await confirmAction({
      title: t('telegramBot.channelClients.deleteTitle'),
      description: t('telegramBot.channelClients.deleteDesc'),
      confirmText: t('telegramBot.channelClients.confirm'),
      cancelText: t('telegramBot.channelClients.cancel'),
      variant: 'destructive',
    })
    if (!ok) return
    try {
      await adminAPI.deleteChannelClient(client.id)
      notifySuccess(t('telegramBot.channelClients.deleteSuccess'))
      void fetchClients()
    } catch {
      /* already notified */
    }
  }

  const copyToClipboard = async (text: string) => {
    try {
      await copyText(text)
      notifySuccess(t('telegramBot.channelClients.copied'))
    } catch {
      /* clipboard unavailable */
    }
  }

  return {
    loading,
    clients,
    fetchClients,
    showCreateDialog,
    creating,
    createForm,
    openCreate,
    handleCreate,
    showEditDialog,
    editing,
    editingClient,
    editForm,
    openEditDialog,
    handleEdit,
    handleToggleStatus,
    handleResetSecret,
    handleDelete,
    copyToClipboard,
  }
}
