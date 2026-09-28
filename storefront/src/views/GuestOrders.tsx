import { defineComponent } from 'vue'
import { useI18n } from 'vue-i18n'
import { ClipboardList, CreditCard, Eye, Search } from 'lucide-vue-next'
import { OrderStatusBadge } from '@/components/common/OrderStatusBadge'
import { Alert, Button, Card, EmptyState, Input, Kitty, Pagination, SectionTitle } from '@/components/ui'
import { useGuestOrders } from '@/composables/useGuestOrders'
import { usePageTitle } from '@/composables/usePageTitle'

export default defineComponent({
  name: 'GuestOrdersView',
  setup() {
    const { t } = useI18n()
    const g = useGuestOrders()
    usePageTitle(() => t('guestOrders.title'))
    return () => (
      <div class="zs-page py-8 sm:py-10">
        <div class="mb-8">
          <SectionTitle as="h1" size="xl" title={t('guestOrders.title')} subtitle={t('guestOrders.subtitle')} />
        </div>
        <Card class="mb-8">
          <div class="flex items-start gap-4">
            <div class="hidden h-24 w-28 shrink-0 md:block">
              <Kitty />
            </div>
            <form
              class="min-w-0 flex-1"
              onSubmit={(e: Event) => {
                e.preventDefault()
                g.handleSearch()
              }}
            >
              {g.hasSavedAuth.value && (
                <div class="mb-4 flex flex-col gap-2 rounded-zs border border-line bg-surface-muted px-4 py-2.5 text-xs text-muted md:flex-row md:items-center md:justify-between">
                  <span>{t('guestOrders.savedHint', { email: g.savedAuth.value.email || '-' })}</span>
                  <Button variant="link" size="xs" onClick={g.clearSaved}>
                    {t('guestOrders.clearSaved')}
                  </Button>
                </div>
              )}
              <div class="grid grid-cols-1 gap-3 md:grid-cols-[1fr_1fr_1fr_auto]">
                <Input v-model={g.email.value} type="email" autocomplete="email" placeholder={t('guestOrders.emailPlaceholder')} />
                <Input v-model={g.orderPassword.value} type="password" autocomplete="current-password" placeholder={t('guestOrders.passwordPlaceholder')} />
                <Input v-model={g.orderNo.value} placeholder={t('guestOrders.orderNoPlaceholder')} />
                <Button type="submit" loading={g.loading.value}>
                  <Search class="size-4" />
                  {g.loading.value ? t('guestOrders.searching') : t('guestOrders.search')}
                </Button>
              </div>
              <p class="mt-3 flex items-center gap-1.5 text-xs text-muted">
                <ClipboardList class="size-3.5" />
                {t('guestOrders.tip')}
              </p>
              {g.error.value && (
                <div class="mt-4">
                  <Alert tone="error">{g.error.value}</Alert>
                </div>
              )}
            </form>
          </div>
        </Card>

        {g.loading.value && g.orders.value.length === 0 ? (
          <div class="space-y-4">
            {[0, 1].map((i) => (
              <div key={i} class="zs-card h-28 p-6">
                <div class="zs-skeleton h-full" />
              </div>
            ))}
          </div>
        ) : g.orders.value.length === 0 ? (
          <Card>
            <EmptyState description={g.emptyMessage.value} />
          </Card>
        ) : (
          <div class="space-y-4">
            {g.orders.value.map((order) => (
              <div key={order.order_no} class="zs-card zs-card-hover flex flex-col gap-4 p-5 md:flex-row md:items-center md:justify-between sm:p-6">
                <div class="min-w-0">
                  <div class="text-xs text-muted">
                    {t('orders.orderNo')}：<span class="zs-num break-all">{order.order_no}</span>
                  </div>
                  <div class="zs-num mt-1 text-xl font-bold text-fg">{g.money(order.total_amount, order.currency)}</div>
                  {g.hasDiscount(order) && (
                    <div class="mt-1 flex flex-wrap gap-x-3 text-xs text-danger-text">
                      {g.hasPositive(order.discount_amount) && (
                        <span>
                          {t('orderDetail.couponDiscountLabel')}：{g.discountMoney(order.discount_amount, order.currency)}
                        </span>
                      )}
                      {g.hasPositive(order.promotion_discount_amount) && (
                        <span>
                          {t('orderDetail.promotionDiscountLabel')}：{g.discountMoney(order.promotion_discount_amount, order.currency)}
                        </span>
                      )}
                    </div>
                  )}
                  <div class="mt-1 text-xs text-muted">{g.formatDate(order.created_at)}</div>
                </div>
                <div class="flex flex-wrap items-center gap-2">
                  <OrderStatusBadge status={order.status} />
                  <Button size="sm" variant="secondary" to={{ name: 'guest-order-detail', params: { order_no: order.order_no } }}>
                    <Eye class="size-4" />
                    {t('guestOrders.viewDetails')}
                  </Button>
                  {order.status === 'pending_payment' && (
                    <Button size="sm" to={{ path: '/pay', query: { guest: '1', order_no: order.order_no } }}>
                      <CreditCard class="size-4" />
                      {t('guestOrders.payNow')}
                    </Button>
                  )}
                </div>
              </div>
            ))}
            <Pagination page={g.pagination.value.page} totalPages={g.pagination.value.total_page} onChange={g.changePage} />
          </div>
        )}
      </div>
    )
  },
})
