import { defineComponent, type PropType } from 'vue'
import type { AdminResellerProfileRef } from '@/api/types'
import { adminUrl } from '@/utils/adminBase'
import { resolveResellerUserId } from '../resellerUtils'

/** Reseller column: display name, email, `#userId` link (new tab) and `R#resellerId`. */
export const ResellerCell = defineComponent({
  name: 'ResellerCell',
  props: {
    profile: {
      type: Object as PropType<AdminResellerProfileRef | undefined>,
      default: undefined,
    },
    resellerId: { type: Number, required: true },
  },
  setup(props) {
    return () => {
      const uid = resolveResellerUserId({ profile: props.profile })
      const user = props.profile?.user
      return (
        <div class="min-w-[160px] text-xs text-muted">
          <div class="break-words text-sm text-fg">{user?.display_name || '-'}</div>
          {user?.email && <div class="mt-0.5 break-all">{user.email}</div>}
          <div class="mt-0.5 break-all">
            {uid > 0 ? (
              <a href={adminUrl(`/users/${uid}`)} target="_blank" rel="noopener" class="font-mono text-accent hover:underline">
                #{uid}
              </a>
            ) : (
              <span class="font-mono text-fg">-</span>
            )}
            <span class="ml-1 font-mono">/ R#{props.resellerId}</span>
          </div>
        </div>
      )
    }
  },
})

export default ResellerCell
