import { defineComponent, type PropType } from 'vue'
import { useI18n } from 'vue-i18n'
import { PageHero } from '@/components/content/PageHero'
import { RichContent } from '@/components/common/RichContent'
import { useLegal, type LegalType } from '@/composables/useLegal'
import { Card, EmptyState } from '@/components/ui'

export default defineComponent({
  name: 'LegalView',
  props: { type: { type: String as PropType<LegalType>, default: 'terms' } },
  setup(props) {
    const { t } = useI18n()
    const s = useLegal(() => props.type)
    return () => (
      <div class="zs-page max-w-4xl! pb-8">
        <PageHero title={s.title.value} />
        <Card padding="lg">
          {s.loading.value ? (
            <div class="space-y-3">
              {Array.from({ length: 6 }).map((_, i) => (
                <div key={i} class="zs-skeleton h-4" style={{ width: `${90 - i * 8}%` }} />
              ))}
            </div>
          ) : s.content.value ? (
            <RichContent html={s.content.value} />
          ) : (
            <EmptyState size="sm" title={t('common.noContent')} description={t('zsContent.legalEmpty')} />
          )}
        </Card>
      </div>
    )
  },
})
