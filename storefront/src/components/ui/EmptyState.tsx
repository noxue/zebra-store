import { defineComponent, type PropType } from 'vue'
import { useI18n } from 'vue-i18n'
import { cn } from './cn'
import { Kitty } from './Mascot'

/** Illustrated empty/error state with optional action slot. */
export const EmptyState = defineComponent({
  name: 'ZsEmptyState',
  props: {
    title: { type: String, default: '' },
    description: { type: String, default: '' },
    variant: { type: String as PropType<'empty' | 'search' | 'error'>, default: 'empty' },
    size: { type: String as PropType<'sm' | 'md'>, default: 'md' },
  },
  setup(props, { slots }) {
    const { t } = useI18n()
    return () => (
      <div class={cn('flex flex-col items-center justify-center text-center', props.size === 'sm' ? 'gap-2 py-8' : 'gap-3 py-14')}>
        <div class={props.size === 'sm' ? 'h-20 w-24' : 'h-28 w-32'}>
          <Kitty mood={props.variant === 'error' ? 'sad' : props.variant === 'search' ? 'happy' : 'sleepy'} />
        </div>
        <h3 class="zs-title text-lg text-fg">{props.title || t(props.variant === 'error' ? 'emptyState.error' : props.variant === 'search' ? 'emptyState.search' : 'emptyState.default')}</h3>
        {props.description && <p class="max-w-sm text-sm text-muted">{props.description}</p>}
        {slots.default && <div class="mt-2 flex flex-wrap justify-center gap-3">{slots.default()}</div>}
      </div>
    )
  },
})
