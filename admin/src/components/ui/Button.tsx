import { defineComponent, type PropType } from 'vue'
import { cn } from './cn'

export type ButtonVariant = 'primary' | 'secondary' | 'outline' | 'ghost' | 'danger' | 'soft' | 'link'
export type ButtonSize = 'xs' | 'sm' | 'md' | 'lg' | 'icon' | 'icon-sm'

const variants: Record<ButtonVariant, string> = {
  primary:
    'zs-gradient-bg text-on-primary zs-text-shadow shadow-zs-sm hover:-translate-y-0.5 hover:shadow-glow border border-transparent',
  secondary: 'bg-secondary text-on-primary hover:-translate-y-0.5 hover:shadow-glow border border-transparent',
  outline: 'bg-surface-strong text-fg border border-line-strong hover:border-primary hover:text-primary hover:bg-primary-soft',
  ghost: 'bg-transparent text-fg border border-transparent hover:bg-primary-soft hover:text-primary',
  danger: 'bg-danger text-on-primary border border-transparent hover:-translate-y-0.5 hover:brightness-110',
  soft: 'bg-primary-soft text-primary border border-transparent hover:bg-primary hover:text-on-primary',
  link: 'bg-transparent text-accent border border-transparent underline-offset-4 hover:underline px-0',
}

const sizes: Record<ButtonSize, string> = {
  xs: 'h-7 px-2.5 text-xs gap-1',
  sm: 'h-8 px-3 text-xs gap-1.5',
  md: 'h-9 px-4 text-sm gap-2',
  lg: 'h-11 px-6 text-base gap-2',
  icon: 'h-9 w-9 p-0',
  'icon-sm': 'h-7 w-7 p-0',
}

export const Button = defineComponent({
  name: 'ZsButton',
  props: {
    variant: { type: String as PropType<ButtonVariant>, default: 'outline' },
    size: { type: String as PropType<ButtonSize>, default: 'md' },
    type: { type: String as PropType<'button' | 'submit' | 'reset'>, default: 'button' },
    loading: Boolean,
    disabled: Boolean,
    block: Boolean,
    title: String,
  },
  emits: { click: (_e: MouseEvent) => true },
  setup(props, { slots, emit }) {
    return () => (
      <button
        type={props.type}
        title={props.title}
        disabled={props.disabled || props.loading}
        class={cn(
          'inline-flex select-none items-center justify-center whitespace-nowrap rounded-full font-medium transition-all duration-200 active:scale-[0.97] disabled:pointer-events-none disabled:opacity-50',
          variants[props.variant],
          sizes[props.size],
          props.block && 'w-full',
        )}
        onClick={(e: MouseEvent) => emit('click', e)}
      >
        {props.loading && (
          <svg class="h-4 w-4 animate-spin" viewBox="0 0 24 24" fill="none" aria-hidden="true">
            <circle cx="12" cy="12" r="9" stroke="currentColor" stroke-opacity="0.25" stroke-width="3" />
            <path d="M21 12a9 9 0 0 0-9-9" stroke="currentColor" stroke-width="3" stroke-linecap="round" />
          </svg>
        )}
        {slots.default?.()}
      </button>
    )
  },
})

export default Button
