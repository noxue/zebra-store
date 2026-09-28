import { computed, reactive, ref, shallowRef, watch } from 'vue'
import { useRoute } from 'vue-router'
import i18n from '@/i18n'
import { adminAPI } from '@/api/admin'
import type { AdminCoupon, AdminMemberLevel } from '@/api/types'
import { useListPage } from '@/composables/useListPage'
import { useListRefresh } from '@/composables/useListRefresh'
import { useCrudModal } from '@/composables/useCrudModal'
import { rules, useFormValidation } from '@/composables/useFormValidation'
import { cleanParams, getLocalizedText, toDateTimeLocal, toRFC3339 } from '@/utils/format'
import { notifySuccess } from '@/utils/notify'
import { confirmAction } from '@/utils/confirm'
import { normalizeMemberLevels, normalizePaymentRoles, normalizeScopeIDs } from './marketingUtils'
import { useProductOptions } from './useProductOptions'

type NumInput = number | string

export interface CouponForm {
  code: string
  type: string
  value: NumInput
  min_amount: NumInput
  max_discount: NumInput
  usage_limit: NumInput
  per_user_limit: NumInput
  disabled_wholesale_price: boolean
  per_item_discount: boolean
  payment_roles: string[]
  member_levels: number[]
  starts_at: string
  ends_at: string
  is_active: boolean
}

const emptyForm = (): CouponForm => ({
  code: '',
  type: 'percent',
  value: 0,
  min_amount: 0,
  max_discount: 0,
  usage_limit: 0,
  per_user_limit: 0,
  disabled_wholesale_price: false,
  per_item_discount: false,
  payment_roles: [],
  member_levels: [],
  starts_at: '',
  ends_at: '',
  is_active: true,
})

const positiveId = (raw: unknown) => {
  const n = Number(raw)
  return Number.isFinite(n) && n > 0 ? Math.floor(n) : 0
}

/** Page logic for 优惠券: list + filters (+ deep link ?id / ?coupon_id / ?code / ?scope_ref_id) and the create/edit dialog. */
export function useCoupons() {
  const t = i18n.global.t
  const route = useRoute()

  const filters = reactive({ id: '', code: '', scopeRefId: '__all__' as string | number, isActive: '__all__' })
  const scopeFilterKeyword = ref('')
  const productKeyword = ref('')
  const selectedScopeIDs = ref<number[]>([])
  const memberLevels = shallowRef<AdminMemberLevel[]>([])
  let autoOpenId: number | null = null

  const productOpts = useProductOptions(() => [...selectedScopeIDs.value, positiveId(filters.scopeRefId)].filter((id) => id > 0))

  const list = useListPage<AdminCoupon>({
    fetchFn: (page, pageSize) =>
      adminAPI.getCoupons(
        cleanParams({
          page,
          page_size: pageSize,
          id: filters.id,
          code: filters.code,
          scope_ref_id: filters.scopeRefId,
          is_active: filters.isActive,
        }),
      ),
  })

  const fetch = async (page = 1) => {
    await list.fetchData(page)
    if (autoOpenId) {
      const target = list.items.value.find((c) => c.id === autoOpenId)
      autoOpenId = null
      if (target) openEdit(target)
    }
  }
  const handleSearch = () => void fetch(1)
  let debounceTimer: ReturnType<typeof setTimeout> | undefined
  const debouncedSearch = () => {
    if (debounceTimer) clearTimeout(debounceTimer)
    debounceTimer = setTimeout(handleSearch, 300)
  }
  const { refreshing, refreshList } = useListRefresh()
  const refresh = () => refreshList(() => fetch(list.pagination.value.page))

  const applyRouteFilter = () => {
    const rawCode = route.query.code
    filters.code = typeof rawCode === 'string' ? rawCode.trim() : ''
    const id = positiveId(route.query.id || route.query.coupon_id)
    filters.id = id ? String(id) : ''
    autoOpenId = id || null
    const scope = positiveId(route.query.scope_ref_id)
    filters.scopeRefId = scope ? String(scope) : '__all__'
  }

  const loadMemberLevels = async () => {
    try {
      const res = await adminAPI.getMemberLevels({ page: 1, page_size: 200 })
      memberLevels.value = Array.isArray(res.data) ? res.data : []
    } catch {
      memberLevels.value = []
    }
  }

  const init = async () => {
    applyRouteFilter()
    await Promise.all([productOpts.load(), loadMemberLevels()])
    await fetch(1)
  }

  watch(
    () => [route.query.id, route.query.coupon_id, route.query.code, route.query.scope_ref_id],
    () => {
      applyRouteFilter()
      void fetch(1)
    },
  )

  // --- options & labels ---
  const paymentRoleOptions = computed(() => [
    { value: 'guest', label: t('admin.coupons.paymentRoles.guest') },
    { value: 'member', label: t('admin.coupons.paymentRoles.member') },
  ])
  const memberLevelOptions = computed(() => memberLevels.value.map((l) => ({ value: l.id, label: getLocalizedText(l.name) || `#${l.id}` })))

  const formatPaymentRoles = (raw: unknown) => {
    const roles = normalizePaymentRoles(raw)
    return roles.length ? roles.map((r) => t(`admin.coupons.paymentRoles.${r}`)).join(', ') : '-'
  }
  const formatMemberLevels = (raw: unknown) => {
    const ids = normalizeMemberLevels(raw)
    if (!ids.length) return '-'
    return ids
      .map((id) => {
        const target = memberLevels.value.find((l) => Number(l.id) === id)
        return (target && getLocalizedText(target.name)) || `#${id}`
      })
      .join(', ')
  }

  // --- modal ---
  const form = reactive<CouponForm>(emptyForm())
  const modal = useCrudModal({ onSuccess: () => void fetch(1) })
  const { errors, validate, clearErrors } = useFormValidation<{ code: string; type: string; value: NumInput }>({
    code: [rules.required(t('admin.common.required'))],
    type: [rules.required(t('admin.common.required'))],
    value: [rules.required(t('admin.common.required')), rules.numeric(), rules.min(0)],
  })

  watch(
    () => form.type,
    (type) => {
      if (type !== 'fixed') form.per_item_discount = false
    },
  )

  const openCreate = () => {
    clearErrors()
    Object.assign(form, emptyForm())
    selectedScopeIDs.value = []
    productKeyword.value = ''
    modal.openCreate()
    void productOpts.load()
  }

  function openEdit(c: AdminCoupon) {
    clearErrors()
    const type = c.type || 'percent'
    Object.assign(form, {
      code: c.code || '',
      type,
      value: c.value || 0,
      min_amount: c.min_amount || 0,
      max_discount: c.max_discount || 0,
      usage_limit: c.usage_limit || 0,
      per_user_limit: c.per_user_limit || 0,
      disabled_wholesale_price: Boolean(c.disabled_wholesale_price),
      per_item_discount: type === 'fixed' && Boolean(c.per_item_discount),
      payment_roles: normalizePaymentRoles(c.payment_roles),
      member_levels: normalizeMemberLevels(c.member_levels),
      starts_at: toDateTimeLocal(c.starts_at),
      ends_at: toDateTimeLocal(c.ends_at),
      is_active: Boolean(c.is_active),
    } satisfies CouponForm)
    selectedScopeIDs.value = normalizeScopeIDs(c.scope_ref_ids)
    modal.openEdit(c.id)
    void productOpts.load()
  }

  // scope multi-select
  const isScopeChecked = (id: number) => selectedScopeIDs.value.includes(id)
  const toggleScope = (id: number, checked: boolean) => {
    selectedScopeIDs.value = checked
      ? Array.from(new Set([...selectedScopeIDs.value, id])).sort((a, b) => a - b)
      : selectedScopeIDs.value.filter((x) => x !== id)
  }
  const selectAllScope = () => {
    const ids = productOpts.products.value.map((p) => Number(p.id)).filter((id) => id > 0)
    selectedScopeIDs.value = Array.from(new Set([...selectedScopeIDs.value, ...ids])).sort((a, b) => a - b)
  }
  const clearScope = () => {
    selectedScopeIDs.value = []
  }

  const submit = () => {
    modal.error.value = ''
    if (!validate({ code: form.code, type: form.type, value: form.value })) return
    const scopeIDs = normalizeScopeIDs(selectedScopeIDs.value)
    if (!scopeIDs.length) {
      modal.error.value = t('admin.coupons.errors.scopeRequired')
      return
    }
    void modal.handleSubmit(async () => {
      // The backend expects JSON numbers for the money fields here (string amounts are rejected).
      const payload = {
        code: form.code.trim(),
        type: form.type,
        value: Number(form.value),
        scope_ref_ids: scopeIDs,
        min_amount: Number(form.min_amount || 0),
        max_discount: Number(form.max_discount || 0),
        usage_limit: Number(form.usage_limit || 0),
        per_user_limit: Number(form.per_user_limit || 0),
        disabled_wholesale_price: form.disabled_wholesale_price,
        per_item_discount: form.type === 'fixed' ? form.per_item_discount : false,
        payment_roles: normalizePaymentRoles(form.payment_roles),
        member_levels: normalizeMemberLevels(form.member_levels),
        starts_at: toRFC3339(form.starts_at) ?? '',
        ends_at: toRFC3339(form.ends_at) ?? '',
        is_active: form.is_active,
      }
      if (modal.isEditing.value && modal.editingId.value) await adminAPI.updateCoupon(modal.editingId.value, payload)
      else await adminAPI.createCoupon(payload)
      notifySuccess(t('admin.common.operationSuccess'))
    })
  }

  const remove = async (c: AdminCoupon) => {
    const ok = await confirmAction({ description: t('admin.coupons.confirmDelete', { code: c.code }), confirmText: t('admin.common.delete'), variant: 'destructive' })
    if (!ok) return
    try {
      await adminAPI.deleteCoupon(c.id)
      notifySuccess(t('admin.common.operationSuccess'))
      await fetch(list.pagination.value.page)
    } catch {
      /* already notified */
    }
  }

  return {
    filters,
    list,
    fetch,
    handleSearch,
    debouncedSearch,
    refreshing,
    refresh,
    init,
    scopeFilterKeyword,
    productKeyword,
    productOpts,
    selectedScopeIDs,
    memberLevels,
    paymentRoleOptions,
    memberLevelOptions,
    formatPaymentRoles,
    formatMemberLevels,
    form,
    modal,
    errors,
    openCreate,
    openEdit,
    isScopeChecked,
    toggleScope,
    selectAllScope,
    clearScope,
    submit,
    remove,
  }
}
