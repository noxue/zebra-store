import { computed, nextTick, reactive, ref, watch } from 'vue'
import i18n from '@/i18n'
import { adminAPI } from '@/api/admin'
import { errorMessage } from '@/api/client'
import type { AdminGatewaySecurityTestResult, AdminMemberLevel } from '@/api/types'
import { getLocalizedText } from '@/utils/format'
import { notifySuccess } from '@/utils/notify'
import { ADMIN_BASE } from '@/utils/adminBase'
import {
  applyBepusdtOrderMode,
  applyDujiaopayOrderMode,
  applyEpusdtOrderMode,
  buildChannelPayload,
  channelOptionsFor,
  channelToState,
  channelTypeForProvider,
  configSectionFor,
  defaultChannelForm,
  defaultConfigs,
  interactionModeOptions,
  interactionModesFor,
  pickInteractionMode,
  showsChannelTypeSelect,
  type ChannelForm,
  type ProviderConfigs,
} from './paymentChannelRules'

export interface PaymentChannelModalOptions {
  isOpen: () => boolean
  channelId: () => number | null
  close: () => void
  onSuccess: () => void
}

/** State + provider rules for the create/edit payment channel dialog (original PaymentChannelModal.vue). */
export function usePaymentChannelModal(opts: PaymentChannelModalOptions) {
  const t = i18n.global.t
  const form = reactive<ChannelForm>(defaultChannelForm())
  const siteOrigin = typeof window === 'undefined' ? '' : window.location.origin
  const manualReviewBase = `${siteOrigin}${ADMIN_BASE}`
  const configs = reactive<ProviderConfigs>(defaultConfigs(siteOrigin, manualReviewBase))
  const error = ref('')
  const nameError = ref('')
  const loading = ref(false)
  const submitting = ref(false)
  const createdId = ref<number | null>(null)
  const manualKey = ref('')
  const customManualKey = ref('')
  const telegramBindUrl = ref('')
  const manualActionBusy = ref(false)
  const manualErrors = reactive<Record<string, string>>({})
  const showAdvanced = ref(false)
  /** While loading/resetting, the provider/order-mode side effects must not run. */
  let applying = false

  const effectiveId = computed(() => opts.channelId() ?? createdId.value)
  const isEditing = computed(() => effectiveId.value !== null)

  const setManualKey = async () => {
    const id = effectiveId.value
    if (!id) return
    manualActionBusy.value = true
    try {
      manualKey.value = (await adminAPI.setManualPaymentKey(id, customManualKey.value || undefined)).data?.key ?? ''
      customManualKey.value = ''
    } catch {
      /* The shared API client displays the failure. */
    } finally { manualActionBusy.value = false }
  }
  const testManualNotice = async () => {
    const id = effectiveId.value
    if (!id) return
    manualActionBusy.value = true
    try {
      await adminAPI.testManualPaymentNotice(id, `${configs.manual_qr.review_base_url.replace(/\/$/, '')}/manual-confirm/TEST`)
      notifySuccess(t('admin.common.operationSuccess'))
    } catch {
      /* The shared API client displays the failure. */
    } finally { manualActionBusy.value = false }
  }
  const beginTelegramBind = async () => {
    const id = effectiveId.value
    if (!id) return
    manualActionBusy.value = true
    try {
      telegramBindUrl.value = (await adminAPI.beginManualTelegramBind(id)).data?.url ?? ''
      delete manualErrors.telegram_token
    } catch (err) {
      manualErrors.telegram_token = errorMessage(err, t('admin.paymentChannels.manualQr.botLookupFailed'))
    }
    finally { manualActionBusy.value = false }
  }
  const finishTelegramBind = async () => {
    const id = effectiveId.value
    if (!id) return
    manualActionBusy.value = true
    try {
      const chats = (await adminAPI.finishManualTelegramBind(id)).data?.chat_ids ?? []
      if (chats.length > 0) { configs.manual_qr.telegram_chat_ids = chats; notifySuccess(t('admin.common.operationSuccess')) }
    } catch {
      /* The shared API client displays the failure. */
    } finally { manualActionBusy.value = false }
  }

  // ---- member levels ----
  const memberLevels = ref<AdminMemberLevel[]>([])
  const loadMemberLevels = async () => {
    try {
      memberLevels.value = (await adminAPI.getMemberLevels({ page: 1, page_size: 100 })).data ?? []
    } catch {
      memberLevels.value = []
    }
  }
  const memberLevelOptions = computed(() => memberLevels.value.map((ml) => ({ label: getLocalizedText(ml.name) || 'Unknown', value: ml.id })))
  const paymentTypeOptions = computed(() => [
    { label: t('admin.paymentChannels.paymentTypes.order'), value: 'order' },
    { label: t('admin.paymentChannels.paymentTypes.wallet'), value: 'wallet' },
  ])
  const paymentRoleOptions = computed(() => [
    { label: t('admin.paymentChannels.paymentRoles.guest'), value: 'guest' },
    { label: t('admin.paymentChannels.paymentRoles.member'), value: 'member' },
  ])

  // ---- provider rules ----
  const allowedModes = () =>
    interactionModesFor(form.provider_type, form.channel_type, { bepusdt: configs.bepusdt.order_mode, dujiaopay: configs.dujiaopay.order_mode })
  const interactionOptions = computed(() => interactionModeOptions(allowedModes()).map((o) => ({ label: t(o.labelKey), value: o.value })))
  const channelTypeOptions = computed(() => channelOptionsFor(form.provider_type).map((o) => ({ label: t(o.labelKey), value: o.value })))
  const showChannelSelect = computed(() => showsChannelTypeSelect(form.provider_type))
  const section = computed(() => configSectionFor(form.provider_type, form.channel_type))
  const syncInteractionMode = () => {
    form.interaction_mode = pickInteractionMode(form.interaction_mode, allowedModes())
  }

  // sync flush so the `applying` guard is effective while bulk-assigning state
  watch(
    () => form.provider_type,
    (provider) => {
      if (applying) return
      form.channel_type = channelTypeForProvider(provider, form.channel_type, configs.dujiaopay.order_mode)
      if (provider === 'manual_qr') form.payment_types = ['order']
      syncInteractionMode()
    },
    { flush: 'sync' },
  )
  watch(
    () => form.channel_type,
    () => {
      if (!applying) syncInteractionMode()
    },
    { flush: 'sync' },
  )
  watch(
    () => configs.bepusdt.order_mode,
    () => {
      if (applying || form.provider_type !== 'bepusdt') return
      applyBepusdtOrderMode(configs.bepusdt)
      form.channel_type = 'bepusdt'
      syncInteractionMode()
    },
    { flush: 'sync' },
  )
  watch(
    () => configs.epusdt.order_mode,
    () => {
      if (applying || form.provider_type !== 'epusdt') return
      applyEpusdtOrderMode(configs.epusdt)
    },
    { flush: 'sync' },
  )
  watch(
    () => configs.dujiaopay.order_mode,
    () => {
      if (applying || form.provider_type !== 'dujiaopay') return
      form.channel_type = applyDujiaopayOrderMode(configs.dujiaopay, form.channel_type)
      syncInteractionMode()
    },
    { flush: 'sync' },
  )

  const assignState = (nextForm: ChannelForm, nextConfigs: ProviderConfigs) => {
    applying = true
    try {
      Object.assign(form, nextForm)
      for (const key of Object.keys(nextConfigs) as (keyof ProviderConfigs)[]) Object.assign(configs[key], nextConfigs[key])
    } finally {
      applying = false
    }
  }

  // ---- wechat public key test ----
  const testingWechatPublicKey = ref(false)
  const wechatTestResult = ref<AdminGatewaySecurityTestResult | null>(null)
  const testWechatPublicKey = async () => {
    const id = opts.channelId()
    if (!id || testingWechatPublicKey.value) return
    error.value = ''
    wechatTestResult.value = null
    testingWechatPublicKey.value = true
    try {
      wechatTestResult.value = (await adminAPI.testWechatPayPublicKey(id)).data ?? null
    } catch (err) {
      error.value = errorMessage(err, t('admin.paymentChannels.modal.wechatPublicKeyTestFailed'))
    } finally {
      testingWechatPublicKey.value = false
    }
  }

  // ---- open / load ----
  const resetForCreate = () => {
    createdId.value = null
    manualKey.value = ''
    telegramBindUrl.value = ''
    Object.keys(manualErrors).forEach((key) => { delete manualErrors[key] })
    error.value = ''
    wechatTestResult.value = null
    showAdvanced.value = false
    assignState(defaultChannelForm(), defaultConfigs(siteOrigin, manualReviewBase))
  }

  let loadSeq = 0
  const loadChannel = async (id: number) => {
    const seq = ++loadSeq
    error.value = ''
    wechatTestResult.value = null
    showAdvanced.value = false
    loading.value = true
    try {
      const channel = (await adminAPI.getPaymentChannel(id)).data
      if (seq !== loadSeq || !channel) return
      const state = channelToState(channel, siteOrigin, manualReviewBase)
      assignState(state.form, state.configs)
      telegramBindUrl.value = ''
      Object.keys(manualErrors).forEach((key) => { delete manualErrors[key] })
      if (state.form.provider_type === 'manual_qr' && state.configs.manual_qr.notification_type === 'telegram' && state.configs.manual_qr.telegram_token) {
        await beginTelegramBind()
      }
    } catch (err) {
      if (seq === loadSeq) error.value = errorMessage(err, t('admin.paymentChannels.errors.fetchFailed'))
    } finally {
      if (seq === loadSeq) loading.value = false
    }
  }

  watch(
    () => [opts.isOpen(), opts.channelId()] as const,
    ([open, id], prev) => {
      if (!open) return
      const [wasOpen, prevId] = prev ?? [false, null]
      if (wasOpen && prevId === id) return
      if (id === null) resetForCreate()
      else void loadChannel(id)
    },
    { immediate: true },
  )

  // ---- submit ----
  const validateManual = () => {
    Object.keys(manualErrors).forEach((key) => { delete manualErrors[key] })
    const c = configs.manual_qr
    const required = t('admin.common.required')
    if (c.supported_channel_types.length === 0) manualErrors.supported_channel_types = required
    if (c.supported_channel_types.includes('alipay') && !c.alipay_qr_image_url.trim()) manualErrors.alipay_qr_image_url = required
    if (c.supported_channel_types.includes('wechat') && !c.wechat_qr_image_url.trim()) manualErrors.wechat_qr_image_url = required
    if (!/^https?:\/\/[^\s/]+/.test(c.review_base_url.trim())) manualErrors.review_base_url = t('admin.paymentChannels.manualQr.invalidUrl')
    if (!['telegram', 'wecom', 'dingtalk', 'email'].includes(c.notification_type)) manualErrors.notification_type = required
    if (c.notification_type === 'telegram') {
      if (!c.telegram_token.trim()) manualErrors.telegram_token = required
      else if (c.telegram_token !== '••••••••' && !/^\d+:[A-Za-z0-9_-]+$/.test(c.telegram_token.trim())) manualErrors.telegram_token = t('admin.paymentChannels.manualQr.invalidToken')
    }
    if (c.notification_type === 'wecom' && !/^https:\/\/qyapi\.weixin\.qq\.com\/cgi-bin\/webhook\/send\?key=.+/.test(c.wecom_webhook_url.trim())) manualErrors.wecom_webhook_url = t('admin.paymentChannels.manualQr.invalidWebhook')
    if (c.notification_type === 'dingtalk' && !/^https:\/\/oapi\.dingtalk\.com\/robot\/send\?access_token=.+/.test(c.dingtalk_webhook_url.trim())) manualErrors.dingtalk_webhook_url = t('admin.paymentChannels.manualQr.invalidWebhook')
    if (c.notification_type === 'email') {
      if (!c.smtp_host.trim()) manualErrors.smtp_host = required
      if (!/^\d+$/.test(c.smtp_port) || Number(c.smtp_port) < 1 || Number(c.smtp_port) > 65535) manualErrors.smtp_port = t('admin.paymentChannels.manualQr.invalidPort')
      if (!/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(c.smtp_from.trim())) manualErrors.smtp_from = t('admin.paymentChannels.manualQr.invalidEmail')
      if (!/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(c.smtp_to.trim())) manualErrors.smtp_to = t('admin.paymentChannels.manualQr.invalidEmail')
      if (c.smtp_tls && c.smtp_ssl) manualErrors.smtp_tls = t('admin.paymentChannels.manualQr.chooseTls')
    }
    return Object.keys(manualErrors).length === 0
  }
  const submit = async () => {
    error.value = ''
    nameError.value = ''
    if (!form.name.trim()) {
      nameError.value = t('admin.common.required')
      error.value = t('admin.paymentChannels.manualQr.fixFields')
      await nextTick()
      document.querySelector('[data-channel-name]')?.scrollIntoView({ behavior: 'smooth', block: 'center' })
      return
    }
    if (form.provider_type === 'manual_qr' && !validateManual()) {
      error.value = t('admin.paymentChannels.manualQr.fixFields')
      await nextTick()
      document.querySelector('[data-manual-error="true"]')?.scrollIntoView({ behavior: 'smooth', block: 'center' })
      return
    }
    const built = buildChannelPayload(form, configs)
    if (!built.ok) {
      error.value = t('admin.paymentChannels.errors.invalidConfig')
      return
    }
    submitting.value = true
    try {
      const id = effectiveId.value
      if (id !== null) await adminAPI.updatePaymentChannel(id, built.payload)
      else {
        const created = await adminAPI.createPaymentChannel(built.payload)
        if (form.provider_type === 'manual_qr' && created.data?.id) {
          createdId.value = created.data.id
          manualKey.value = (await adminAPI.setManualPaymentKey(created.data.id)).data?.key ?? ''
          if (configs.manual_qr.notification_type === 'telegram') await beginTelegramBind()
          opts.onSuccess()
          return
        }
      }
      if (form.provider_type === 'manual_qr' && configs.manual_qr.notification_type === 'telegram' && configs.manual_qr.telegram_token !== '••••••••') {
        await beginTelegramBind()
        opts.onSuccess()
        return
      }
      notifySuccess(t('admin.common.operationSuccess'))
      opts.close()
      opts.onSuccess()
    } catch {
      /* already notified by the API client */
    } finally {
      submitting.value = false
    }
  }

  return {
    form,
    configs,
    error,
    nameError,
    loading,
    submitting,
    manualActionBusy,
    manualErrors,
    manualKey,
    customManualKey,
    telegramBindUrl,
    effectiveId,
    setManualKey,
    testManualNotice,
    beginTelegramBind,
    finishTelegramBind,
    showAdvanced,
    isEditing,
    memberLevels,
    memberLevelOptions,
    paymentTypeOptions,
    paymentRoleOptions,
    interactionOptions,
    channelTypeOptions,
    showChannelSelect,
    section,
    testingWechatPublicKey,
    wechatTestResult,
    testWechatPublicKey,
    loadMemberLevels,
    submit,
  }
}

export type PaymentChannelModalState = ReturnType<typeof usePaymentChannelModal>
