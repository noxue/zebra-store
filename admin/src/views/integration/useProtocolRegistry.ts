import { ref } from 'vue'
import { adminAPI } from '@/api/admin'
import { errorMessage } from '@/api/client'
import type { SiteConnectionProtocolDef } from '@/api/types'
import { defaultProtocolId, fallbackProtocolDef } from './integrationUtils'

/** Supplier adapters registered on the backend (`GET /admin/site-connections/protocols`). */
export function useProtocolRegistry() {
  const protocols = ref<SiteConnectionProtocolDef[]>([])
  const loading = ref(false)
  const loaded = ref(false)
  const error = ref('')

  const load = async () => {
    loading.value = true
    error.value = ''
    try {
      const data = (await adminAPI.getSiteConnectionProtocols()).data
      protocols.value = Array.isArray(data) ? data : []
      loaded.value = true
    } catch (err) {
      error.value = errorMessage(err)
    } finally {
      loading.value = false
    }
  }

  const find = (id: string | undefined | null): SiteConnectionProtocolDef | null => (id ? (protocols.value.find((p) => p.id === id) ?? null) : null)

  /** The definition to render for `id`; a generic URL/Key/Secret one when it is not registered. */
  const resolve = (id: string | undefined | null): SiteConnectionProtocolDef => find(id) ?? fallbackProtocolDef(id || '')

  const defaultId = () => defaultProtocolId(protocols.value)

  return { protocols, loading, loaded, error, load, find, resolve, defaultId }
}

export type ProtocolRegistry = ReturnType<typeof useProtocolRegistry>
