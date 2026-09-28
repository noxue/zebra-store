import { computed, ref, watch } from 'vue'
import { memberLevelAPI } from '@/api/catalog'
import { userProfileAPI } from '@/api/user'
import type { PublicMemberLevel } from '@/api/types'
import { useAppStore } from '@/stores/app'
import { useUserAuthStore } from '@/stores/userAuth'

// Shared (module level) so product detail and quick-buy reuse one fetch.
const memberLevelId = ref(0)
const memberLevels = ref<PublicMemberLevel[]>([])
let profileLoadedFor = ''
let levelsLoaded = false

/**
 * Current user's member level + discount rate (for member price display).
 * Reseller sites never apply member discounts (the checkout charges the reseller
 * price), so the level reads as 0 there (LQA-R1).
 */
export function useMemberPricing() {
  const auth = useUserAuthStore()
  const appStore = useAppStore()

  const ensure = async () => {
    if (!auth.isAuthenticated) {
      memberLevelId.value = 0
      profileLoadedFor = ''
      return
    }
    if (profileLoadedFor !== auth.token) {
      profileLoadedFor = auth.token
      try {
        const res = await userProfileAPI.current()
        memberLevelId.value = Number(res.data?.member_level_id || 0)
      } catch {
        memberLevelId.value = 0
      }
    }
    if (memberLevelId.value > 0 && !levelsLoaded) {
      levelsLoaded = true
      try {
        const res = await memberLevelAPI.list()
        memberLevels.value = Array.isArray(res.data) ? res.data : []
      } catch {
        levelsLoaded = false
      }
    }
  }

  watch(() => auth.token, () => void ensure())

  const effectiveLevelId = computed(() => (appStore.isResellerTenant ? 0 : memberLevelId.value))

  const discountRate = computed(() => {
    if (!effectiveLevelId.value) return 0
    const level = memberLevels.value.find((l) => Number(l.id) === effectiveLevelId.value)
    return Number(level?.discount_rate || 0)
  })

  return { memberLevelId: effectiveLevelId, discountRate, ensure }
}
