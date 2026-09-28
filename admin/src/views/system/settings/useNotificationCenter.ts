import { computed, reactive, ref } from 'vue'
import { adminAPI } from '@/api/admin'
import type { AdminNotificationLog, AdminProduct } from '@/api/types'
import { useListPage } from '@/composables/useListPage'
import { useListRefresh } from '@/composables/useListRefresh'
import { cleanParams, getLocalizedText, toRFC3339 } from '@/utils/format'
import { notifyError, notifySuccess } from '@/utils/notify'
import { notifyFailure, tr } from './common'
import {
  asRecord,
  asString,
  createSceneTemplate,
  joinLines,
  normalizeNumber,
  normalizeSceneTemplate,
  splitNumericIDs,
  splitRecipients,
  type LangCode,
  type SceneTemplate,
} from './settingsUtils'

export const NOTIFICATION_SCENES = ['wallet_recharge_success', 'order_paid_success', 'manual_fulfillment_pending', 'exception_alert'] as const
export type NotificationScene = (typeof NOTIFICATION_SCENES)[number]
export const NOTIFICATION_SCENE_LABEL_KEYS: Record<NotificationScene, string> = {
  wallet_recharge_success: 'admin.settings.notification.scenes.walletRechargeSuccess',
  order_paid_success: 'admin.settings.notification.scenes.orderPaidSuccess',
  manual_fulfillment_pending: 'admin.settings.notification.scenes.manualFulfillmentPending',
  exception_alert: 'admin.settings.notification.scenes.exceptionAlert',
}
export const NOTIFICATION_CHANNELS = ['email', 'telegram', 'feishu'] as const
export type NotificationChannel = (typeof NOTIFICATION_CHANNELS)[number]
export const FEISHU_RECEIVE_ID_TYPES = [
  ['chat_id', 'chatID'],
  ['open_id', 'openID'],
  ['user_id', 'userID'],
  ['union_id', 'unionID'],
  ['email', 'email'],
] as const

export const notificationSceneLabel = (value: string) => {
  const key = NOTIFICATION_SCENE_LABEL_KEYS[value as NotificationScene]
  return key ? tr(key) : value || '-'
}
export const notificationChannelLabel = (value: string) => {
  if (value === 'telegram') return tr('admin.settings.notification.channels.telegram.title')
  if (value === 'feishu') return tr('admin.settings.notification.channels.feishu.title')
  return tr('admin.settings.notification.channels.email.title')
}

export const buildProductLabel = (product: AdminProduct) => {
  const name = getLocalizedText(product.title || {})
  return name ? `#${product.id} ${name}` : `#${product.id}`
}

const createTemplates = () =>
  Object.fromEntries(NOTIFICATION_SCENES.map((s) => [s, createSceneTemplate()])) as Record<NotificationScene, SceneTemplate>

/** 通知中心: settings (`settings/notification-center`), test send, and send logs. */
export function useNotificationCenter() {
  const loading = ref(false)
  const submitting = ref(false)
  const testing = ref(false)
  const currentLang = ref<LangCode>('zh-CN')

  const form = reactive({
    default_locale: 'zh-CN',
    dedupe_ttl_seconds: 300 as number | '',
    inventory_alert_interval_seconds: 1800 as number | '',
    payment_order_alert_interval_seconds: 1800 as number | '',
    payment_order_alert_check_interval_seconds: 86400 as number | '',
    channels: {
      email: { enabled: false, recipients_text: '' },
      telegram: { enabled: false, recipients_text: '' },
      feishu: { enabled: false, app_id: '', app_secret: '', has_app_secret: false, receive_id_type: 'chat_id', recipients_text: '' },
    },
    scenes: {
      wallet_recharge_success: true,
      order_paid_success: true,
      manual_fulfillment_pending: true,
      exception_alert: true,
    } as Record<NotificationScene, boolean>,
    templates: createTemplates(),
  })

  // ---- ignored products picker ----
  const productKeyword = ref('')
  const productOptions = ref<AdminProduct[]>([])
  const productOptionsLoading = ref(false)
  const selectedIgnoredProduct = ref<string | number>('')
  const ignoredProducts = ref<{ id: number; label: string }[]>([])

  const resolveIgnoredProducts = async (ids: number[]) => {
    ignoredProducts.value = ids.map((id) => ({ id, label: `#${id}` }))
    if (ids.length === 0) return
    ignoredProducts.value = await Promise.all(
      ids.map(async (id) => {
        try {
          const product = (await adminAPI.getProduct(id)).data
          return { id, label: product ? buildProductLabel(product) : `#${id}` }
        } catch {
          return { id, label: `#${id}` }
        }
      }),
    )
  }

  const searchProducts = async () => {
    productOptionsLoading.value = true
    try {
      const res = await adminAPI.getProducts(cleanParams({ page: 1, page_size: 20, search: productKeyword.value }))
      productOptions.value = Array.isArray(res.data) ? res.data : []
    } catch (err) {
      notifyFailure(err, 'admin.settings.notification.inventory.searchFailed')
    } finally {
      productOptionsLoading.value = false
    }
  }

  const addIgnoredProduct = () => {
    const id = Number(selectedIgnoredProduct.value)
    if (!Number.isInteger(id) || id <= 0) {
      notifyError(tr('admin.settings.notification.inventory.productRequired'))
      return
    }
    if (!ignoredProducts.value.some((item) => item.id === id)) {
      const product = productOptions.value.find((item) => item.id === id)
      ignoredProducts.value = [...ignoredProducts.value, { id, label: product ? buildProductLabel(product) : `#${id}` }]
    }
    selectedIgnoredProduct.value = ''
  }

  const removeIgnoredProduct = (id: number) => {
    ignoredProducts.value = ignoredProducts.value.filter((item) => item.id !== id)
  }

  // ---- test send ----
  const testForm = reactive({ channel: 'email' as NotificationChannel, scene: 'order_paid_success' as NotificationScene, target: '' })

  const currentChannelTargets = computed(() => splitRecipients(form.channels[testForm.channel].recipients_text))

  const syncTestTarget = (force = false) => {
    if (!force && testForm.target.trim() !== '') return
    testForm.target = currentChannelTargets.value[0] || ''
  }

  // ---- load / save ----
  const load = (raw: unknown) => {
    const n = asRecord(raw)
    form.default_locale = asString(n.default_locale, 'zh-CN')
    form.dedupe_ttl_seconds = normalizeNumber(n.dedupe_ttl_seconds, 300)
    form.inventory_alert_interval_seconds = normalizeNumber(n.inventory_alert_interval_seconds, 1800)
    form.payment_order_alert_interval_seconds = normalizeNumber(n.payment_order_alert_interval_seconds ?? n.payment_failed_alert_interval_seconds, 1800)
    form.payment_order_alert_check_interval_seconds = normalizeNumber(n.payment_order_alert_check_interval_seconds, 86400)
    const channels = asRecord(n.channels)
    const email = asRecord(channels.email)
    const telegram = asRecord(channels.telegram)
    const feishu = asRecord(channels.feishu)
    form.channels.email.enabled = !!email.enabled
    form.channels.email.recipients_text = joinLines(email.recipients)
    form.channels.telegram.enabled = !!telegram.enabled
    form.channels.telegram.recipients_text = joinLines(telegram.recipients)
    form.channels.feishu.enabled = !!feishu.enabled
    form.channels.feishu.app_id = asString(feishu.app_id)
    form.channels.feishu.app_secret = ''
    form.channels.feishu.has_app_secret = !!feishu.has_app_secret
    form.channels.feishu.receive_id_type = asString(feishu.receive_id_type, 'chat_id')
    form.channels.feishu.recipients_text = joinLines(feishu.recipients)
    const scenes = asRecord(n.scenes)
    const templates = asRecord(n.templates)
    NOTIFICATION_SCENES.forEach((s) => {
      form.scenes[s] = !!scenes[s]
      form.templates[s] = normalizeSceneTemplate(templates[s])
    })
    syncTestTarget(true)
    void resolveIgnoredProducts(splitNumericIDs(joinLines(n.ignored_product_ids)))
  }

  const fetchSettings = async () => {
    loading.value = true
    try {
      load((await adminAPI.getNotificationCenterSettings()).data)
    } catch {
      /* already notified */
    } finally {
      loading.value = false
    }
  }

  const save = async () => {
    submitting.value = true
    try {
      await adminAPI.updateNotificationCenterSettings({
        default_locale: form.default_locale,
        dedupe_ttl_seconds: Number(form.dedupe_ttl_seconds),
        inventory_alert_interval_seconds: Number(form.inventory_alert_interval_seconds),
        payment_order_alert_interval_seconds: Number(form.payment_order_alert_interval_seconds),
        payment_order_alert_check_interval_seconds: Number(form.payment_order_alert_check_interval_seconds),
        ignored_product_ids: ignoredProducts.value.map((p) => p.id),
        channels: {
          email: { enabled: form.channels.email.enabled, recipients: splitRecipients(form.channels.email.recipients_text) },
          telegram: { enabled: form.channels.telegram.enabled, recipients: splitRecipients(form.channels.telegram.recipients_text) },
          feishu: {
            enabled: form.channels.feishu.enabled,
            app_id: form.channels.feishu.app_id.trim(),
            app_secret: form.channels.feishu.app_secret.trim(),
            receive_id_type: form.channels.feishu.receive_id_type,
            recipients: splitRecipients(form.channels.feishu.recipients_text),
          },
        },
        scenes: { ...form.scenes },
        templates: form.templates,
      })
      notifySuccess(tr('admin.settings.alerts.saveSuccess'))
      await fetchSettings()
    } catch (err) {
      notifyFailure(err)
    } finally {
      submitting.value = false
    }
  }

  // ---- logs ----
  const logFilters = reactive({
    channel: '__all__',
    status: '__all__',
    eventType: '__all__',
    isTest: '__all__',
    createdFrom: '',
    createdTo: '',
  })

  const logs = useListPage<AdminNotificationLog>({
    pageSize: 10,
    fetchFn: (page, pageSize) =>
      adminAPI.listNotificationLogs(
        cleanParams({
          page,
          page_size: pageSize,
          channel: logFilters.channel,
          status: logFilters.status,
          event_type: logFilters.eventType,
          is_test: logFilters.isTest === '__all__' ? undefined : logFilters.isTest === 'true',
          created_from: toRFC3339(logFilters.createdFrom),
          created_to: toRFC3339(logFilters.createdTo),
        }),
      ),
  })
  const { refreshing, refreshList } = useListRefresh()
  const refreshLogs = () => refreshList(logs.refresh)

  const resetLogFilters = () => {
    Object.assign(logFilters, { channel: '__all__', status: '__all__', eventType: '__all__', isTest: '__all__', createdFrom: '', createdTo: '' })
    void logs.fetchData(1)
  }

  const sendTest = async () => {
    if (testForm.target.trim() === '') {
      notifyError(tr('admin.settings.notification.test.targetRequired'))
      return
    }
    testing.value = true
    try {
      await adminAPI.testNotificationCenterSettings({
        channel: testForm.channel,
        target: testForm.target.trim(),
        scene: testForm.scene,
        locale: currentLang.value,
      })
      notifySuccess(tr('admin.settings.notification.test.success'))
      await logs.fetchData(1)
    } catch (err) {
      notifyFailure(err, 'admin.settings.notification.test.failed')
      // a failed test is still recorded in the send history
      void logs.fetchData(1)
    } finally {
      testing.value = false
    }
  }

  const init = async () => {
    await fetchSettings()
    await logs.fetchData(1)
  }

  return {
    loading,
    submitting,
    testing,
    currentLang,
    form,
    productKeyword,
    productOptions,
    productOptionsLoading,
    selectedIgnoredProduct,
    ignoredProducts,
    searchProducts,
    addIgnoredProduct,
    removeIgnoredProduct,
    testForm,
    currentChannelTargets,
    syncTestTarget,
    sendTest,
    logFilters,
    logs,
    refreshing,
    refreshLogs,
    resetLogFilters,
    fetchSettings,
    save,
    init,
  }
}

export type NotificationCenterModel = ReturnType<typeof useNotificationCenter>
