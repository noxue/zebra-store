import type { AnnouncementConfig } from '@/api/types'

const DISMISS_KEY = 'announcement_dismiss'
const SESSION_KEY = 'announcement_closed'

interface DismissRecord {
  version: string
  mode: 'today' | 'forever'
  date?: string
}

/** Browser-local `YYYY-MM-DD`. */
export const todayStr = (now: Date = new Date()): string => {
  const pad = (n: number) => String(n).padStart(2, '0')
  return `${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}`
}

const versionOf = (a: AnnouncementConfig | null | undefined): string =>
  a && a.version !== undefined && a.version !== null ? String(a.version) : ''

const readDismiss = (): DismissRecord | null => {
  try {
    const raw = localStorage.getItem(DISMISS_KEY)
    if (!raw) return null
    const parsed: unknown = JSON.parse(raw)
    if (parsed && typeof parsed === 'object' && typeof (parsed as DismissRecord).version === 'string') return parsed as DismissRecord
  } catch {
    // invalid record = none
  }
  return null
}

/** Announcement dismissal (session / today / forever), keyed by version. */
export function useAnnouncement() {
  const shouldShow = (announcement: AnnouncementConfig | null | undefined): boolean => {
    const version = versionOf(announcement)
    if (!announcement || !version) return false
    if (announcement.enabled === false) return false
    const dismiss = readDismiss()
    if (dismiss && dismiss.version === version) {
      if (dismiss.mode === 'forever') return false
      if (dismiss.mode === 'today' && dismiss.date === todayStr()) return false
    }
    try {
      if (sessionStorage.getItem(SESSION_KEY) === version) return false
    } catch {
      // ignore
    }
    return true
  }
  const write = (store: 'localStorage' | 'sessionStorage', key: string, value: string) => {
    try {
      window[store].setItem(key, value)
    } catch {
      // ignore
    }
  }
  const dismissForever = (version: string) => write('localStorage', DISMISS_KEY, JSON.stringify({ version, mode: 'forever' }))
  const dismissToday = (version: string) => write('localStorage', DISMISS_KEY, JSON.stringify({ version, mode: 'today', date: todayStr() }))
  const closeForSession = (version: string) => write('sessionStorage', SESSION_KEY, version)
  return { shouldShow, dismissForever, dismissToday, closeForSession, versionOf }
}
