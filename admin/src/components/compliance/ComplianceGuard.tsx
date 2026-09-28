import { defineComponent, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { ShieldCheck } from 'lucide-vue-next'
import { useComplianceStore } from '@/stores/compliance'
import { useAdminAuthStore } from '@/stores/auth'
import { formatDate } from '@/utils/format'
import { ComplianceAckDialog } from './ComplianceAckDialog'

/** Wraps compliance-gated pages: super admins who have not acknowledged get the blocking dialog. */
export const ComplianceGuard = defineComponent({
  name: 'ComplianceGuard',
  setup(_, { slots }) {
    const { t } = useI18n()
    const compliance = useComplianceStore()
    const auth = useAdminAuthStore()
    const open = ref(false)
    const sync = () => {
      if (!compliance.loaded) return
      open.value = !compliance.acknowledged && auth.isSuper
    }
    onMounted(async () => {
      if (!compliance.loaded) {
        try {
          await compliance.fetchStatus()
        } catch {
          /* silent */
        }
      }
      sync()
    })
    watch(() => [compliance.loaded, compliance.acknowledged, auth.isSuper], sync)
    return () => (
      <div class="space-y-3">
        {compliance.acknowledged && (
          <div class="flex justify-end">
            <span
              class="inline-flex items-center gap-1.5 rounded-full border border-success/40 bg-success-soft px-3 py-1 text-xs text-success-text"
              title={t('compliance.badge.by', { username: compliance.acknowledgedByUsername || '-', date: formatDate(compliance.acknowledgedAt) })}
            >
              <ShieldCheck class="h-3.5 w-3.5" />
              {t('compliance.badge.acked')}
            </span>
          </div>
        )}
        {slots.default?.()}
        <ComplianceAckDialog open={open.value} onUpdate:open={(v: boolean) => (open.value = v)} />
      </div>
    )
  },
})

export default ComplianceGuard
