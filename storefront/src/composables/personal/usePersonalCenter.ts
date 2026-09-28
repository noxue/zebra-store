import { computed, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import type { LucideIcon } from 'lucide-vue-next'
import { Gift, Home, Key, Megaphone, ShieldCheck, ShoppingBag, Store, UserCircle, Wallet } from 'lucide-vue-next'
import type { PublicMemberLevel } from '@/api/types'
import { useAppStore } from '@/stores/app'
import { useUserProfileStore } from '@/stores/userProfile'
import { getImageUrl } from '@/utils/image'
import { localizedText } from '@/utils/localized'
import { discountNumber, isImageIcon } from '@/utils/personal'
import { usePageTitle } from '@/composables/usePageTitle'

export type PersonalSection = 'overview' | 'profile' | 'security' | 'orders' | 'wallet' | 'giftCard' | 'affiliate' | 'reseller' | 'api'

export interface PersonalNavItem {
  key: PersonalSection
  label: string
  icon: LucideIcon
  to: string
}

const SECTIONS: Array<{ key: PersonalSection; icon: LucideIcon; to: string }> = [
  { key: 'overview', icon: Home, to: '/me' },
  { key: 'orders', icon: ShoppingBag, to: '/me/orders' },
  { key: 'wallet', icon: Wallet, to: '/me/wallet' },
  { key: 'affiliate', icon: Megaphone, to: '/me/affiliate' },
  { key: 'reseller', icon: Store, to: '/reseller' },
  { key: 'giftCard', icon: Gift, to: '/me/gift-cards' },
  { key: 'security', icon: ShieldCheck, to: '/me/security' },
  { key: 'api', icon: Key, to: '/me/api' },
  { key: 'profile', icon: UserCircle, to: '/me/profile' },
]

const KNOWN = new Set<string>(SECTIONS.map((s) => s.key))

/** Personal center shell: header data, section navigation, initial loading. */
export function usePersonalCenter(sectionGetter: () => string) {
  const { t, locale } = useI18n()
  const appStore = useAppStore()
  const store = useUserProfileStore()
  const loadError = ref('')

  const canAccessResellerConsole = computed(() => appStore.canAccessResellerConsole)
  const navItems = computed<PersonalNavItem[]>(() =>
    SECTIONS.filter((s) => s.key !== 'reseller' || canAccessResellerConsole.value).map((s) => ({
      ...s,
      label: t(`personalCenter.tabs.${s.key}`),
    })),
  )

  const currentSection = computed<PersonalSection>(() => {
    const raw = sectionGetter()
    if (!KNOWN.has(raw)) return 'overview'
    if (raw === 'reseller' && !canAccessResellerConsole.value) return 'overview'
    return raw as PersonalSection
  })

  usePageTitle(() => t(`personalCenter.tabs.${currentSection.value}`))

  const displayInitial = computed(() => (store.displayName || '').trim().slice(0, 1).toUpperCase() || 'U')
  const emailVerified = computed(() => !!store.profile?.email_verified_at)

  const levelName = (level: PublicMemberLevel | null | undefined): string => {
    if (!level) return t('personalCenter.memberLevel.defaultLevel')
    return localizedText(level.name, locale.value) || level.slug || t('personalCenter.memberLevel.defaultLevel')
  }

  const discountLabel = (level: PublicMemberLevel | null | undefined): string => {
    const n = discountNumber(level?.discount_rate)
    return n ? t('personalCenter.memberLevel.discountOff', { n }) : t('personalCenter.memberLevel.noDiscount')
  }

  const levelIconUrl = (level: PublicMemberLevel | null | undefined): string =>
    level && isImageIcon(level.icon) ? getImageUrl(level.icon) : ''

  const initialize = async () => {
    loadError.value = ''
    const [ok] = await Promise.all([store.loadProfile(), store.loadRecentOrders(5), store.loadMemberLevels()])
    if (!ok) loadError.value = store.profileError || t('personalCenter.common.loadFailed')
  }

  onMounted(() => {
    void initialize()
  })

  return {
    store,
    navItems,
    currentSection,
    canAccessResellerConsole,
    displayInitial,
    emailVerified,
    loadError,
    levelName,
    discountLabel,
    levelIconUrl,
    initialize,
  }
}
