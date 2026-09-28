import { ref } from 'vue'
import { adminAPI } from '@/api/admin'

/** Version / self-update info shown by the 检测更新 dialog. */
export interface SystemUpdateInfo {
  currentVersion: string
  latestVersion: string
  hasUpdate: boolean
  canUpdate: boolean
  /** Original `block_reason` (e.g. `source_build`); empty when updating is possible. */
  blockReason: string
}

const str = (v: unknown): string => (typeof v === 'string' ? v : v == null ? '' : String(v))
const obj = (v: unknown): Record<string, unknown> => (v && typeof v === 'object' ? (v as Record<string, unknown>) : {})

/**
 * Loads version + capability from the original endpoints
 * (`GET /admin/system/version/check`, `GET /admin/system/update/capability`).
 * `GET /admin/system/version` does not exist (live QA I-5).
 */
export async function loadSystemUpdateInfo(fallbackVersion = ''): Promise<SystemUpdateInfo> {
  const [check, cap] = await Promise.all([adminAPI.checkSystemUpdate(), adminAPI.getUpdateCapability()])
  const c = obj(check.data)
  const capability = obj(obj(cap.data).capability)
  const currentVersion = str(c.current_version) || fallbackVersion
  const canUpdate = capability.can_update === true
  return {
    currentVersion,
    latestVersion: str(c.latest_version) || currentVersion,
    hasUpdate: c.has_update === true,
    canUpdate,
    blockReason: canUpdate ? '' : str(capability.block_reason) || 'unsupported',
  }
}

export function useSystemUpdateInfo(fallbackVersion: () => string) {
  const loading = ref(false)
  const info = ref<SystemUpdateInfo | null>(null)
  const failed = ref(false)
  async function load() {
    loading.value = true
    failed.value = false
    try {
      info.value = await loadSystemUpdateInfo(fallbackVersion())
    } catch {
      info.value = null
      failed.value = true
    } finally {
      loading.value = false
    }
  }
  return { loading, info, failed, load }
}
