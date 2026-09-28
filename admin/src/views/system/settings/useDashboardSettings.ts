import { reactive, ref } from 'vue'
import { adminAPI } from '@/api/admin'
import { notifySuccess } from '@/utils/notify'
import { notifyFailure, tr } from './common'
import { asRecord, clampNumber } from './settingsUtils'

type Num = number | ''

/** Clamp ranges from the original Settings.vue (dashboard_config). */
export const normalizeDashboardConfig = (raw: unknown) => {
  const d = asRecord(raw)
  const accounting = asRecord(d.accounting)
  const alert = asRecord(d.alert)
  const ranking = asRecord(d.ranking)
  return {
    accounting: { refund_reverses_cost: accounting.refund_reverses_cost === true },
    alert: {
      low_stock_threshold: clampNumber(alert.low_stock_threshold, 1, 500, 5),
      out_of_stock_products_threshold: clampNumber(alert.out_of_stock_products_threshold, 1, 10000, 1),
      pending_payment_orders_threshold: clampNumber(alert.pending_payment_orders_threshold, 1, 100000, 20),
      payments_failed_threshold: clampNumber(alert.payments_failed_threshold, 1, 100000, 10),
    },
    ranking: {
      top_products_limit: clampNumber(ranking.top_products_limit, 1, 20, 5),
      top_channels_limit: clampNumber(ranking.top_channels_limit, 1, 20, 5),
    },
  }
}

/** 仪表盘配置 (key `dashboard_config`). */
export function useDashboardSettings() {
  const submitting = ref(false)
  const form = reactive({
    accounting: { refund_reverses_cost: false },
    alert: {
      low_stock_threshold: 5 as Num,
      out_of_stock_products_threshold: 1 as Num,
      pending_payment_orders_threshold: 20 as Num,
      payments_failed_threshold: 10 as Num,
    },
    ranking: { top_products_limit: 5 as Num, top_channels_limit: 5 as Num },
  })

  const load = (raw: unknown) => {
    const n = normalizeDashboardConfig(raw)
    form.accounting.refund_reverses_cost = n.accounting.refund_reverses_cost
    Object.assign(form.alert, n.alert)
    Object.assign(form.ranking, n.ranking)
  }

  const save = async () => {
    submitting.value = true
    try {
      const normalized = normalizeDashboardConfig(form)
      load(normalized)
      await adminAPI.updateSettings({ key: 'dashboard_config', value: normalized })
      notifySuccess(tr('admin.settings.alerts.saveSuccess'))
    } catch (err) {
      notifyFailure(err)
    } finally {
      submitting.value = false
    }
  }

  return { form, submitting, load, save }
}

export type DashboardSettingsModel = ReturnType<typeof useDashboardSettings>
