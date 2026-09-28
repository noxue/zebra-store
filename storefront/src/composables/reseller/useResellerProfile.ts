import { computed, inject, provide, ref, type InjectionKey } from 'vue'
import { resellerAPI } from '@/api/reseller'
import type { ResellerManagementSnapshotData } from '@/api/types'
import { getResellerConsoleState, pickPrimaryDomain } from '@/utils/reseller/console'

/** Management snapshot (`/reseller/profile`) + derived console state. */
export const useResellerProfile = () => {
  const loading = ref(false)
  const snapshot = ref<ResellerManagementSnapshotData | null>(null)
  const error = ref('')
  const state = computed(() => getResellerConsoleState(snapshot.value))
  const primaryDomain = computed(() => pickPrimaryDomain(snapshot.value?.domains)?.domain || '')
  const primaryDomainUrl = computed(() => (primaryDomain.value ? `https://${primaryDomain.value}` : ''))

  const load = async () => {
    loading.value = true
    error.value = ''
    try {
      const res = await resellerAPI.managementProfile()
      snapshot.value = res.data || null
    } catch (err) {
      error.value = err instanceof Error ? err.message : 'error'
      snapshot.value = null
    } finally {
      loading.value = false
    }
  }

  return { loading, snapshot, error, state, primaryDomain, primaryDomainUrl, load }
}

type ResellerProfileContext = ReturnType<typeof useResellerProfile>
const PROFILE_KEY: InjectionKey<ResellerProfileContext> = Symbol('reseller-profile')

/** Layout-level shared profile so pages and the nav stay in sync. */
export const provideResellerProfile = (): ResellerProfileContext => {
  const ctx = useResellerProfile()
  provide(PROFILE_KEY, ctx)
  return ctx
}

/** Shared profile from the console layout (standalone instance as fallback). */
export const useResellerProfileContext = (): ResellerProfileContext => inject(PROFILE_KEY, null) ?? useResellerProfile()
