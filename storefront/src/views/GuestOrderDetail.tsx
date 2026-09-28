import { defineComponent } from 'vue'
import { RouterLink } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { ArrowLeft, CreditCard } from 'lucide-vue-next'
import { GuestAuthForm } from '@/components/order/GuestAuthForm'
import { OrderBody, OrderBodySkeleton } from '@/components/order/OrderBody'
import { Button, Card, EmptyState, SectionTitle } from '@/components/ui'
import { useGuestOrderDetail } from '@/composables/useGuestOrderDetail'
import { usePageTitle } from '@/composables/usePageTitle'

export default defineComponent({
  name: 'GuestOrderDetailView',
  setup() {
    const { t } = useI18n()
    const d = useGuestOrderDetail()
    usePageTitle(() => t('guestOrderDetail.title'))
    return () => (
      <div class="zs-page py-8 sm:py-10">
        <div class="mb-8 flex flex-wrap items-end justify-between gap-4">
          <SectionTitle as="h1" size="xl" title={t('guestOrderDetail.title')} subtitle={t('guestOrderDetail.subtitle')} />
          <RouterLink to="/guest/orders" class="inline-flex items-center gap-1 text-sm font-bold text-muted transition hover:text-primary-text">
            <ArrowLeft class="size-4" />
            {t('guestOrderDetail.backSearch')}
          </RouterLink>
        </div>
        {d.viewState.value === 'auth' ? (
          <GuestAuthForm
            v-model={d.auth.value}
            title={t('guestOrderDetail.authTitle')}
            hint={t('guestOrderDetail.authHint')}
            error={d.authError.value}
            submitText={t('guestOrderDetail.authSubmit')}
            clearText={t('guestOrderDetail.authClear')}
            onSubmit={() => void d.handleAuthSubmit()}
            onClear={d.clearAuth}
          />
        ) : d.viewState.value === 'loading' ? (
          <OrderBodySkeleton />
        ) : d.viewState.value === 'empty' || !d.order.value ? (
          <Card>
            <EmptyState variant="error" title={t('guestOrderDetail.notFound')} />
          </Card>
        ) : (
          <OrderBody order={d.order.value} helpers={d.helpers} downloading={d.fulfillmentDownloading.value} onDownload={(no: string) => void d.handleDownloadFulfillment(no)}>
            {{
              actions: () =>
                d.order.value?.status === 'pending_payment' ? (
                  <Button size="sm" to={{ path: '/pay', query: { guest: '1', order_no: d.order.value.order_no } }}>
                    <CreditCard class="size-4" />
                    {t('orders.payNow')}
                  </Button>
                ) : null,
            }}
          </OrderBody>
        )}
      </div>
    )
  },
})
