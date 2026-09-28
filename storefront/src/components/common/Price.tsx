import { defineComponent, type PropType } from 'vue'
import { useLocalized } from '@/composables/useLocalized'
import { cn } from '@/components/ui'

/** Price in the number font; `strike` renders the original price style. */
export const Price = defineComponent({
  name: 'Price',
  props: {
    amount: { type: [String, Number] as PropType<string | number | null | undefined>, default: '' },
    currency: { type: String as PropType<string | null>, default: undefined },
    size: { type: String as PropType<'sm' | 'md' | 'lg' | 'xl'>, default: 'md' },
    strike: Boolean,
    highlight: Boolean,
  },
  setup(props) {
    const { formatPrice } = useLocalized()
    const sizes = { sm: 'text-sm', md: 'text-base', lg: 'text-2xl', xl: 'text-4xl' }
    return () => (
      <span
        class={cn(
          'zs-num font-bold leading-none whitespace-nowrap',
          sizes[props.size],
          props.strike && 'font-medium text-muted line-through opacity-80',
          props.highlight && !props.strike && 'text-primary-text',
        )}
      >
        {formatPrice(props.amount, props.currency)}
      </span>
    )
  },
})
