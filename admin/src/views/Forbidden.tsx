import { defineComponent } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { Button, Mascot } from '@/components/ui'

export default defineComponent({
  name: 'ForbiddenView',
  setup() {
    const { t } = useI18n()
    const router = useRouter()
    const route = useRoute()
    return () => (
      <div class="flex min-h-[60vh] flex-col items-center justify-center gap-4 text-center">
        <Mascot size={150} mood="surprised" float />
        <h1 class="zs-display zs-gradient-text text-3xl">403</h1>
        <p class="zs-display text-lg text-fg">{t('admin.zebra.forbiddenTitle')}</p>
        <p class="max-w-md text-sm text-muted">{t('admin.zebra.forbiddenDesc')}</p>
        {typeof route.query.from === 'string' && <code class="rounded-full bg-surface-muted px-3 py-1 text-xs text-muted">{route.query.from}</code>}
        <div class="flex gap-2">
          <Button onClick={() => router.back()}>{t('admin.zebra.back')}</Button>
          <Button variant="primary" onClick={() => router.push('/')}>
            {t('admin.zebra.home')}
          </Button>
        </div>
      </div>
    )
  },
})
