import { computed, reactive, ref, watch } from 'vue'
import i18n from '@/i18n'
import { adminAPI } from '@/api/admin'
import { errorMessage } from '@/api/client'
import type { AdminGatewaySecurityTestResult, AdminMemberLevel } from '@/api/types'
import { getLocalizedText } from '@/utils/format'
import { notifySuccess } from '@/utils/notify'
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
  const configs = reactive<ProviderConfigs>(defaultConfigs(siteOrigin))
  const error = ref('')
  const loading = ref(false)
  const submitting = ref(false)
  const showAdvanced = ref(false)
  /** While loading/resetting, the provider/order-mode side effects must not run. */
  let applying = false

  const isEditing = computed(() => opts.channelId() !== null)

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
    error.value = ''
    wechatTestResult.value = null
    showAdvanced.value = false
    assignState(defaultChannelForm(), defaultConfigs(siteOrigin))
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
      const state = channelToState(channel)
      assignState(state.form, state.configs)
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
  const submit = async () => {
    error.value = ''
    if (!form.name.trim()) {
      error.value = `${t('admin.paymentChannels.modal.name')}: ${t('admin.common.required')}`
      return
    }
    const built = buildChannelPayload(form, configs)
    if (!built.ok) {
      error.value = t('admin.paymentChannels.errors.invalidConfig')
      return
    }
    submitting.value = true
    try {
      const id = opts.channelId()
      if (id !== null) await adminAPI.updatePaymentChannel(id, built.payload)
      else await adminAPI.createPaymentChannel(built.payload)
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
    loading,
    submitting,
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
