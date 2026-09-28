import { reactive, ref } from 'vue'
import i18n from '@/i18n'
import { adminAPI } from '@/api/admin'
import {
  CALLBACK_ROUTE_KEYS,
  buildCallbackRoutesSavePayload,
  getCallbackRouteDisplayValue,
  toCallbackRouteSaveValue,
  validateCallbackRoutes,
  type CallbackRouteKey,
  type CallbackRoutesValue,
} from '@/utils/callbackRoutes'
import { notifyError, notifySuccess } from '@/utils/notify'

/** Page logic for 回调路由 (settings key callback_routes_config). Empty value = default path. */
export function useCallbackRoutes() {
  const t = i18n.global.t
  const form = reactive<CallbackRoutesValue>({
    payment_callback: '',
    dujiaopay_webhook: '',
    paypal_webhook: '',
    stripe_webhook: '',
    upstream_callback: '',
  })
  const loading = ref(false)
  const saving = ref(false)

  const load = async () => {
    loading.value = true
    try {
      const data = (await adminAPI.getSettings<Partial<Record<CallbackRouteKey, string>> | null>({ key: 'callback_routes_config' })).data
      if (data) for (const key of CALLBACK_ROUTE_KEYS) form[key] = typeof data[key] === 'string' ? data[key] : ''
    } catch {
      /* not configured yet: keep defaults */
    } finally {
      loading.value = false
    }
  }

  const displayValue = (key: CallbackRouteKey) => getCallbackRouteDisplayValue(key, form[key])
  const setValue = (key: CallbackRouteKey, value: string | number) => {
    form[key] = toCallbackRouteSaveValue(key, value)
  }

  const save = async () => {
    const payload = buildCallbackRoutesSavePayload(form)
    const invalid = validateCallbackRoutes(payload)
    if (invalid) {
      notifyError(t(`admin.settings.callbackRoutes.${invalid}`))
      return
    }
    saving.value = true
    try {
      await adminAPI.updateSettings({ key: 'callback_routes_config', value: payload })
      notifySuccess(t('admin.settings.saved'))
    } catch {
      /* already notified */
    } finally {
      saving.value = false
    }
  }

  return { form, loading, saving, load, displayValue, setValue, save }
}
