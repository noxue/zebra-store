import { defineComponent, type PropType } from 'vue'
import { useI18n } from 'vue-i18n'
import { Search, UserPlus, UserPen } from 'lucide-vue-next'
import { Badge, Button, Card, DataTable, FormField, IdCell, Input, Switch, type DataTableColumn } from '@/components/ui'
import type { AdminAuthzAdmin } from '@/api/types'
import { useAdminAuthStore } from '@/stores/auth'
import { formatDate } from '@/utils/format'
import { stripRolePrefix } from './authzUtils'
import type { AuthzPage } from './useAuthz'

/** 管理员列表 + 新增/编辑管理员表单. */
export const AuthzAdminsSection = defineComponent({
  name: 'AuthzAdminsSection',
  props: { p: { type: Object as PropType<AuthzPage>, required: true } },
  setup(props) {
    const { t } = useI18n()
    const auth = useAdminAuthStore()
    const tx = (k: string) => t(`admin.authz.${k}`)

    const columns = (): DataTableColumn<AdminAuthzAdmin>[] => {
      const p = props.p
      return [
        { key: 'id', title: tx('adminTableId'), render: (r) => <IdCell value={r.id} /> },
        { key: 'username', title: tx('adminTableUsername'), class: 'font-medium break-words min-w-[140px]', render: (r) => r.username },
        {
          key: 'roles',
          title: tx('adminTableRoles'),
          class: 'min-w-[160px]',
          render: (r) =>
            r.roles && r.roles.length ? (
              <div class="flex flex-wrap gap-1">
                {r.roles.map((role) => (
                  <Badge key={role} tone="secondary">
                    {stripRolePrefix(role)}
                  </Badge>
                ))}
              </div>
            ) : (
              <span class="text-xs text-muted">{tx('adminRolesEmpty')}</span>
            ),
        },
        {
          key: 'super',
          title: tx('adminTableSuper'),
          render: (r) => (
            <Badge tone={r.is_super ? 'gold' : 'neutral'} dot>
              {r.is_super ? tx('yes') : tx('no')}
            </Badge>
          ),
        },
        {
          key: 'totp',
          title: tx('adminTableTotp'),
          render: (r) => (
            <Badge tone={r.totp_enabled ? 'success' : 'neutral'} dot>
              {r.totp_enabled ? tx('adminTotpEnabled') : tx('adminTotpDisabled')}
            </Badge>
          ),
        },
        {
          key: 'createdAt',
          title: tx('adminTableCreatedAt'),
          class: 'text-xs text-muted whitespace-nowrap',
          render: (r) => formatDate(r.created_at) || tx('unknown'),
        },
        {
          key: 'lastLogin',
          title: tx('adminTableLastLoginAt'),
          class: 'text-xs text-muted whitespace-nowrap',
          render: (r) => formatDate(r.last_login_at) || tx('unknown'),
        },
        {
          key: 'op',
          title: tx('tableOperation'),
          align: 'right',
          render: (r) => (
            <div class="flex flex-wrap items-center justify-end gap-2">
              <Button size="xs" variant={p.selectedAdminId.value === r.id ? 'soft' : 'outline'} onClick={() => p.pickAdminForRoles(r)}>
                {p.selectedAdminId.value === r.id ? tx('adminRoleTargetSelected') : tx('adminSelectRoleTarget')}
              </Button>
              <Button size="xs" onClick={() => p.openAdminEditForm(r)}>
                {tx('edit')}
              </Button>
              {auth.isSuper && r.totp_enabled && (
                <Button size="xs" onClick={() => p.resetAdmin2FA(r)}>
                  {tx('adminTotpReset')}
                </Button>
              )}
              <Button size="xs" variant="danger" onClick={() => p.deleteAdmin(r)}>
                {tx('delete')}
              </Button>
            </div>
          ),
        },
      ]
    }

    return () => {
      const p = props.p
      const editing = p.adminFormMode.value === 'edit'
      return (
        <Card title={tx('adminManageTitle')} description={tx('adminManageHint')}>
          {{
            extra: () => <Badge tone="primary">{p.admins.value.length}</Badge>,
            default: () => (
              <div class="grid grid-cols-1 gap-4 2xl:grid-cols-[minmax(0,1fr)_340px]">
                <div class="min-w-0 space-y-3">
                  <Input icon={Search} v-model={p.adminKeyword.value} placeholder={tx('adminSearchPlaceholder')} />
                  <DataTable
                    columns={columns()}
                    rows={p.filteredAdmins.value}
                    rowKey={(r) => r.id}
                    loading={p.loadingAdmins.value}
                    emptyText={tx('adminListEmpty')}
                    rowClass={(r) => (p.selectedAdminId.value === r.id ? 'bg-primary-soft/50' : '')}
                    minWidth="900px"
                  />
                </div>

                <div class="space-y-4 rounded-zs border border-line bg-surface-muted/40 p-4">
                  <div class="flex items-center justify-between gap-2">
                    <h4 class="flex items-center gap-2 text-sm font-semibold text-fg">
                      {editing ? <UserPen class="h-4 w-4 text-primary" /> : <UserPlus class="h-4 w-4 text-primary" />}
                      {editing ? tx('adminFormTitleEdit') : tx('adminFormTitleCreate')}
                    </h4>
                    <Button size="xs" variant="ghost" onClick={p.resetAdminForm}>
                      {tx('adminFormReset')}
                    </Button>
                  </div>
                  <div class="grid grid-cols-1 gap-4 md:grid-cols-2 2xl:grid-cols-1">
                    <FormField label={tx('adminUsername')} required>
                      <Input v-model={p.adminForm.username} placeholder={tx('adminUsernamePlaceholder')} autocomplete="off" />
                    </FormField>
                    <FormField label={tx('adminPassword')} required={!editing}>
                      <Input
                        type="password"
                        v-model={p.adminForm.password}
                        autocomplete="new-password"
                        placeholder={editing ? tx('adminPasswordPlaceholderEdit') : tx('adminPasswordPlaceholderCreate')}
                      />
                    </FormField>
                    <div class="flex items-center rounded-zs-sm border border-line bg-surface-strong px-3 py-2">
                      <Switch v-model={p.adminForm.isSuper} size="sm" label={tx('adminIsSuper')} />
                    </div>
                    <Button variant="primary" block class="self-end" loading={p.savingAdminForm.value} onClick={p.submitAdminForm}>
                      {editing ? tx('adminSubmitUpdate') : tx('adminSubmitCreate')}
                    </Button>
                  </div>
                </div>
              </div>
            ),
          }}
        </Card>
      )
    }
  },
})
