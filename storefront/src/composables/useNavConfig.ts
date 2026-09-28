import { computed } from 'vue'
import type { LucideIcon } from 'lucide-vue-next'
import { useI18n } from 'vue-i18n'
import {
  Bell,
  BookOpen,
  Camera,
  Code,
  FileText,
  Gift,
  Globe,
  Heart,
  Home,
  Info,
  LayoutGrid,
  Link2,
  MapPin,
  MessageCircle,
  Music,
  Newspaper,
  Phone,
  Shield,
  Star,
  Zap,
} from 'lucide-vue-next'
import { useAppStore } from '@/stores/app'
import { localizedText } from '@/utils/localized'
import { isSafeNavUrl } from '@/utils/navUrl'

export interface NavItem {
  key: string
  path: string
  label: string
  icon: LucideIcon
  /** route → RouterLink, link → <a> */
  type: 'route' | 'link'
  target: string
}

const builtinNavDefs: Record<string, { path: string; label: string; icon: LucideIcon }> = {
  blog: { path: '/blog', label: 'nav.blog', icon: Newspaper },
  notice: { path: '/notice', label: 'nav.notice', icon: Bell },
  about: { path: '/about', label: 'nav.about', icon: Info },
}

const presetIcons: Record<string, LucideIcon> = {
  link: Link2,
  document: FileText,
  globe: Globe,
  star: Star,
  heart: Heart,
  chat: MessageCircle,
  gift: Gift,
  lightning: Zap,
  shield: Shield,
  book: BookOpen,
  code: Code,
  phone: Phone,
  map: MapPin,
  music: Music,
  camera: Camera,
}

const isExternalUrl = (url: string) => /^([a-z][a-z0-9+.-]*:)?\/\//i.test(url) || /^(mailto|tel):/i.test(url)

/** Site navigation from `config.nav_config` (builtin toggles + custom items). */
export const useNavConfig = () => {
  const { t, locale } = useI18n()
  const appStore = useAppStore()
  const navConfig = computed(() => appStore.config?.nav_config)
  const isListMode = computed(() => appStore.isListMode)
  const blogEnabled = computed(() => navConfig.value?.builtin?.blog !== false)
  const noticeEnabled = computed(() => navConfig.value?.builtin?.notice !== false)
  const aboutEnabled = computed(() => navConfig.value?.builtin?.about !== false)

  const builtinNavItems = computed<NavItem[]>(() => {
    const builtin = navConfig.value?.builtin
    return Object.entries(builtinNavDefs)
      .filter(([key]) => !(builtin && builtin[key] === false))
      .map(([key, def]) => ({ key, path: def.path, label: t(def.label), icon: def.icon, type: 'route' as const, target: '_self' }))
  })

  const customNavItems = computed<NavItem[]>(() => {
    const items = navConfig.value?.custom_items
    if (!Array.isArray(items)) return []
    return items
      .filter((item) => item.enabled !== false)
      .slice()
      .sort((a, b) => (a.sort_order || 0) - (b.sort_order || 0))
      .filter((item) => {
        const url = String(item.url || '').trim()
        return isSafeNavUrl(url, item.link_type === 'external' || isExternalUrl(url))
      })
      .map((item, index) => {
        const url = String(item.url || '').trim()
        const external = item.link_type === 'external' || isExternalUrl(url)
        return {
          key: `custom-${item.id ?? index}`,
          path: external || url.startsWith('/') ? url : `/${url}`,
          label: localizedText(item.title || item.name, locale.value),
          icon: presetIcons[String(item.icon)] || Link2,
          type: external ? ('link' as const) : ('route' as const),
          target: item.target === '_blank' ? '_blank' : '_self',
        }
      })
      .filter((item) => item.label && item.path)
  })

  const primaryNavItems = computed<NavItem[]>(() => {
    const items: NavItem[] = [{ key: 'home', path: '/', label: t('nav.home'), icon: Home, type: 'route', target: '_self' }]
    if (!isListMode.value) {
      items.push({ key: 'products', path: '/products', label: t('nav.products'), icon: LayoutGrid, type: 'route', target: '_self' })
    }
    items.push(...builtinNavItems.value, ...customNavItems.value)
    return items
  })

  const secondaryNavItems = computed<NavItem[]>(() => [...builtinNavItems.value, ...customNavItems.value])

  return {
    navConfig,
    isListMode,
    blogEnabled,
    noticeEnabled,
    aboutEnabled,
    builtinNavItems,
    customNavItems,
    primaryNavItems,
    secondaryNavItems,
  }
}
