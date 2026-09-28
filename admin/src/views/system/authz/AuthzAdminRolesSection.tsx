import { defineComponent, type PropType } from 'vue'
import { useI18n } from 'vue-i18n'
import { Save } from 'lucide-vue-next'
import { Button, Card, Checkbox, FormField, IdCell, Loader, Select, cn } from '@/components/ui'
import { stripRolePrefix } from './authzUtils'
import type { AuthzPage } from './useAuthz'

/** 管理员角色分配: pick an admin, tick roles, save. */
export const AuthzAdminRolesSection = defineComponent({
  name: 'AuthzAdminRolesSection',
  props: { p: { type: Object as PropType<AuthzPage>, required: true } },
  setup(props) {
    const { t } = useI18n()
    const tx = (k: string) => t(`admin.authz.${k}`)

    return () => {
      const p = props.p
      const admin = p.selectedAdmin.value
      return (
        <Card title={tx('adminRolesTitle')}>
          <div class="grid grid-cols-1 gap-5 lg:grid-cols-[320px_1fr]">
            <div class="space-y-3">
              <FormField label={tx('adminLabel')}>
                <Select
                  modelValue={p.selectedAdminId.value}
                  onUpdate:modelValue={(v) => (p.selectedAdminId.value = Number(v) || 0)}
                  options={[
                    { label: tx('selectAdmin'), value: 0 },
                    ...p.admins.value.map((a) => ({ label: `${a.username}${a.is_super ? ' (super)' : ''}`, value: a.id })),
                  ]}
                />
              </FormField>
              {p.loadingAdmins.value ? (
                <Loader />
              ) : admin ? (
                <div class="space-y-1.5 rounded-zs border border-line bg-surface-muted/40 px-3 py-2 text-xs text-muted">
                  <div class="flex items-center justify-between">
                    <span>{tx('adminId')}</span>
                    <IdCell value={admin.id} />
                  </div>
                  <div class="flex items-center justify-between">
                    <span>{tx('superAdmin')}</span>
                    <span class={admin.is_super ? 'text-success-text' : ''}>{admin.is_super ? tx('yes') : tx('no')}</span>
                  </div>
                </div>
              ) : null}
            </div>

            <div class="min-w-0 space-y-3">
              <p class="text-sm text-muted">{tx('rolesLabel')}</p>
              <div class="grid grid-cols-1 gap-2 sm:grid-cols-2 lg:grid-cols-3">
                {p.roles.value.map((role) => {
                  const checked = p.isRoleChecked(role)
                  return (
                    <div
                      key={`admin-role-${role}`}
                      class={cn('rounded-zs-sm border px-3 py-2 transition-colors', checked ? 'border-primary bg-primary-soft' : 'border-line bg-surface-strong')}
                    >
                      <Checkbox modelValue={checked} onUpdate:modelValue={(v) => p.toggleAdminRole(role, v)} label={stripRolePrefix(role)} />
                    </div>
                  )
                })}
              </div>
              <div class="flex justify-end">
                <Button variant="primary" loading={p.savingAdminRoles.value} onClick={p.saveAdminRoles}>
                  <Save class="h-4 w-4" />
                  {tx('saveRoles')}
                </Button>
              </div>
            </div>
          </div>
        </Card>
      )
    }
  },
})
