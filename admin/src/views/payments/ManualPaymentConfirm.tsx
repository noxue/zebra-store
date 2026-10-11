import { defineComponent, onMounted, ref } from 'vue'
import { useRoute } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { Button, Input, PageHeader } from '@/components/ui'
import { manualPaymentAPI } from '@/api/manualPayment'
import type { ManualPaymentReview } from '@/api/types'

/** Human review page reached from a notification link. Its fragment credential
 * never enters HTTP URLs; both human and monitoring clients call the same API. */
export default defineComponent({
  name: 'ManualPaymentConfirm',
  setup() {
    const { t, locale } = useI18n()
    const route = useRoute()
    const paymentId = Number(route.params.id)
    const key = new URLSearchParams(window.location.hash.slice(1)).get('key') ?? ''
    const review = ref<ManualPaymentReview | null>(null)
    const received = ref('')
    const note = ref('')
    const acknowledged = ref(false)
    const loading = ref(false)
    const error = ref('')
    const done = ref(false)
    const m = (part: string) => t(`manualPayment.${part}`)
    const load = async () => {
      if (!Number.isSafeInteger(paymentId) || paymentId <= 0 || !key) { error.value = m('invalidLink'); return }
      loading.value = true
      try {
        review.value = await manualPaymentAPI.review(paymentId, key)
        received.value = review.value.amount
        error.value = ''
      } catch (err) { error.value = err instanceof Error ? err.message : m('loadFailed') }
      finally { loading.value = false }
    }
    const confirm = async (force: boolean) => {
      if (!acknowledged.value || !review.value || loading.value) return
      loading.value = true
      try {
        review.value = await manualPaymentAPI.confirm(paymentId, key, received.value, force, note.value)
        done.value = true
        error.value = ''
      } catch (err) { error.value = err instanceof Error ? err.message : m('confirmFailed') }
      finally { loading.value = false }
    }
    onMounted(() => { void load() })
    return () => (
      <main class="min-h-screen bg-bg px-4 py-8 text-fg sm:py-14">
        <div class="mx-auto max-w-2xl space-y-6">
          <PageHeader title={m('title')} subtitle={m('subtitle')} />
          {error.value && <div class="rounded-zs border border-danger-text bg-danger-soft p-4 text-sm text-danger-text">{error.value}</div>}
          {loading.value && !review.value && <div class="rounded-zs border border-line bg-surface-strong p-6 text-sm text-muted">{m('loading')}</div>}
          {review.value && <section class="space-y-5 rounded-zs-lg border border-line bg-surface-strong p-5 shadow-zs sm:p-7">
            <div class="flex flex-wrap items-center justify-between gap-3">
              <div>
                <div class="text-xs text-muted">{m('orderNo')}</div>
                <div class="break-all font-mono text-base font-bold">{review.value.order_no}</div>
              </div>
              <span class="rounded-full bg-primary-soft px-3 py-1 text-xs font-semibold text-primary">{review.value.channel_name}</span>
            </div>
            <div class="rounded-zs border border-line bg-surface-muted p-4">
              <div class="text-xs text-muted">{m('amount')}</div>
              <div class="zs-num mt-1 text-3xl font-bold text-primary">{review.value.amount} <span class="text-base">{review.value.currency}</span></div>
              <p class="mt-2 text-xs text-muted">{m('amountHint')}</p>
            </div>
            <div>
              <div class="mb-2 text-sm font-semibold">{m('items')}</div>
              <ul class="space-y-2">
                {review.value.items.map((item, index) => <li key={index} class="flex justify-between gap-4 rounded-zs bg-surface-muted p-3 text-sm">
                  <span>{item.title[locale.value] || item.title['zh-CN'] || Object.values(item.title)[0] || '-'}</span>
                  <span class="shrink-0">× {item.quantity}</span>
                </li>)}
              </ul>
            </div>
            <div class="text-xs text-muted">{m('status')}: {t(`order.status.${review.value.order_status}`)} / {t(`payment.status.${review.value.payment_status}`)}</div>
            {review.value.buyer_report && <div class="rounded-zs border border-warning-text bg-warning-soft p-4 text-sm text-warning-text">
              <div class="font-bold">{m('buyerReport')}</div>
              <div class="mt-2">{m('payerName')}：{review.value.buyer_report.payer_name}</div>
              <div>{m('issue')}：{review.value.buyer_report.issue === 'wrong_amount' ? m('issueWrongAmount') : m('issueNotReceived')}</div>
              {review.value.buyer_report.note && <div class="whitespace-pre-wrap break-words">{m('note')}：{review.value.buyer_report.note}</div>}
              <div class="mt-1 text-xs">{m('reportedAt')}：{review.value.buyer_report.reported_at}</div>
            </div>}
            {review.value.requires_force && <div class="rounded-zs border border-warning-text bg-warning-soft p-4 text-sm text-warning-text">{m('closedWarning')}</div>}
            {review.value.can_confirm && !done.value && <div class="space-y-4 border-t border-line pt-4">
              <label class="block text-sm font-medium">{m('receivedAmount')}<Input class="mt-2" modelValue={received.value} onUpdate:modelValue={(v: string | number) => { received.value = String(v) }} /></label>
              {review.value.requires_force && <label class="block text-sm font-medium">{m('forceReason')}<Input class="mt-2" modelValue={note.value} onUpdate:modelValue={(v: string | number) => { note.value = String(v) }} /></label>}
              <label class="flex items-start gap-2 text-sm"><input type="checkbox" class="mt-1" checked={acknowledged.value} onChange={(e: Event) => { acknowledged.value = (e.target as HTMLInputElement).checked }} />{m('verified')}</label>
              <div class="flex flex-wrap gap-3">
                {!review.value.requires_force && <Button variant="primary" disabled={!acknowledged.value || loading.value} onClick={() => void confirm(false)}>{m('confirm')}</Button>}
                {review.value.requires_force && <Button variant="primary" disabled={!acknowledged.value || !note.value.trim() || loading.value} onClick={() => void confirm(true)}>{m('forceConfirm')}</Button>}
              </div>
            </div>}
            {done.value && <div class="rounded-zs bg-success-soft p-4 text-sm text-success-text">{m('success')}</div>}
          </section>}
        </div>
      </main>
    )
  },
})
