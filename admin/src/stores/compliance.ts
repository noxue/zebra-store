import { defineStore } from 'pinia'
import { adminAPI } from '@/api/admin'

export const useComplianceStore = defineStore('compliance', {
  state: () => ({
    acknowledged: false,
    acknowledgedAt: '',
    acknowledgedByUsername: '',
    loaded: false,
    loading: false,
  }),
  actions: {
    async fetchStatus(force = false): Promise<void> {
      if (this.loaded && !force) return
      this.loading = true
      try {
        const { data } = await adminAPI.getComplianceStatus()
        this.acknowledged = !!data?.acknowledged
        this.acknowledgedAt = data?.acknowledged_at ?? ''
        this.acknowledgedByUsername = data?.acknowledged_by_username ?? ''
        this.loaded = true
      } finally {
        this.loading = false
      }
    },
    async acknowledge(segment1: string, segment2: string, segment3: string): Promise<void> {
      await adminAPI.acknowledgeCompliance({ segment1, segment2, segment3 })
      await this.fetchStatus(true)
    },
    reset(): void {
      this.acknowledged = false
      this.acknowledgedAt = ''
      this.acknowledgedByUsername = ''
      this.loaded = false
    },
  },
})
