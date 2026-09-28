import { defineComponent } from 'vue'
import { useI18n } from 'vue-i18n'
import { CreditCard, XCircle } from 'lucide-vue-next'
import { OrderBody, OrderBodySkeleton } from '@/components/order/OrderBody'
import { Breadcrumb, Button, Card, EmptyState, SectionTitle } from '@/components/ui'
import { useOrderDetail } from '@/composables/useOrderDetail'
import { usePageTitle } from '@/composables/usePageTitle'

export default defineComponent({
  name: 'OrderDetailView',
  setup() {
    const { t } = useI18n()
    const d = useOrderDetail()
    usePageTitle(() => t('orderDetail.title'))
    return () => (
      <div class="zs-page py-8 sm:py-10">
        <div class="mb-5">
          <Breadcrumb items={[{ label: t('nav.home'), to: '/' }, { label: t('orders.title'), to: '/me/orders' }, { label: t('orderDetail.title') }]} />
        </div>
        <div class="mb-8">
          <SectionTitle as="h1" size="xl" title={t('orderDetail.title')} subtitle={t('orderDetail.subtitle')} />
        </div>
        {d.loading.value ? (
          <OrderBodySkeleton />
        ) : !d.order.value ? (
          <Card>
            <EmptyState variant="error" title={t('orderDetail.notFound')}>
              <Button variant="secondary" onClick={() => void d.loadOrder()}>
                {t('errorBoundary.retry')}
              </Button>
            </EmptyState>
          </Card>
        ) : (
          <OrderBody order={d.order.value} helpers={d.helpers} downloading={d.fulfillmentDownloading.value} onDownload={(no: string) => void d.handleDownloadFulfillment(no)}>
            {{
              actions: () =>
                d.order.value?.status === 'pending_payment' ? (
                  <>
                    <Button size="sm" to={{ path: '/pay', query: { order_no: d.order.value.order_no } }}>
                      <CreditCard class="size-4" />
                      {t('orderDetail.payNow')}
                    </Button>
                    <Button size="sm" variant="danger" onClick={() => void d.cancelOrder()}>
                      <XCircle class="size-4" />
                      {t('orderDetail.cancel')}
                    </Button>
                  </>
                ) : null,
            }}
          </OrderBody>
        )}
      </div>
    )
  },
})
