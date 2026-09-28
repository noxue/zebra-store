import i18n from '@/i18n'
import { useNoticeStore, type NoticeType, simplifyNoticeMessage } from '@/stores/notice'

const t = (key: string) => i18n.global.t(key)

const dedupeWindowMs = 1500
let last = { type: '' as NoticeType | '', core: '', at: 0 }

const shouldSkip = (type: NoticeType, content: string) => {
  const core = simplifyNoticeMessage(content)
  const now = Date.now()
  const dup =
    last.type === type &&
    now - last.at <= dedupeWindowMs &&
    core !== '' &&
    last.core !== '' &&
    (core === last.core || core.includes(last.core) || last.core.includes(core))
  last = { type, core, at: now }
  return dup
}

const push = (message: string | undefined, type: NoticeType, fallbackKey: string) => {
  try {
    const content = message && message.trim() ? message : t(fallbackKey)
    if (shouldSkip(type, content)) return
    useNoticeStore().show(content, type)
  } catch {
    /* pinia not ready */
  }
}

export const notifyError = (message?: string) => push(message, 'error', 'common.api.requestFailed')
export const notifySuccess = (message?: string) => push(message, 'success', 'admin.common.operationSuccess')
export const notifyInfo = (message?: string) => push(message, 'info', 'admin.common.notice')
