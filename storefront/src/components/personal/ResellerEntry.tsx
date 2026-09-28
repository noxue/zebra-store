import { defineComponent } from 'vue'
import { useI18n } from 'vue-i18n'
import { ArrowRight, Store } from 'lucide-vue-next'
import { Button, Card } from '@/components/ui'
import { PanelHeading } from './PanelParts'

export const ResellerEntry = defineComponent({
  name: 'ResellerEntry',
  setup() {
    const { t } = useI18n()
    return () => (
      <Card>
        <PanelHeading title={t('resellerConsole.title')} description={t('resellerConsole.dashboard.description')} icon={Store} />
        <Button to="/reseller">
          {t('resellerConsole.nav.dashboard')}
          <ArrowRight class="size-4" />
        </Button>
      </Card>
    )
  },
})
