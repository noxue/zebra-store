import { computed, reactive, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { adminAPI } from '@/api/admin'
import { notifySuccess } from '@/utils/notify'
import { buildRiskControlPayload, defaultRiskControlForm, parseRiskControlConfig, recommendedGuestPolicy } from './riskControlUtils'

const SETTINGS_KEY = 'order_risk_control_config'

export function useOrderRiskControl() {
  const { t } = useI18n()
  const loading = ref(false)
  const saving = ref(false)
  const form = reactive(defaultRiskControlForm())

  const load = async () => {
    loading.value = true
    try {
      const res = await adminAPI.getSettings({ key: SETTINGS_KEY })
      Object.assign(form, parseRiskControlConfig(res.data))
    } catch {
      // keep the safe recommended defaults; saving stays an explicit user action
    } finally {
      loading.value = false
    }
  }

  const save = async () => {
    saving.value = true
    try {
      await adminAPI.updateSettings({ key: SETTINGS_KEY, value: buildRiskControlPayload(form) })
      notifySuccess(t('admin.settings.alerts.saveSuccess'))
    } catch {
      /* already notified */
    } finally {
      saving.value = false
    }
  }

  const applyRecommendedGuestPolicy = () => {
    form.guest = recommendedGuestPolicy()
  }

  const limitText = (v: number | '') => (Number(v) > 0 ? String(v) : t('admin.settings.orderRiskControl.noLimit'))
  const guestSummary = computed(() =>
    t('admin.settings.orderRiskControl.guest.summary', {
      orders: limitText(form.guest.max_pending_orders_per_ip),
      quantity: limitText(form.guest.max_quantity_per_product_per_order),
      pending: limitText(form.guest.max_pending_quantity_per_ip_product),
    }),
  )

  return { loading, saving, form, load, save, applyRecommendedGuestPolicy, guestSummary }
}
