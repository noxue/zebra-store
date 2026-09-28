import { ref } from 'vue'
import { defineStore } from 'pinia'

export type NoticeType = 'error' | 'success' | 'info'

export interface NoticeItem {
  id: number
  message: string
  type: NoticeType
}

export const simplifyNoticeMessage = (input: string) => {
  let value = String(input || '').trim()
  if (!value) return ''
  value = value.replace(
    /^(操作失败|請求失敗|请求失败|提交失败|保存失败|删除失败|更新失败|创建失败|上傳失敗|上传失败|operation failed|request failed)\s*[:：-]\s*/i,
    '',
  )
  return value.toLowerCase().replace(/[\s`~!@#$%^&*()_+\-=[\]{};':"\\|,.<>/?，。！？；：、“”‘’（）【】《》·…—]/g, '')
}

export const useNoticeStore = defineStore('admin-notice', () => {
  const items = ref<NoticeItem[]>([])
  const timers = new Map<number, number>()
  let sequence = 0

  const remove = (id: number) => {
    const timer = timers.get(id)
    if (timer !== undefined) {
      window.clearTimeout(timer)
      timers.delete(id)
    }
    items.value = items.value.filter((item) => item.id !== id)
  }

  const hasSimilar = (kind: NoticeType, message: string) => {
    const current = simplifyNoticeMessage(message)
    if (!current) return false
    return items.value.some((item) => {
      if (item.type !== kind) return false
      const existing = simplifyNoticeMessage(item.message)
      return !!existing && (existing === current || existing.includes(current) || current.includes(existing))
    })
  }

  const show = (msg: string, kind: NoticeType = 'error', duration = 5000) => {
    const message = msg?.trim()
    if (!message || hasSimilar(kind, message)) return undefined
    const id = ++sequence
    items.value.push({ id, message, type: kind })
    if (duration > 0) timers.set(id, window.setTimeout(() => remove(id), duration))
    if (items.value.length > 5) items.value.slice(0, items.value.length - 5).forEach((item) => remove(item.id))
    return id
  }

  const clearAll = () => {
    timers.forEach((timer) => window.clearTimeout(timer))
    timers.clear()
    items.value = []
  }

  return { items, show, remove, clearAll }
})
