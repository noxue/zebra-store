import { onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { userProfileAPI } from '@/api/user'
import type { UserLoginLogItem } from '@/api/types'
import type { BadgeTone } from '@/utils/status'
import { usePagedList } from './usePagedList'

/** Paginated login history for the security panel. */
export function useLoginHistory(pageSize = 10) {
  const { t, te } = useI18n()
  const logs = usePagedList<UserLoginLogItem>((p) => userProfileAPI.loginLogs(p), pageSize)
  const statusLabel = (s?: string) => {
    const k = `personalCenter.security.loginLogsStatusMap.${s || ''}`
    return s && te(k) ? t(k) : s || '-'
  }
  const statusTone = (s?: string): BadgeTone => (s === 'success' ? 'success' : s === 'failed' ? 'danger' : 'neutral')
  const reasonLabel = (r?: string) => {
    if (!r) return '-'
    const k = `personalCenter.security.loginLogsReasonMap.${r}`
    return te(k) ? t(k) : r
  }
  onMounted(() => void logs.load(1))
  return { logs, statusLabel, statusTone, reasonLabel }
}
