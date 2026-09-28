import { defineComponent, type PropType } from 'vue'
import { useI18n } from 'vue-i18n'
import { Lock, Plus, Trash2 } from 'lucide-vue-next'
import { Badge, Button, Card, Input, Loader, cn } from '@/components/ui'
import { stripRolePrefix } from './authzUtils'
import type { AuthzPage } from './useAuthz'

/** 角色列表: create / select / delete (built-in immutable roles are locked). */
export const AuthzRolesSection = defineComponent({
  name: 'AuthzRolesSection',
  props: { p: { type: Object as PropType<AuthzPage>, required: true } },
  setup(props) {
    const { t } = useI18n()
    const tx = (k: string) => t(`admin.authz.${k}`)

    return () => {
      const p = props.p
      return (
        <Card title={tx('rolesTitle')}>
          {{
            extra: () => <Badge tone="primary">{p.roles.value.length}</Badge>,
            default: () => (
              <div class="space-y-4">
                <div class="flex gap-2">
                  <Input v-model={p.newRole.value} placeholder={tx('rolePlaceholder')} onEnter={p.createRole} />
                  <Button variant="primary" onClick={p.createRole}>
                    <Plus class="h-4 w-4" />
                    {tx('create')}
                  </Button>
                </div>
                <div class="max-h-[420px] space-y-2 overflow-auto pr-1">
                  {p.loadingRoles.value ? (
                    <Loader />
                  ) : p.roles.value.length === 0 ? (
                    <p class="text-sm text-muted">{tx('rolesEmpty')}</p>
                  ) : (
                    p.roles.value.map((role) => {
                      const active = p.selectedRole.value === role
                      const locked = p.isRoleImmutable(role)
                      return (
                        <div
                          key={role}
                          role="button"
                          tabindex={0}
                          class={cn(
                            'flex w-full cursor-pointer items-center justify-between gap-2 rounded-zs border px-3 py-2 text-left text-sm transition-colors',
                            active ? 'border-primary bg-primary-soft text-fg' : 'border-line text-muted hover:border-line-strong hover:bg-surface-muted',
                          )}
                          onClick={() => (p.selectedRole.value = role)}
                          onKeydown={(e: KeyboardEvent) => e.key === 'Enter' && (p.selectedRole.value = role)}
                        >
                          <span class="flex min-w-0 items-center gap-2 font-medium">
                            {locked && <Lock class="h-3.5 w-3.5 shrink-0 text-muted" />}
                            <span class="truncate">{stripRolePrefix(role)}</span>
                          </span>
                          <Button
                            size="icon-sm"
                            variant="ghost"
                            class="text-danger-text"
                            disabled={locked}
                            title={locked ? tx('immutableRoleHint') : tx('delete')}
                            onClick={(e: MouseEvent) => {
                              e.stopPropagation()
                              void p.deleteRole(role)
                            }}
                          >
                            <Trash2 class="h-3.5 w-3.5" />
                          </Button>
                        </div>
                      )
                    })
                  )}
                </div>
              </div>
            ),
          }}
        </Card>
      )
    }
  },
})
