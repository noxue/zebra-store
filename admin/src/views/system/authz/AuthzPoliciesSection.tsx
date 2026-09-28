import { defineComponent, type PropType } from 'vue'
import { useI18n } from 'vue-i18n'
import { Check, ChevronDown, ChevronRight, Info, Plus, Search } from 'lucide-vue-next'
import { Badge, Button, Card, DataTable, Input, Loader, Select, type DataTableColumn } from '@/components/ui'
import type { AdminAuthzPolicy } from '@/api/types'
import { POLICY_ACTIONS, stripRolePrefix } from './authzUtils'
import type { AuthzPage } from './useAuthz'

const methodTone = (m: string) =>
  m === 'GET' ? 'info' : m === 'POST' ? 'success' : m === 'PUT' || m === 'PATCH' ? 'warning' : m === 'DELETE' ? 'danger' : m === '*' ? 'gold' : 'neutral'

/** 角色策略: manual grant form, searchable/grouped permission catalog with one-click grant, policy table with revoke. */
export const AuthzPoliciesSection = defineComponent({
  name: 'AuthzPoliciesSection',
  props: { p: { type: Object as PropType<AuthzPage>, required: true } },
  setup(props) {
    const { t } = useI18n()
    const tx = (k: string) => t(`admin.authz.${k}`)

    const columns = (): DataTableColumn<AdminAuthzPolicy>[] => [
      { key: 'object', title: tx('tableObject'), class: 'font-mono text-xs text-muted break-all min-w-[320px]', render: (r) => r.object },
      {
        key: 'action',
        title: tx('tableAction'),
        render: (r) => (
          <Badge tone={methodTone(r.action)} class="font-mono">
            {r.action}
          </Badge>
        ),
      },
      {
        key: 'op',
        title: tx('tableOperation'),
        align: 'right',
        render: (r) => (
          <Button size="xs" variant="outline" class="text-danger-text" disabled={props.p.selectedRoleImmutable.value} onClick={() => props.p.revokePolicy(r)}>
            {tx('delete')}
          </Button>
        ),
      },
    ]

    const catalog = () => {
      const p = props.p
      const immutable = p.selectedRoleImmutable.value
      return (
        <div class="space-y-3 rounded-zs border border-line bg-surface-muted/40 p-3">
          <div class="flex items-start justify-between gap-2">
            <div>
              <div class="text-sm font-semibold text-fg">{tx('catalogTitle')}</div>
              <div class="text-xs text-muted">{tx('catalogHint')}</div>
            </div>
            <span class="zs-num text-xs text-muted">
              {p.filteredCatalog.value.length}/{p.permissionCatalog.value.length}
            </span>
          </div>
          <div class="grid grid-cols-1 gap-2 md:grid-cols-[1fr_auto_auto]">
            <Input icon={Search} size="sm" v-model={p.catalogKeyword.value} placeholder={tx('catalogSearch')} />
            <Button size="sm" onClick={p.expandAllModules}>
              {tx('catalogExpandAll')}
            </Button>
            <Button size="sm" onClick={p.collapseAllModules}>
              {tx('catalogCollapseAll')}
            </Button>
          </div>
          <div class="max-h-72 space-y-2 overflow-auto pr-1">
            {p.loadingCatalog.value ? (
              <Loader />
            ) : p.groupedCatalog.value.length === 0 ? (
              <p class="text-xs text-muted">{tx('catalogEmpty')}</p>
            ) : (
              p.groupedCatalog.value.map((group) => {
                const collapsed = p.isModuleCollapsed(group.module)
                return (
                  <div key={group.module} class="space-y-2">
                    <button
                      type="button"
                      class="flex w-full items-center justify-between rounded-zs-sm border border-line bg-surface-strong px-3 py-2 text-left hover:border-primary"
                      onClick={() => p.toggleModule(group.module)}
                    >
                      <span class="flex items-center gap-1.5 text-xs font-semibold uppercase tracking-wide text-fg">
                        {collapsed ? <ChevronRight class="h-3.5 w-3.5" /> : <ChevronDown class="h-3.5 w-3.5" />}
                        {group.module}
                      </span>
                      <span class="zs-num text-xs text-muted">{group.items.length}</span>
                    </button>
                    {!collapsed && (
                      <div class="space-y-1.5 pl-1">
                        {group.items.map((item) => {
                          const covered = p.isCovered(item)
                          return (
                            <div key={item.permission} class="flex items-center justify-between gap-2 rounded-zs-sm border border-line bg-surface px-3 py-2">
                              <div class="min-w-0">
                                <div class="text-[11px] uppercase tracking-wide text-muted">{item.module}</div>
                                <div class="flex items-center gap-2 break-all font-mono text-xs text-fg">
                                  <Badge tone={methodTone(item.method)} class="font-mono">
                                    {item.method}
                                  </Badge>
                                  <span>{item.object}</span>
                                </div>
                              </div>
                              <Button
                                size="xs"
                                variant={covered ? 'outline' : 'soft'}
                                disabled={immutable || covered}
                                onClick={() => p.grantCatalogPolicy(item)}
                              >
                                {covered ? <Check class="h-3 w-3" /> : <Plus class="h-3 w-3" />}
                                {covered ? tx('catalogAdded') : tx('catalogAdd')}
                              </Button>
                            </div>
                          )
                        })}
                      </div>
                    )}
                  </div>
                )
              })
            )}
          </div>
        </div>
      )
    }

    return () => {
      const p = props.p
      const immutable = p.selectedRoleImmutable.value
      return (
        <Card title={tx('policiesTitle')}>
          {{
            extra: () => (p.selectedRole.value ? <Badge tone="secondary">{stripRolePrefix(p.selectedRole.value)}</Badge> : null),
            default: () =>
              !p.selectedRole.value ? (
                <p class="text-sm text-muted">{tx('selectRoleHint')}</p>
              ) : (
                <div class="space-y-4">
                  {immutable && (
                    <div class="flex items-start gap-2 rounded-zs border border-line bg-info-soft px-3 py-2 text-sm text-info-text">
                      <Info class="mt-0.5 h-4 w-4 shrink-0" />
                      {tx('immutableRoleHint')}
                    </div>
                  )}
                  <div class="grid grid-cols-1 gap-2 md:grid-cols-[1fr_140px_auto]">
                    <Input v-model={p.policyForm.object} mono placeholder={tx('objectPlaceholder')} disabled={immutable} onEnter={p.grantPolicy} />
                    <Select
                      modelValue={p.policyForm.action}
                      onUpdate:modelValue={(v) => (p.policyForm.action = String(v))}
                      options={POLICY_ACTIONS.map((a) => ({ label: a, value: a }))}
                      disabled={immutable}
                    />
                    <Button variant="primary" disabled={immutable} onClick={p.grantPolicy}>
                      <Plus class="h-4 w-4" />
                      {tx('addPolicy')}
                    </Button>
                  </div>
                  {catalog()}
                  <DataTable
                    columns={columns()}
                    rows={p.policies.value}
                    rowKey={(r) => `${r.object}:${r.action}`}
                    loading={p.loadingPolicies.value}
                    emptyText={tx('policiesEmpty')}
                    minWidth="560px"
                  />
                </div>
              ),
          }}
        </Card>
      )
    }
  },
})
