import { reactive, ref } from 'vue'
import { adminAPI } from '@/api/admin'
import { notifySuccess } from '@/utils/notify'
import { notifyFailure, tr } from './common'
import { asRecord, clampNumber } from './settingsUtils'

export const normalizeUpstreamSyncConfig = (raw: unknown) => {
  const d = asRecord(raw)
  return {
    interval_minutes: clampNumber(d.interval_minutes, 5, 1440, 5),
    pre_order_stock_check_enabled: d.pre_order_stock_check_enabled !== false,
    sync_page_size: clampNumber(d.sync_page_size, 10, 200, 50),
    sync_max_pages: clampNumber(d.sync_max_pages, 10, 500, 200),
    sync_conn_concurrency: clampNumber(d.sync_conn_concurrency, 1, 10, 3),
  }
}

/** 上游同步 (key `upstream_sync_config`). */
export function useUpstreamSyncSettings() {
  const submitting = ref(false)
  const form = reactive({
    interval_minutes: 5 as number | '',
    pre_order_stock_check_enabled: true,
    sync_page_size: 50 as number | '',
    sync_max_pages: 200 as number | '',
    sync_conn_concurrency: 3 as number | '',
  })

  const load = (raw: unknown) => {
    // 首次使用无数据时保持默认值
    if (raw && typeof raw === 'object' && Object.keys(raw).length > 0) Object.assign(form, normalizeUpstreamSyncConfig(raw))
  }

  const save = async () => {
    submitting.value = true
    try {
      const normalized = normalizeUpstreamSyncConfig(form)
      Object.assign(form, normalized)
      await adminAPI.updateSettings({ key: 'upstream_sync_config', value: normalized })
      notifySuccess(tr('admin.settings.alerts.saveSuccess'))
    } catch (err) {
      notifyFailure(err)
    } finally {
      submitting.value = false
    }
  }

  return { form, submitting, load, save }
}

export type UpstreamSyncSettingsModel = ReturnType<typeof useUpstreamSyncSettings>
