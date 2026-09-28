import { defineComponent, type PropType } from 'vue'
import { useI18n } from 'vue-i18n'
import { Mascot, type MascotMood } from './Mascot'

export const EmptyState = defineComponent({
  name: 'ZsEmptyState',
  props: {
    title: String,
    description: String,
    mood: { type: String as PropType<MascotMood>, default: 'sleepy' },
    size: { type: Number, default: 96 },
  },
  setup(props, { slots }) {
    const { t } = useI18n()
    return () => (
      <div class="flex flex-col items-center justify-center gap-2 px-4 py-10 text-center">
        <Mascot mood={props.mood} size={props.size} />
        <p class="zs-display text-sm text-fg">{props.title || t('admin.common.noData')}</p>
        {props.description && <p class="max-w-md text-xs text-muted">{props.description}</p>}
        {slots.default?.()}
      </div>
    )
  },
})

export default EmptyState
