import { computed, reactive, ref, watch } from 'vue'
import { useRoute } from 'vue-router'
import i18n from '@/i18n'
import { adminAPI } from '@/api/admin'
import type { AdminPromotion } from '@/api/types'
import { useListPage } from '@/composables/useListPage'
import { useListRefresh } from '@/composables/useListRefresh'
import { useCrudModal } from '@/composables/useCrudModal'
import { rules, useFormValidation } from '@/composables/useFormValidation'
import { cleanParams, toDateTimeLocal, toRFC3339 } from '@/utils/format'
import { notifySuccess } from '@/utils/notify'
import { confirmAction } from '@/utils/confirm'
import { fixedDiscountRisk, referenceUnitPrice } from './marketingUtils'
import { useProductOptions } from './useProductOptions'

type NumInput = number | string

export interface PromotionForm {
  name: string
  type: string
  scope: string
  value: NumInput
  min_amount: NumInput
  starts_at: string
  ends_at: string
  is_active: boolean
}

const emptyForm = (): PromotionForm => ({ name: '', type: 'percent', scope: '__none__', value: 0, min_amount: 0, starts_at: '', ends_at: '', is_active: true })

const positiveId = (raw: unknown) => {
  const n = Number(raw)
  return Number.isFinite(n) && n > 0 ? Math.floor(n) : 0
}

/** Page logic for 活动价: list + filters (+ deep link ?id / ?promotion_id) and create/edit dialog. */
export function usePromotions() {
  const t = i18n.global.t
  const route = useRoute()

  const filters = reactive({ id: '', name: '', scopeRefId: '__all__' as string | number, isActive: '__all__' })
  const productKeyword = ref('')
  const form = reactive<PromotionForm>(emptyForm())
  let autoOpenId: number | null = null

  const productOpts = useProductOptions(() => [positiveId(filters.scopeRefId), positiveId(form.scope)].filter((id) => id > 0))

  const list = useListPage<AdminPromotion>({
    fetchFn: (page, pageSize) =>
      adminAPI.getPromotions(
        cleanParams({ page, page_size: pageSize, id: filters.id, name: filters.name, scope_ref_id: filters.scopeRefId, is_active: filters.isActive }),
      ),
  })

  const fetch = async (page = 1) => {
    await list.fetchData(page)
    if (autoOpenId) {
      const target = list.items.value.find((x) => x.id === autoOpenId)
      autoOpenId = null
      if (target) openEdit(target)
    }
  }
  const handleSearch = () => void fetch(1)
  const { refreshing, refreshList } = useListRefresh()
  const refresh = () => refreshList(() => fetch(list.pagination.value.page))

  const applyRouteFilter = () => {
    const id = positiveId(route.query.id || route.query.promotion_id)
    if (id) {
      filters.id = String(id)
      autoOpenId = id
    } else filters.id = ''
  }

  const init = async () => {
    applyRouteFilter()
    await productOpts.load(productKeyword.value)
    await fetch(1)
  }

  watch(
    () => [route.query.id, route.query.promotion_id],
    () => {
      applyRouteFilter()
      void fetch(1)
    },
  )

  // --- modal ---
  const modal = useCrudModal({ onSuccess: () => void fetch(1) })
  const { errors, validate, clearErrors } = useFormValidation<{ name: string; type: string; value: NumInput }>({
    name: [rules.required(t('admin.common.required'))],
    type: [rules.required(t('admin.common.required'))],
    value: [rules.required(t('admin.common.required')), rules.numeric(), rules.min(0)],
  })

  const valueHint = computed(() => {
    switch (form.type) {
      case 'percent':
        return t('admin.promotions.modal.valueHintPercent')
      case 'fixed':
        return t('admin.promotions.modal.valueHintFixed')
      case 'special_price':
        return t('admin.promotions.modal.valueHintSpecialPrice')
      default:
        return t('admin.promotions.modal.valueHintDefault')
    }
  })

  const selectedScopeProduct = computed(() => {
    const id = positiveId(form.scope)
    return id ? (productOpts.products.value.find((p) => Number(p.id) === id) ?? null) : null
  })
  const risk = computed(() => fixedDiscountRisk(form.type, form.value, referenceUnitPrice(selectedScopeProduct.value)))

  const openCreate = () => {
    clearErrors()
    Object.assign(form, emptyForm())
    modal.openCreate()
    void productOpts.load(productKeyword.value)
  }

  function openEdit(promo: AdminPromotion) {
    clearErrors()
    Object.assign(form, {
      name: promo.name || '',
      type: promo.type || 'percent',
      scope: promo.scope_ref_id > 0 ? String(promo.scope_ref_id) : '__none__',
      value: promo.value || 0,
      min_amount: promo.min_amount || 0,
      starts_at: toDateTimeLocal(promo.starts_at),
      ends_at: toDateTimeLocal(promo.ends_at),
      is_active: Boolean(promo.is_active),
    } satisfies PromotionForm)
    modal.openEdit(promo.id)
    void productOpts.load(productKeyword.value)
  }

  const submit = () => {
    modal.error.value = ''
    if (!validate({ name: form.name, type: form.type, value: form.value })) return
    const scopeRefId = positiveId(form.scope)
    if (!scopeRefId) {
      modal.error.value = t('admin.promotions.errors.scopeRequired')
      return
    }
    void modal.handleSubmit(async () => {
      // Numbers on the wire: the backend rejects string amounts for promotions.
      const payload = {
        name: form.name.trim(),
        type: form.type,
        scope_ref_id: scopeRefId,
        value: Number(form.value),
        min_amount: Number(form.min_amount || 0),
        starts_at: toRFC3339(form.starts_at) ?? '',
        ends_at: toRFC3339(form.ends_at) ?? '',
        is_active: form.is_active,
      }
      if (modal.isEditing.value && modal.editingId.value) await adminAPI.updatePromotion(modal.editingId.value, payload)
      else await adminAPI.createPromotion(payload)
      notifySuccess(t('admin.common.operationSuccess'))
    })
  }

  const remove = async (promo: AdminPromotion) => {
    const ok = await confirmAction({ description: t('admin.promotions.confirmDelete', { name: promo.name }), confirmText: t('admin.common.delete'), variant: 'destructive' })
    if (!ok) return
    try {
      await adminAPI.deletePromotion(promo.id)
      notifySuccess(t('admin.common.operationSuccess'))
      await fetch(list.pagination.value.page)
    } catch {
      /* already notified */
    }
  }

  return { filters, list, fetch, handleSearch, refreshing, refresh, init, productKeyword, productOpts, form, modal, errors, valueHint, risk, openCreate, openEdit, submit, remove }
}
