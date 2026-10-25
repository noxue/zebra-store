import { defineComponent } from 'vue'
import { useI18n } from 'vue-i18n'
import { Button } from '@/components/ui'
import { usePersonalApiKey } from '@/composables/usePersonalApiKey'
export const PersonalApiKeyCard = defineComponent({
  name: 'PersonalApiKeyCard',
  setup() {
    const { t } = useI18n()
    const p = usePersonalApiKey()
    return () => (
      <section class="space-y-4 rounded-zs-lg border border-line bg-surface-solid p-5">
        <h3 class="font-semibold text-fg">{t('personalApiKey.title')}</h3>
        <p class="text-sm text-muted">{t('personalApiKey.hint')}</p>
        {p.error.value && <p role="alert" class="text-sm text-danger-text">{p.error.value}</p>}
        {p.loaded.value && <p class="text-sm text-fg">{p.status.value ? `${t('personalApiKey.prefix')}: ${p.status.value.prefix}…` : t('personalApiKey.empty')}</p>}
        {p.secret.value && <div class="space-y-2 rounded-zs border border-line p-3">
          <p class="text-sm text-muted">{t('personalApiKey.once')}</p>
          <code class="block break-all select-all text-sm text-fg">{p.secret.value}</code>
          <Button onClick={() => void p.copy()}>{t(p.copied.value ? 'personalApiKey.copied' : 'personalApiKey.copy')}</Button>
        </div>}
        <div class="flex flex-wrap gap-2">
          {p.loaded.value && (p.status.value ? <>
            <Button disabled={p.busy.value} onClick={() => void p.act('rotate')}>{t('personalApiKey.rotate')}</Button>
            <Button disabled={p.busy.value} onClick={() => void p.act('delete')}>{t('personalApiKey.delete')}</Button>
          </> : <Button variant="primary" disabled={p.busy.value} onClick={() => void p.act('create')}>{t('personalApiKey.create')}</Button>)}
          <Button disabled={p.busy.value} onClick={() => void p.load()}>{t('personalApiKey.refresh')}</Button>
        </div>
        <p class="text-xs text-muted">{t('personalApiKey.endpoint')}: <code>POST /api/v1/auth/api-key/token</code></p>
        <p class="text-xs text-muted">{t('personalApiKey.usage')}</p>
      </section>
    )
  },
})
