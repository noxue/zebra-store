import { defineComponent, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { Plus, RefreshCw, Search } from 'lucide-vue-next'
import {
  Badge,
  Button,
  Checkbox,
  DataTable,
  DateTimeInput,
  Dialog,
  FilterBar,
  FormField,
  IdCell,
  Input,
  ListPagination,
  MultiSelect,
  PageHeader,
  Select,
  Switch,
  type DataTableColumn,
} from '@/components/ui'
import type { AdminCoupon } from '@/api/types'
import { formatDate, labelSeparator } from '@/utils/format'
import { buildProductLabel, formatProductScope, normalizeScopeIDs } from './marketingUtils'
import { useCoupons } from './useCoupons'

export default defineComponent({
  name: 'CouponsView',
  setup() {
    const { t, locale } = useI18n()
    const p = useCoupons()
    onMounted(() => void p.init())

    const discountTypeLabel = (type: string) =>
      type === 'percent' ? t('admin.common.discountTypes.percent') : type === 'fixed' ? t('admin.common.discountTypes.fixed') : type

    const line = (label: string, value: unknown) => (
      <div class="break-words">
        <span class="text-muted/80">
          {label}
          {labelSeparator(locale.value)}
        </span>
        <span class="text-fg/80">{value === 0 || value === '' || value === null || value === undefined ? '-' : String(value)}</span>
      </div>
    )

    const columns = (): DataTableColumn<AdminCoupon>[] => [
      {
        key: 'id',
        title: t('admin.coupons.table.id'),
        render: (r) => <IdCell value={r.id} />,
      },
      {
        key: 'code',
        title: t('admin.coupons.table.code'),
        class: 'font-mono font-medium break-all',
        render: (r) => r.code,
      },
      {
        key: 'type',
        title: t('admin.coupons.table.type'),
        class: 'text-xs text-muted whitespace-nowrap',
        render: (r) => discountTypeLabel(r.type),
      },
      {
        key: 'value',
        title: t('admin.coupons.table.value'),
        class: 'zs-num',
        render: (r) => String(r.value),
      },
      {
        key: 'scope',
        title: t('admin.coupons.table.scope'),
        class: 'min-w-[140px] max-w-[260px] text-xs text-muted break-words',
        render: (r) => formatProductScope(normalizeScopeIDs(r.scope_ref_ids), p.productOpts.products.value),
      },
      {
        key: 'limits',
        title: t('admin.coupons.table.limits'),
        class: 'min-w-[180px] text-xs',
        render: (r) => (
          <div class="space-y-0.5">
            {line(t('admin.coupons.limit.minAmount'), r.min_amount)}
            {line(t('admin.coupons.limit.maxDiscount'), r.max_discount)}
            {line(t('admin.coupons.limit.usageLimit'), r.usage_limit)}
            {line(t('admin.coupons.limit.perUserLimit'), r.per_user_limit)}
            {line(t('admin.coupons.limit.paymentRoles'), p.formatPaymentRoles(r.payment_roles))}
            {line(t('admin.coupons.limit.memberLevels'), p.formatMemberLevels(r.member_levels))}
          </div>
        ),
      },
      {
        key: 'period',
        title: t('admin.coupons.table.period'),
        class: 'min-w-[170px] text-xs',
        render: (r) => (
          <div class="space-y-0.5">
            {line(t('admin.coupons.period.startsAt'), formatDate(r.starts_at))}
            {line(t('admin.coupons.period.endsAt'), formatDate(r.ends_at))}
          </div>
        ),
      },
      {
        key: 'status',
        title: t('admin.coupons.table.status'),
        render: (r) => (
          <Badge tone={r.is_active ? 'success' : 'neutral'} dot>
            {r.is_active ? t('admin.common.enabled') : t('admin.common.disabled')}
          </Badge>
        ),
      },
      {
        key: 'action',
        title: t('admin.coupons.table.action'),
        align: 'right',
        render: (r) => (
          <div class="flex justify-end gap-2">
            <Button size="sm" onClick={() => p.openEdit(r)}>
              {t('admin.coupons.actions.edit')}
            </Button>
            <Button size="sm" variant="danger" onClick={() => p.remove(r)}>
              {t('admin.coupons.actions.delete')}
            </Button>
          </div>
        ),
      },
    ]

    const productSelectOptions = () => [
      { label: t('admin.coupons.filterScopeAll'), value: '__all__' },
      ...p.productOpts.products.value.map((prod) => ({
        label: buildProductLabel(prod),
        value: String(prod.id),
      })),
    ]

    const renderScopePicker = () => (
      <div class="space-y-2">
        <div class="flex flex-col gap-2 sm:flex-row sm:items-center">
          <div class="flex-1">
            <Input
              icon={Search}
              v-model={p.productKeyword.value}
              placeholder={t('admin.coupons.modal.scopeSearchPlaceholder')}
              onEnter={() => p.productOpts.load(p.productKeyword.value)}
            />
          </div>
          <Button size="sm" loading={p.productOpts.loading.value} onClick={() => p.productOpts.load(p.productKeyword.value)}>
            {t('admin.coupons.modal.scopeSearchAction')}
          </Button>
        </div>
        <div class="flex flex-wrap items-center gap-2 text-xs text-muted">
          <span>
            {t('admin.coupons.modal.scopeSelectedCount', {
              count: p.selectedScopeIDs.value.length,
            })}
          </span>
          <Button size="xs" variant="ghost" onClick={p.selectAllScope}>
            {t('admin.coupons.modal.scopeSelectAll')}
          </Button>
          <Button size="xs" variant="ghost" onClick={p.clearScope}>
            {t('admin.coupons.modal.scopeClear')}
          </Button>
        </div>
        <div class="max-h-56 overflow-auto rounded-zs-sm border border-line bg-surface-strong">
          {p.productOpts.loading.value ? (
            <div class="px-3 py-3 text-xs text-muted">{t('admin.common.loading')}</div>
          ) : p.productOpts.products.value.length === 0 ? (
            <div class="px-3 py-3 text-xs text-muted">{t('admin.coupons.modal.scopeEmpty')}</div>
          ) : (
            p.productOpts.products.value.map((prod) => (
              <div key={prod.id} class="border-b border-line px-3 py-2 last:border-b-0 hover:bg-primary-soft/40">
                <Checkbox modelValue={p.isScopeChecked(prod.id)} onUpdate:modelValue={(v) => p.toggleScope(prod.id, v)} label={buildProductLabel(prod)} />
              </div>
            ))
          )}
        </div>
      </div>
    )

    return () => (
      <div class="space-y-6">
        <PageHeader title={t('admin.coupons.title')}>
          {{
            actions: () => (
              <Button variant="primary" onClick={p.openCreate}>
                <Plus class="h-4 w-4" />
                {t('admin.coupons.create')}
              </Button>
            ),
          }}
        </PageHeader>

        <FilterBar cols={4}>
          {{
            default: () => (
              <>
                <Input
                  icon={Search}
                  v-model={p.filters.code}
                  placeholder={t('admin.coupons.filterCode')}
                  onUpdate:modelValue={p.debouncedSearch}
                  onEnter={p.handleSearch}
                />
                <div class="flex gap-2">
                  <div class="min-w-0 flex-1">
                    <Input
                      v-model={p.scopeFilterKeyword.value}
                      placeholder={t('admin.coupons.filterScopeSearchPlaceholder')}
                      onEnter={() => p.productOpts.load(p.scopeFilterKeyword.value)}
                    />
                  </div>
                  <Button loading={p.productOpts.loading.value} onClick={() => p.productOpts.load(p.scopeFilterKeyword.value)}>
                    {t('admin.coupons.filterScopeSearchAction')}
                  </Button>
                </div>
                <Select v-model={p.filters.scopeRefId} onChange={p.handleSearch} options={productSelectOptions()} placeholder={t('admin.coupons.filterScope')} />
                <Select
                  v-model={p.filters.isActive}
                  onChange={p.handleSearch}
                  options={[
                    {
                      label: t('admin.coupons.filterStatusAll'),
                      value: '__all__',
                    },
                    { label: t('admin.common.enabled'), value: 'true' },
                    { label: t('admin.common.disabled'), value: 'false' },
                  ]}
                />
              </>
            ),
            actions: () => (
              <Button size="sm" loading={p.refreshing.value} onClick={p.refresh}>
                <RefreshCw class="h-3.5 w-3.5" />
                {t('admin.common.refresh')}
              </Button>
            ),
          }}
        </FilterBar>

        <div>
          <DataTable
            columns={columns()}
            rows={p.list.items.value}
            rowKey={(r) => r.id}
            loading={p.list.loading.value}
            emptyText={t('admin.coupons.empty')}
            minWidth="1000px"
          />
          <ListPagination pagination={p.list.pagination.value} onChangePage={p.list.changePage} onChangePageSize={p.list.changePageSize} />
        </div>

        <Dialog
          v-model={p.modal.showModal.value}
          title={p.modal.isEditing.value ? t('admin.coupons.modal.editTitle') : t('admin.coupons.modal.title')}
          size="3xl"
          closeOnOverlay={false}
        >
          {{
            default: () => (
              <form
                class="grid grid-cols-1 gap-4 md:grid-cols-2"
                onSubmit={(e: Event) => {
                  e.preventDefault()
                  p.submit()
                }}
              >
                <FormField label={t('admin.coupons.modal.code')} required error={p.errors.code}>
                  <Input v-model={p.form.code} placeholder="PROMO2026" mono />
                </FormField>
                <FormField label={t('admin.coupons.modal.type')} required error={p.errors.type}>
                  <Select
                    v-model={p.form.type}
                    options={[
                      {
                        label: t('admin.common.discountTypes.percent'),
                        value: 'percent',
                      },
                      {
                        label: t('admin.common.discountTypes.fixed'),
                        value: 'fixed',
                      },
                    ]}
                  />
                </FormField>
                <div class="space-y-3">
                  <FormField label={t('admin.coupons.modal.value')} required error={p.errors.value}>
                    <Input type="number" step="0.01" v-model={p.form.value} placeholder="20" />
                  </FormField>
                  {p.form.type === 'fixed' && <Switch v-model={p.form.per_item_discount} label={t('admin.coupons.modal.perItemDiscount')} />}
                </div>
                <div class="hidden md:block" />
                <div class="md:col-span-2">
                  <FormField label={t('admin.coupons.modal.scope')} required>
                    {renderScopePicker()}
                  </FormField>
                </div>
                <FormField label={t('admin.coupons.modal.minAmount')}>
                  <Input type="number" step="0.01" v-model={p.form.min_amount} placeholder="0" />
                </FormField>
                <FormField label={t('admin.coupons.modal.maxDiscount')}>
                  <Input type="number" step="0.01" v-model={p.form.max_discount} placeholder="0" />
                </FormField>
                <div class="space-y-3">
                  <FormField label={t('admin.coupons.modal.usageLimit')}>
                    <Input type="number" v-model={p.form.usage_limit} placeholder="0" />
                  </FormField>
                  <Switch v-model={p.form.disabled_wholesale_price} label={t('admin.coupons.modal.disabledWholesalePrice')} />
                </div>
                <FormField label={t('admin.coupons.modal.perUserLimit')}>
                  <Input type="number" v-model={p.form.per_user_limit} placeholder="0" />
                </FormField>
                <div class="md:col-span-2">
                  <FormField label={t('admin.coupons.modal.paymentRoles')}>
                    <MultiSelect
                      modelValue={p.form.payment_roles}
                      onUpdate:modelValue={(v) => (p.form.payment_roles = v.map(String))}
                      options={p.paymentRoleOptions.value}
                      placeholder={t('admin.coupons.modal.paymentRolesPlaceholder')}
                      searchable={false}
                    />
                  </FormField>
                </div>
                <div class="md:col-span-2">
                  <FormField label={t('admin.coupons.modal.memberLevels')}>
                    <MultiSelect
                      modelValue={p.form.member_levels}
                      onUpdate:modelValue={(v) => (p.form.member_levels = v.map(Number))}
                      options={p.memberLevelOptions.value}
                      placeholder={t('admin.coupons.modal.memberLevelsPlaceholder')}
                      disabled={p.memberLevelOptions.value.length === 0}
                    />
                  </FormField>
                </div>
                <FormField label={t('admin.coupons.modal.startsAt')}>
                  <DateTimeInput v-model={p.form.starts_at} />
                </FormField>
                <FormField label={t('admin.coupons.modal.endsAt')}>
                  <DateTimeInput v-model={p.form.ends_at} />
                </FormField>
                <div class="md:col-span-2">
                  <Switch v-model={p.form.is_active} label={t('admin.common.enabled')} />
                </div>
                {p.modal.error.value && <p class="rounded-zs-sm bg-danger-soft px-3 py-2 text-sm text-danger-text md:col-span-2">{p.modal.error.value}</p>}
              </form>
            ),
            footer: () => (
              <>
                <Button onClick={p.modal.closeModal}>{t('admin.common.cancel')}</Button>
                <Button variant="primary" loading={p.modal.submitting.value} onClick={p.submit}>
                  {t('admin.common.save')}
                </Button>
              </>
            ),
          }}
        </Dialog>
      </div>
    )
  },
})
