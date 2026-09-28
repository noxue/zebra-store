import { defineComponent, type PropType } from 'vue'
import { adminUrl } from '@/utils/adminBase'

export interface AffiliateProfileRef {
  user_id?: number
  code?: string
  user?: { id?: number; email?: string; display_name?: string }
}

/** Display name / email / #userId (link) / code — shared by commissions & withdraws tables. */
export const AffiliateProfileCell = defineComponent({
  name: 'AffiliateProfileCell',
  props: { profile: { type: Object as PropType<AffiliateProfileRef | null | undefined>, default: undefined } },
  setup(props) {
    return () => {
      const p = props.profile
      const uid = Number(p?.user_id || p?.user?.id || 0)
      return (
        <div class="text-xs text-muted">
          <div class="break-words text-fg">{p?.user?.display_name || '-'}</div>
          {p?.user?.email && <div class="mt-0.5 break-all">{p.user.email}</div>}
          <div class="mt-0.5 break-all">
            {uid > 0 ? (
              <a href={adminUrl(`/users/${uid}`)} target="_blank" rel="noopener" class="font-mono text-primary underline-offset-4 hover:underline">
                #{uid}
              </a>
            ) : (
              <span class="font-mono text-fg">-</span>
            )}
            <span class="ml-1 font-mono">/ {p?.code || '-'}</span>
          </div>
        </div>
      )
    }
  },
})

export default AffiliateProfileCell
