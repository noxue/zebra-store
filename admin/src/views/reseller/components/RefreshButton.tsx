import { defineComponent } from 'vue'
import { useI18n } from 'vue-i18n'
import { RefreshCw } from 'lucide-vue-next'
import { Button } from '@/components/ui'

export const RefreshButton = defineComponent({
  name: 'ResellerRefreshButton',
  props: { loading: Boolean },
  emits: { click: () => true },
  setup(props, { emit }) {
    const { t } = useI18n()
    return () => (
      <Button size="sm" loading={props.loading} onClick={() => emit('click')}>
        <RefreshCw class="h-3.5 w-3.5" />
        {t('admin.common.refresh')}
      </Button>
    )
  },
})

export default RefreshButton
