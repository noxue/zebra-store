import { defineComponent, type PropType } from 'vue'
import { RouterLink, type RouteLocationRaw } from 'vue-router'
import { LoaderCircle } from 'lucide-vue-next'
import { cn } from './cn'

export type ButtonVariant = 'primary' | 'secondary' | 'soft' | 'outline' | 'ghost' | 'danger' | 'link'
export type ButtonSize = 'xs' | 'sm' | 'md' | 'lg' | 'icon' | 'icon-sm'

const variants: Record<ButtonVariant, string> = {
  primary:
    'zs-gradient-bg text-on-primary shadow-zs [text-shadow:0_1px_2px_rgba(0,0,0,.18)] hover:-translate-y-0.5 hover:shadow-glow',
  secondary: 'bg-surface-strong text-fg border border-line hover:border-line-strong hover:-translate-y-0.5 hover:shadow-zs',
  soft: 'bg-primary-soft text-primary-text hover:-translate-y-0.5 hover:shadow-zs',
  outline: 'bg-transparent text-primary-text border border-line-strong hover:bg-primary-soft',
  ghost: 'bg-transparent text-fg hover:bg-primary-soft hover:text-primary-text',
  danger: 'bg-danger text-on-primary hover:-translate-y-0.5 hover:shadow-zs',
  link: 'bg-transparent text-accent-text underline-offset-4 hover:underline px-0!',
}

const sizes: Record<ButtonSize, string> = {
  xs: 'h-7 px-2.5 text-xs gap-1',
  sm: 'h-9 px-3.5 text-sm gap-1.5',
  md: 'h-11 px-5 text-sm gap-2',
  lg: 'h-13 px-7 text-base gap-2',
  icon: 'h-10 w-10 p-0',
  'icon-sm': 'h-8 w-8 p-0',
}

/** Pill button. Renders RouterLink with `to`, <a> with `href`. */
export const Button = defineComponent({
  name: 'ZsButton',
  props: {
    variant: { type: String as PropType<ButtonVariant>, default: 'primary' },
    size: { type: String as PropType<ButtonSize>, default: 'md' },
    type: { type: String as PropType<'button' | 'submit' | 'reset'>, default: 'button' },
    loading: Boolean,
    disabled: Boolean,
    block: Boolean,
    to: { type: [String, Object] as PropType<RouteLocationRaw>, default: undefined },
    href: { type: String, default: undefined },
    target: { type: String, default: undefined },
  },
  emits: { click: (_e: MouseEvent) => true },
  setup(props, { slots, emit }) {
    return () => {
      const isDisabled = props.disabled || props.loading
      const classes = cn(
        'inline-flex select-none items-center justify-center whitespace-nowrap rounded-full font-bold',
        'transition-all duration-200 ease-[var(--zs-ease-bounce)] active:scale-[0.97]',
        'disabled:pointer-events-none disabled:opacity-55 disabled:translate-y-0',
        variants[props.variant],
        sizes[props.size],
        props.block && 'w-full',
        isDisabled && 'pointer-events-none opacity-55',
      )
      const content = [
        props.loading ? <LoaderCircle class="size-4 zs-spin" /> : null,
        slots.default?.(),
      ]
      const onClick = (e: MouseEvent) => {
        if (isDisabled) {
          e.preventDefault()
          return
        }
        emit('click', e)
      }
      if (props.to !== undefined) {
        return (
          <RouterLink to={props.to} class={classes} onClick={onClick}>
            {content}
          </RouterLink>
        )
      }
      if (props.href !== undefined) {
        return (
          <a href={props.href} target={props.target} rel={props.target === '_blank' ? 'noopener noreferrer' : undefined} class={classes} onClick={onClick}>
            {content}
          </a>
        )
      }
      return (
        <button type={props.type} class={classes} disabled={isDisabled} onClick={onClick}>
          {content}
        </button>
      )
    }
  },
})
