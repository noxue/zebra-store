import { defineComponent } from 'vue'
import { useI18n } from 'vue-i18n'
import { Badge } from '@/components/ui'
import { orderStatusLabel, orderStatusVariant } from '@/utils/status'

export const OrderStatusBadge = defineComponent({
  name: 'OrderStatusBadge',
  props: { status: { type: String, default: '' } },
  setup(props) {
    const { t } = useI18n()
    return () => <Badge tone={orderStatusVariant(props.status)}>{orderStatusLabel(t, props.status)}</Badge>
  },
})
