import { defineComponent, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { ArrowDown, ArrowUp, ChevronsUpDown, RefreshCw, RotateCcw, Search } from 'lucide-vue-next'
import {
  Badge,
  Button,
  DataTable,
  Dialog,
  FilterBar,
  FormField,
  IdCell,
  Input,
  ListPagination,
  PageHeader,
  RangeFilter,
  Select,
  Textarea,
  type DataTableColumn,
} from '@/components/ui'
import type { AdminUser } from '@/api/types'
import { toRowSelection } from '@/composables/useSelection'
import { formatDate, formatMoney } from '@/utils/format'
import { userStatusLabel, userStatusTone } from '@/utils/status'
import { LinkButton } from './components/LinkButton'
import { formatLocale, LOCALE_OPTIONS, memberLevelLabel, type UserSortColumn } from './usersUtils'
import { useUsers } from './useUsers'

export default defineComponent({
  name: 'UsersView',
  setup() {
    const { t } = useI18n()
    const p = useUsers()
    onMounted(() => void p.init())

    const sortHeader = (column: UserSortColumn, label: string) => {
      const active = p.sort.by === column
      const Icon = active ? (p.sort.order === 'asc' ? ArrowUp : ArrowDown) : ChevronsUpDown
      return (
        <button
          type="button"
          class={['inline-flex items-center gap-1 transition-colors hover:text-primary', active && 'text-primary']}
          aria-sort={active ? (p.sort.order === 'asc' ? 'ascending' : 'descending') : 'none'}
          onClick={() => p.toggleSort(column)}
        >
          {label}
          <Icon class={['h-3.5 w-3.5', !active && 'opacity-40']} />
        </button>
      )
    }

    const localeOptions = () => LOCALE_OPTIONS.map((l) => ({ label: formatLocale(t, l), value: l }))
    const statusOptions = () => [
      { label: t('admin.users.status.active'), value: 'active' },
      { label: t('admin.users.status.disabled'), value: 'disabled' },
    ]

    const columns = (): DataTableColumn<AdminUser>[] => [
      { key: 'id', title: t('admin.users.table.id'), render: (r) => <IdCell value={r.id} /> },
      { key: 'email', title: t('admin.users.table.email'), class: 'min-w-[140px] break-all', render: (r) => r.email },
      { key: 'nickname', title: t('admin.users.table.nickname'), class: 'min-w-[140px] text-muted break-words', render: (r) => r.display_name || '-' },
      {
        key: 'status',
        title: t('admin.users.table.status'),
        render: (r) => (
          <Badge tone={userStatusTone(r.status)} dot>
            {userStatusLabel(t, r.status)}
          </Badge>
        ),
      },
      { key: 'locale', title: t('admin.users.table.locale'), class: 'text-xs text-muted whitespace-nowrap', render: (r) => formatLocale(t, r.locale) },
      {
        key: 'wallet',
        title: sortHeader('wallet_balance', t('admin.users.table.walletBalance')),
        class: 'zs-num text-xs whitespace-nowrap',
        render: (r) => formatMoney(r.wallet_balance, p.siteCurrency.value),
      },
      {
        key: 'memberLevel',
        title: t('admin.users.table.memberLevel'),
        class: 'min-w-[120px] text-xs break-words',
        render: (r) => memberLevelLabel(r.member_level_id, p.memberLevels.value),
      },
      {
        key: 'createdAt',
        title: sortHeader('created_at', t('admin.users.table.createdAt')),
        class: 'min-w-[88px] text-xs text-muted',
        render: (r) => formatDate(r.created_at) || '-',
      },
      {
        key: 'lastLoginAt',
        title: sortHeader('last_login_at', t('admin.users.table.lastLoginAt')),
        class: 'min-w-[88px] text-xs text-muted',
        render: (r) => formatDate(r.last_login_at) || '-',
      },
      {
        key: 'adminNote',
        title: t('admin.users.table.adminNote'),
        class: 'text-xs text-muted',
        render: (r) => (
          <span class="block max-w-[160px] truncate" title={r.admin_note || ''}>
            {r.admin_note || '-'}
          </span>
        ),
      },
      {
        key: 'action',
        title: t('admin.users.table.action'),
        align: 'right',
        render: (r) => (
          <div class="flex justify-end gap-2">
            <LinkButton to={`/users/${r.id}`}>
              {t('admin.users.actions.detail')}
            </LinkButton>
            <Button size="sm" onClick={() => p.openEdit(r)}>
              {t('admin.users.actions.edit')}
            </Button>
          </div>
        ),
      },
    ]

    return () => (
      <div class="space-y-6">
        <PageHeader title={t('admin.users.title')} />

        <FilterBar cols={4}>
          {{
            default: () => (
              <>
                <Input icon={Search} v-model={p.filters.userId} placeholder={t('admin.users.filterUserId')} onEnter={p.list.handleSearch} onUpdate:modelValue={p.list.debouncedSearch} />
                <div class="sm:col-span-2">
                  <Input v-model={p.filters.keyword} placeholder={t('admin.users.filterKeyword')} onEnter={p.list.handleSearch} onUpdate:modelValue={p.list.debouncedSearch} />
                </div>
                <Select
                  v-model={p.filters.status}
                  onChange={p.list.handleSearch}
                  options={[{ label: t('admin.users.filterStatusAll'), value: '__all__' }, ...statusOptions()]}
                />
                <RangeFilter
                  label={t('admin.users.filterCreatedRange')}
                  from={p.filters.createdFrom}
                  to={p.filters.createdTo}
                  fromPlaceholder={t('admin.users.filterCreatedFrom')}
                  toPlaceholder={t('admin.users.filterCreatedTo')}
                  onUpdate:from={(v: string) => (p.filters.createdFrom = v)}
                  onUpdate:to={(v: string) => (p.filters.createdTo = v)}
                  onChange={p.list.handleSearch}
                />
                <RangeFilter
                  label={t('admin.users.filterLastLoginRange')}
                  from={p.filters.lastLoginFrom}
                  to={p.filters.lastLoginTo}
                  fromPlaceholder={t('admin.users.filterLastLoginFrom')}
                  toPlaceholder={t('admin.users.filterLastLoginTo')}
                  onUpdate:from={(v: string) => (p.filters.lastLoginFrom = v)}
                  onUpdate:to={(v: string) => (p.filters.lastLoginTo = v)}
                  onChange={p.list.handleSearch}
                />
              </>
            ),
            actions: () => (
              <>
                <Button size="sm" variant="ghost" onClick={p.resetFilters}>
                  <RotateCcw class="h-3.5 w-3.5" />
                  {t('admin.common.reset')}
                </Button>
                <Button size="sm" loading={p.refreshing.value} onClick={p.refresh}>
                  <RefreshCw class="h-3.5 w-3.5" />
                  {t('admin.common.refresh')}
                </Button>
              </>
            ),
          }}
        </FilterBar>

        <div>
          <DataTable
            columns={columns()}
            rows={p.list.items.value}
            rowKey={(r) => r.id}
            loading={p.list.loading.value}
            selection={toRowSelection(p.selection)}
            emptyText={t('admin.users.empty')}
            minWidth="1100px"
            dense
          />
          <ListPagination pagination={p.list.pagination.value} onChangePage={p.list.changePage} onChangePageSize={p.list.changePageSize}>
            {{
              actions: () =>
                p.hasSelection.value && (
                  <div class="flex items-center gap-2">
                    <Button size="xs" variant="soft" onClick={() => p.batchUpdateStatus('active')}>
                      {t('admin.users.batch.enable')}
                    </Button>
                    <Button size="xs" variant="danger" onClick={() => p.batchUpdateStatus('disabled')}>
                      {t('admin.users.batch.disable')}
                    </Button>
                  </div>
                ),
            }}
          </ListPagination>
        </div>

        <Dialog v-model={p.modal.showModal.value} title={t('admin.users.modal.editTitle')} size="md">
          {{
            default: () => (
              <form
                class="space-y-4"
                onSubmit={(e: Event) => {
                  e.preventDefault()
                  p.submit()
                }}
              >
                <FormField label={t('admin.users.form.email')} required error={p.errors.email}>
                  <Input v-model={p.form.email} placeholder={t('admin.users.form.emailPlaceholder')} />
                </FormField>
                <FormField label={t('admin.users.form.nickname')} required error={p.errors.nickname}>
                  <Input v-model={p.form.nickname} placeholder={t('admin.users.form.nicknamePlaceholder')} />
                </FormField>
                <FormField label={t('admin.users.form.password')} hint={t('admin.users.form.passwordTip')}>
                  <Input type="password" autocomplete="new-password" v-model={p.form.password} placeholder={t('admin.users.form.passwordPlaceholder')} />
                </FormField>
                <div class="grid grid-cols-1 gap-4 sm:grid-cols-3">
                  <FormField label={t('admin.users.form.locale')}>
                    <Select v-model={p.form.locale} options={localeOptions()} />
                  </FormField>
                  <FormField label={t('admin.users.form.emailVerifiedStatus')}>
                    <Select
                      v-model={p.form.email_verified}
                      options={[
                        { label: t('admin.users.emailVerification.verified'), value: 'verified' },
                        { label: t('admin.users.emailVerification.unverified'), value: 'unverified' },
                      ]}
                    />
                  </FormField>
                  <FormField label={t('admin.users.form.status')}>
                    <Select v-model={p.form.status} options={statusOptions()} />
                  </FormField>
                </div>
                <FormField label={t('admin.users.form.adminNote')}>
                  <Textarea v-model={p.form.admin_note} rows={3} placeholder={t('admin.users.form.adminNotePlaceholder')} />
                </FormField>
                {p.modal.error.value && <p class="rounded-zs-sm bg-danger-soft px-3 py-2 text-xs text-danger-text">{p.modal.error.value}</p>}
                <button type="submit" class="hidden" />
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
