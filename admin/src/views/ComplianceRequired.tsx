import { defineComponent } from 'vue'
import { useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { Button, Mascot } from '@/components/ui'

export default defineComponent({
  name: 'ComplianceRequiredView',
  setup() {
    const { t } = useI18n()
    const router = useRouter()
    return () => (
      <div class="flex min-h-[60vh] flex-col items-center justify-center gap-4 px-6 text-center">
        <Mascot size={140} mood="sleepy" />
        <h2 class="zs-display text-xl text-fg">{t('compliance.required.title')}</h2>
        <p class="max-w-xl text-sm leading-relaxed text-muted">{t('compliance.required.message')}</p>
        <Button variant="primary" onClick={() => router.push('/')}>
          {t('compliance.required.back')}
        </Button>
      </div>
    )
  },
})
