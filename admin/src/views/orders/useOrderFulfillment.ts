import { reactive, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { adminAPI } from '@/api/admin'
import type { AdminOrder } from '@/api/types'
import { errorMessage } from '@/api/client'
import { buildDeliveryDataPayload, type DeliveryEntry } from './orderUtils'

/** Manual fulfillment form: note + key/value delivery entries → POST /admin/fulfillments. */
export function useOrderFulfillment(onSuccess: () => void) {
  const { t } = useI18n()
  const loading = ref(false)
  const loadError = ref('')
  const order = ref<AdminOrder | null>(null)
  const submitting = ref(false)
  const error = ref('')
  const success = ref('')
  const form = reactive({ note: '', entries: [{ key: '', value: '' }] as DeliveryEntry[] })
  let seq = 0

  const reset = () => {
    form.note = ''
    form.entries = [{ key: '', value: '' }]
    error.value = ''
    success.value = ''
  }

  const addEntry = () => form.entries.push({ key: '', value: '' })
  const removeEntry = (index: number) => form.entries.splice(index, 1)

  const fetchOrder = async (id: number) => {
    const current = ++seq
    loading.value = true
    loadError.value = ''
    order.value = null
    try {
      const res = await adminAPI.getOrder(id)
      if (current === seq) order.value = res.data
    } catch (err) {
      if (current === seq) loadError.value = errorMessage(err, t('admin.orders.detailFetchFailed'))
    } finally {
      if (current === seq) loading.value = false
    }
  }

  const open = (seed: AdminOrder | null) => {
    reset()
    if (seed?.id) void fetchOrder(seed.id)
  }

  const close = () => {
    seq++
    order.value = null
    loadError.value = ''
    loading.value = false
    reset()
  }

  const submit = async () => {
    if (!order.value) return
    error.value = ''
    success.value = ''
    const deliveryData = buildDeliveryDataPayload(form.note, form.entries)
    if (Object.keys(deliveryData).length === 0) {
      error.value = t('admin.orders.fulfillmentSubmitRequired')
      return
    }
    submitting.value = true
    try {
      await adminAPI.createFulfillment({ order_id: order.value.id, delivery_data: deliveryData })
      success.value = t('admin.orders.fulfillmentSuccess')
      onSuccess()
    } catch (err) {
      error.value = errorMessage(err, t('admin.orders.fulfillmentFailed'))
    } finally {
      submitting.value = false
    }
  }

  return { loading, loadError, order, submitting, error, success, form, reset, addEntry, removeEntry, open, close, submit }
}
