import { computed, defineComponent, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { ImagePlus, Send, Trash2 } from 'lucide-vue-next'
import type { ResellerWithdrawData } from '@/api/types'
import { useResellerWithdraws } from '@/composables/reseller/useResellerWithdraws'
import { ResellerAlert, ResellerPageHeader } from '@/components/reseller/ConsoleParts'
import { Alert, Badge, Button, Card, DataTable, Field, Input, Pagination, Select, columns } from '@/components/ui'
import { formatResellerConsoleAmount, formatResellerConsoleDate } from '@/utils/reseller/console'
import { getResellerWithdrawStatusKey, withdrawStatusTone } from '@/utils/reseller/finance'
import { getImageUrl } from '@/utils/image'

export default defineComponent({
  name: 'ResellerWithdraws',
  setup() {
    const { t } = useI18n()
    const w = useResellerWithdraws()
    const qrInput = ref<HTMLInputElement | null>(null)
    const k = (key: string) => t(`personalCenter.reseller.${key}`)
    onMounted(() => void w.load())
    const statusLabel = (s?: string) => {
      const key = getResellerWithdrawStatusKey(s)
      return key ? k(`withdrawStatus.${key}`) : s || '-'
    }
    const currencyOptions = computed(() => w.currencies.value.map((c) => ({ value: c, label: c })))
    const onQrFile = (event: Event) => {
      const input = event.target as HTMLInputElement
      const file = input.files?.[0]
      input.value = ''
      if (file) void w.uploadQr(file)
    }
    const payoutDetails = (row: ResellerWithdrawData) => (
      <div class="min-w-0 space-y-1 text-sm">
        <div class="font-bold">{row.channel}</div>
        <div class="break-all text-muted">{row.account}</div>
        {row.payee_name && <div class="break-words">{t('resellerConsole.withdraws.payeeName')}: {row.payee_name}</div>}
        {row.qr_code_url && <a href={getImageUrl(row.qr_code_url)} target="_blank" rel="noopener noreferrer" class="text-primary-text underline">{t('resellerConsole.withdraws.viewQr')}</a>}
      </div>
    )
    const withdrawColumns = columns<ResellerWithdrawData>([
      { key: 'amount', title: k('withdrawTable.amount'), render: (row) => <span class="whitespace-nowrap zs-num font-bold">{formatResellerConsoleAmount(row.amount, row.currency)}</span> },
      {
        key: 'channel',
        title: k('withdrawTable.channel'),
        render: payoutDetails,
      },
      {
        key: 'status',
        title: k('withdrawTable.status'),
        render: (row) => (
          <div class="space-y-1">
            <Badge tone={withdrawStatusTone(row.status)}>{statusLabel(row.status)}</Badge>
            {row.reject_reason && <div class="text-xs text-danger-text">{row.reject_reason}</div>}
          </div>
        ),
      },
      { key: 'created_at', title: k('withdrawTable.createdAt'), render: (row) => <span class="whitespace-nowrap text-xs text-muted">{formatResellerConsoleDate(row.created_at)}</span> },
      { key: 'processed_at', title: k('withdrawTable.processedAt'), render: (row) => <span class="whitespace-nowrap text-xs text-muted">{formatResellerConsoleDate(row.processed_at)}</span> },
    ])

    return () => (
      <div class="space-y-5">
        <ResellerPageHeader title={t('resellerConsole.withdraws.title')} description={t('resellerConsole.withdraws.description')} />
        <ResellerAlert alert={w.alert.value} />
        <div class="grid gap-5 lg:grid-cols-[380px_minmax(0,1fr)]">
          <Card>
            <h3 class="zs-title text-lg text-fg">{k('withdrawTitle')}</h3>
            <p class="mb-4 mt-1 text-xs text-muted">{k('withdrawSubtitle')}</p>
            {!w.withdrawEnabled.value && !w.dashboardLoading.value && (
              <div class="mb-4">
                <Alert tone="warning">{w.disabledReasonText.value}</Alert>
              </div>
            )}
            <form
              class="space-y-4"
              onSubmit={(e: Event) => {
                e.preventDefault()
                void w.submit()
              }}
            >
              <Field label={k('withdrawCurrencyLabel')} hint={w.currencies.value.length ? '' : t('resellerConsole.withdraws.noCurrency')}>
                {w.currencies.value.length ? (
                  <Select v-model={w.form.currency} options={currencyOptions.value} />
                ) : (
                  <Input v-model={w.form.currency} placeholder={t('resellerConsole.common.currencyPlaceholder')} />
                )}
              </Field>
              <Field
                label={k('withdrawAmountLabel')}
                error={w.amountExceeded.value ? t('resellerConsole.withdraws.exceedAvailable') : ''}
                hint={w.selectedAvailable.value !== null ? `${t('resellerConsole.withdraws.available')}: ${formatResellerConsoleAmount(w.selectedAvailable.value, w.form.currency)}` : ''}
              >
                <Input v-model={w.form.amount} inputmode="decimal" placeholder={k('withdrawAmountPlaceholder')} invalid={w.amountExceeded.value} />
              </Field>
              <Field label={k('withdrawChannelLabel')}>
                <Input v-model={w.form.channel} placeholder={k('withdrawChannelPlaceholder')} />
                <div class="mt-2 flex flex-wrap gap-2">
                  {(['alipay', 'wechat'] as const).map((channel) => (
                    <Button key={channel} type="button" size="xs" variant={w.form.channel === channel ? 'primary' : 'soft'} onClick={() => (w.form.channel = channel)}>
                      {t(`resellerConsole.withdraws.channels.${channel}`)}
                    </Button>
                  ))}
                </div>
              </Field>
              <Field label={k('withdrawAccountLabel')}>
                <Input v-model={w.form.account} placeholder={k('withdrawAccountPlaceholder')} />
              </Field>
              <Field label={t('resellerConsole.withdraws.payeeName')}>
                <Input v-model={w.form.payee_name} maxlength={100} placeholder={t('resellerConsole.withdraws.payeeNamePlaceholder')} />
              </Field>
              <Field label={t('resellerConsole.withdraws.qrCode')} hint={t('resellerConsole.withdraws.qrHint')}>
                <div class="flex flex-wrap items-center gap-3">
                  {w.form.qr_code_url && <img src={getImageUrl(w.form.qr_code_url)} alt={t('resellerConsole.withdraws.qrCode')} class="size-24 rounded-zs border border-line object-contain" />}
                  <input ref={qrInput} type="file" accept="image/*" class="hidden" onChange={onQrFile} />
                  <Button type="button" size="sm" variant="soft" loading={w.qrUploading.value} onClick={() => qrInput.value?.click()}>
                    <ImagePlus class="size-4" />{t('resellerConsole.withdraws.uploadQr')}
                  </Button>
                  {w.form.qr_code_url && <Button type="button" size="sm" variant="ghost" onClick={() => (w.form.qr_code_url = '')}><Trash2 class="size-4" />{t('resellerConsole.withdraws.removeQr')}</Button>}
                </div>
                {w.qrUploadError.value && <p class="mt-2 text-xs text-danger-text">{w.qrUploadError.value}</p>}
              </Field>
              <Button type="submit" block loading={w.submittingWithdraw.value} disabled={!w.withdrawEnabled.value || w.amountExceeded.value}>
                <Send class="size-4" />
                {w.submittingWithdraw.value ? k('withdrawing') : k('withdrawSubmit')}
              </Button>
            </form>
          </Card>
          <Card>
            <h3 class="zs-title mb-4 text-lg text-fg">{k('withdrawRecordTitle')}</h3>
            <div class="space-y-3 md:hidden">
              {!w.withdrawsLoading.value && w.withdraws.value.length === 0 && <p class="py-8 text-center text-sm text-muted">{k('withdrawEmpty')}</p>}
              {w.withdraws.value.map((row) => (
                <div key={row.id} class="rounded-zs border border-line bg-surface-muted p-4">
                  <div class="flex items-start justify-between gap-3">
                    <strong class="zs-num text-base">{formatResellerConsoleAmount(row.amount, row.currency)}</strong>
                    <Badge tone={withdrawStatusTone(row.status)}>{statusLabel(row.status)}</Badge>
                  </div>
                  <div class="mt-3">{payoutDetails(row)}</div>
                  {row.reject_reason && <p class="mt-2 text-xs text-danger-text">{row.reject_reason}</p>}
                  <p class="mt-3 text-xs text-muted">{formatResellerConsoleDate(row.created_at)}</p>
                </div>
              ))}
            </div>
            <div class="hidden md:block"><DataTable columns={withdrawColumns} rows={w.withdraws.value} loading={w.withdrawsLoading.value} emptyText={k('withdrawEmpty')} /></div>
            <div class="mt-4">
              <Pagination page={w.withdrawsPagination.page} totalPages={w.withdrawsPagination.total_page} onChange={(pg: number) => void w.loadWithdraws({ page: pg })} />
            </div>
          </Card>
        </div>
      </div>
    )
  },
})
