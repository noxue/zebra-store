import { defineComponent, ref, watch, type PropType } from 'vue'
import { RouterLink } from 'vue-router'
import { ChevronRight, ImageOff } from 'lucide-vue-next'
import { getImageUrl } from '@/utils/image'
import { cn } from './cn'

/** Display-font section heading with gradient text and twinkling star. */
export const SectionTitle = defineComponent({
  name: 'ZsSectionTitle',
  props: {
    title: { type: String, required: true },
    subtitle: { type: String, default: '' },
    as: { type: String as PropType<'h1' | 'h2' | 'h3'>, default: 'h2' },
    size: { type: String as PropType<'md' | 'lg' | 'xl'>, default: 'lg' },
  },
  setup(props, { slots }) {
    const sizes = { md: 'text-xl sm:text-2xl', lg: 'text-2xl sm:text-3xl', xl: 'text-3xl sm:text-4xl' }
    return () => {
      const Tag = props.as
      return (
        <div class="flex flex-wrap items-end justify-between gap-3">
          <div class="min-w-0">
            <Tag class={cn('zs-title flex items-center gap-2', sizes[props.size])}>
              <span class="zs-gradient-text">{props.title}</span>
              <span class="zs-sparkle text-base" aria-hidden="true">
                ✦
              </span>
            </Tag>
            {props.subtitle && <p class="mt-1.5 text-sm text-muted">{props.subtitle}</p>}
          </div>
          {slots.actions && <div class="flex items-center gap-3">{slots.actions()}</div>}
        </div>
      )
    }
  },
})

/** Lazy image with gradient fallback when missing/broken. */
export const SmartImage = defineComponent({
  name: 'ZsSmartImage',
  props: {
    src: { type: String as PropType<string | null | undefined>, default: '' },
    alt: { type: String, default: '' },
    imgClass: { type: String, default: '' },
    eager: Boolean,
  },
  setup(props) {
    const failed = ref(false)
    watch(
      () => props.src,
      () => {
        failed.value = false
      },
    )
    return () => {
      const url = getImageUrl(props.src || '')
      if (!url || failed.value) {
        return (
          <div class={cn('flex size-full items-center justify-center zs-soft-bg text-muted', props.imgClass)}>
            <ImageOff class="size-7 opacity-60" />
          </div>
        )
      }
      return (
        <img
          src={url}
          alt={props.alt}
          loading={props.eager ? 'eager' : 'lazy'}
          decoding="async"
          class={cn('size-full object-cover', props.imgClass)}
          onError={() => {
            failed.value = true
          }}
        />
      )
    }
  },
})

export interface BreadcrumbItem {
  label: string
  to?: string
}

export const Breadcrumb = defineComponent({
  name: 'ZsBreadcrumb',
  props: { items: { type: Array as PropType<BreadcrumbItem[]>, required: true } },
  setup(props) {
    return () => (
      <nav aria-label="breadcrumb" class="flex flex-wrap items-center gap-1 text-sm text-muted">
        {props.items.map((item, i) => (
          <span key={i} class="inline-flex items-center gap-1">
            {i > 0 && <ChevronRight class="size-3.5 opacity-60" />}
            {item.to ? (
              <RouterLink to={item.to} class="transition hover:text-primary-text">
                {item.label}
              </RouterLink>
            ) : (
              <span class="max-w-[16rem] truncate font-bold text-fg">{item.label}</span>
            )}
          </span>
        ))}
      </nav>
    )
  },
})

/** Stat tile for dashboards (label, value, icon). */
export const StatCard = defineComponent({
  name: 'ZsStatCard',
  props: {
    label: { type: String, required: true },
    value: { type: [String, Number], default: '' },
    tone: { type: String as PropType<'primary' | 'secondary' | 'accent' | 'gold' | 'success'>, default: 'primary' },
  },
  setup(props, { slots }) {
    const tones = {
      primary: 'bg-primary-soft text-primary-text',
      secondary: 'bg-secondary-soft text-secondary-text',
      accent: 'bg-accent-soft text-accent-text',
      gold: 'bg-gold-soft text-warning-text',
      success: 'bg-success-soft text-success-text',
    }
    return () => (
      <div class="zs-card zs-card-hover flex items-start justify-between gap-3 p-5">
        <div class="min-w-0">
          <div class="text-xs font-bold text-muted">{props.label}</div>
          <div class="mt-2 truncate text-2xl font-bold text-fg zs-num">{slots.default ? slots.default() : props.value}</div>
        </div>
        {slots.icon && <div class={cn('flex size-11 shrink-0 items-center justify-center rounded-zs', tones[props.tone])}>{slots.icon()}</div>}
      </div>
    )
  },
})

/** SVG wave separator between sections. */
export const WaveDivider = defineComponent({
  name: 'ZsWaveDivider',
  props: { flip: Boolean },
  setup(props) {
    return () => (
      <svg viewBox="0 0 1440 60" preserveAspectRatio="none" class={cn('block h-8 w-full sm:h-12', props.flip && 'rotate-180')} aria-hidden="true">
        <path d="M0 30 C 240 70 480 -10 720 30 C 960 70 1200 -10 1440 30 L1440 60 L0 60 Z" fill="var(--zs-primary-soft)" opacity="0.55" />
        <path d="M0 40 C 260 10 520 70 760 38 C 1000 6 1220 60 1440 36 L1440 60 L0 60 Z" fill="var(--zs-secondary-soft)" opacity="0.5" />
      </svg>
    )
  },
})
