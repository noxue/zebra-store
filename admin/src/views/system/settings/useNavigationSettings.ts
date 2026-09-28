import { reactive, ref } from 'vue'
import { adminAPI } from '@/api/admin'
import { notifySuccess } from '@/utils/notify'
import { notifyFailure, tr } from './common'
import { NAV_CUSTOM_MAX, asRecord, asString, normalizeLocalizedField, isRecord, type LocalizedField } from './settingsUtils'

export interface CustomNavItem {
  id: number
  title: LocalizedField
  link_type: 'internal' | 'external'
  url: string
  target: '_self' | '_blank'
  sort_order: number | ''
  enabled: boolean
  icon: string
}

/** Icon presets (same keys/paths as the original SettingsNavigationTab so the storefront renders them). */
export const NAV_PRESET_ICONS = [
  { key: 'link', label: 'Link', path: 'M13.828 10.172a4 4 0 00-5.656 0l-4 4a4 4 0 105.656 5.656l1.102-1.101m-.758-4.899a4 4 0 005.656 0l4-4a4 4 0 00-5.656-5.656l-1.1 1.1' },
  { key: 'document', label: 'Document', path: 'M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z' },
  { key: 'globe', label: 'Globe', path: 'M21 12a9 9 0 01-9 9m9-9a9 9 0 00-9-9m9 9H3m9 9a9 9 0 01-9-9m9 9c1.657 0 3-4.03 3-9s-1.343-9-3-9m0 18c-1.657 0-3-4.03-3-9s1.343-9 3-9' },
  { key: 'star', label: 'Star', path: 'M11.049 2.927c.3-.921 1.603-.921 1.902 0l1.519 4.674a1 1 0 00.95.69h4.915c.969 0 1.371 1.24.588 1.81l-3.976 2.888a1 1 0 00-.363 1.118l1.518 4.674c.3.922-.755 1.688-1.538 1.118l-3.976-2.888a1 1 0 00-1.176 0l-3.976 2.888c-.783.57-1.838-.197-1.538-1.118l1.518-4.674a1 1 0 00-.363-1.118l-3.976-2.888c-.784-.57-.38-1.81.588-1.81h4.914a1 1 0 00.951-.69l1.519-4.674z' },
  { key: 'heart', label: 'Heart', path: 'M4.318 6.318a4.5 4.5 0 000 6.364L12 20.364l7.682-7.682a4.5 4.5 0 00-6.364-6.364L12 7.636l-1.318-1.318a4.5 4.5 0 00-6.364 0z' },
  { key: 'chat', label: 'Chat', path: 'M8 12h.01M12 12h.01M16 12h.01M21 12c0 4.418-4.03 8-9 8a9.863 9.863 0 01-4.255-.949L3 20l1.395-3.72C3.512 15.042 3 13.574 3 12c0-4.418 4.03-8 9-8s9 3.582 9 8z' },
  { key: 'gift', label: 'Gift', path: 'M12 8v13m0-13V6a4 4 0 00-4-4 4 4 0 00-4 4v2h8zm0 0V6a4 4 0 014-4 4 4 0 014 4v2h-8zM5 8h14a1 1 0 011 1v3H4V9a1 1 0 011-1zm0 4h14v7a2 2 0 01-2 2H7a2 2 0 01-2-2v-7z' },
  { key: 'lightning', label: 'Lightning', path: 'M13 10V3L4 14h7v7l9-11h-7z' },
  { key: 'shield', label: 'Shield', path: 'M9 12l2 2 4-4m5.618-4.016A11.955 11.955 0 0112 2.944a11.955 11.955 0 01-8.618 3.04A12.02 12.02 0 003 9c0 5.591 3.824 10.29 9 11.622 5.176-1.332 9-6.03 9-11.622 0-1.042-.133-2.052-.382-3.016z' },
  { key: 'book', label: 'Book', path: 'M12 6.253v13m0-13C10.832 5.477 9.246 5 7.5 5S4.168 5.477 3 6.253v13C4.168 18.477 5.754 18 7.5 18s3.332.477 4.5 1.253m0-13C13.168 5.477 14.754 5 16.5 5c1.747 0 3.332.477 4.5 1.253v13C19.832 18.477 18.247 18 16.5 18c-1.746 0-3.332.477-4.5 1.253' },
  { key: 'code', label: 'Code', path: 'M10 20l4-16m4 4l4 4-4 4M6 16l-4-4 4-4' },
  { key: 'phone', label: 'Phone', path: 'M3 5a2 2 0 012-2h3.28a1 1 0 01.948.684l1.498 4.493a1 1 0 01-.502 1.21l-2.257 1.13a11.042 11.042 0 005.516 5.516l1.13-2.257a1 1 0 011.21-.502l4.493 1.498a1 1 0 01.684.949V19a2 2 0 01-2 2h-1C9.716 21 3 14.284 3 6V5z' },
  { key: 'map', label: 'Map', path: 'M17.657 16.657L13.414 20.9a1.998 1.998 0 01-2.827 0l-4.244-4.243a8 8 0 1111.314 0z M15 11a3 3 0 11-6 0 3 3 0 016 0z' },
  { key: 'music', label: 'Music', path: 'M9 19V6l12-3v13M9 19c0 1.105-1.343 2-3 2s-3-.895-3-2 1.343-2 3-2 3 .895 3 2zm12-3c0 1.105-1.343 2-3 2s-3-.895-3-2 1.343-2 3-2 3 .895 3 2zM9 10l12-3' },
  { key: 'camera', label: 'Camera', path: 'M3 9a2 2 0 012-2h.93a2 2 0 001.664-.89l.812-1.22A2 2 0 0110.07 4h3.86a2 2 0 011.664.89l.812 1.22A2 2 0 0018.07 7H19a2 2 0 012 2v9a2 2 0 01-2 2H5a2 2 0 01-2-2V9z M15 13a3 3 0 11-6 0 3 3 0 016 0z' },
] as const

let idSeq = 0
const createEmptyItem = (): CustomNavItem => ({
  id: Date.now() + idSeq++,
  title: { 'zh-CN': '', 'zh-TW': '', 'en-US': '' },
  link_type: 'internal',
  url: '',
  target: '_self',
  sort_order: 0,
  enabled: true,
  icon: 'link',
})

/** 导航配置 (key `nav_config`). */
export function useNavigationSettings() {
  const submitting = ref(false)
  const form = reactive({
    builtin: { blog: true, notice: true, about: true },
    customItems: [] as CustomNavItem[],
  })

  const load = (raw: unknown) => {
    const d = asRecord(raw)
    if (isRecord(d.builtin)) {
      form.builtin.blog = d.builtin.blog !== false
      form.builtin.notice = d.builtin.notice !== false
      form.builtin.about = d.builtin.about !== false
    }
    if (Array.isArray(d.custom_items)) {
      form.customItems = d.custom_items.filter(isRecord).map((item) => ({
        id: Number(item.id) || Date.now() + idSeq++,
        title: normalizeLocalizedField(item.title),
        link_type: item.link_type === 'external' ? 'external' : 'internal',
        url: asString(item.url),
        target: item.target === '_blank' ? '_blank' : '_self',
        sort_order: Number(item.sort_order) || 0,
        enabled: item.enabled !== false,
        icon: asString(item.icon, 'link'),
      }))
    }
  }

  const addItem = () => {
    if (form.customItems.length >= NAV_CUSTOM_MAX) return
    form.customItems.push(createEmptyItem())
  }
  const removeItem = (index: number) => form.customItems.splice(index, 1)

  const save = async () => {
    submitting.value = true
    try {
      await adminAPI.updateSettings({
        key: 'nav_config',
        value: {
          builtin: { ...form.builtin },
          custom_items: form.customItems.map((item) => ({
            id: item.id,
            title: { ...item.title },
            link_type: item.link_type,
            url: item.url,
            target: item.target,
            sort_order: Number(item.sort_order) || 0,
            enabled: item.enabled,
            icon: item.icon,
          })),
        },
      })
      notifySuccess(tr('admin.settings.alerts.saveSuccess'))
    } catch (err) {
      notifyFailure(err)
    } finally {
      submitting.value = false
    }
  }

  return { form, submitting, load, addItem, removeItem, save }
}

export type NavigationSettingsModel = ReturnType<typeof useNavigationSettings>
