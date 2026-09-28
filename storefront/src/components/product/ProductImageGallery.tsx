import { defineComponent, type PropType } from 'vue'
import { SmartImage, cn } from '@/components/ui'

/** Main image + thumbnails. */
export const ProductImageGallery = defineComponent({
  name: 'ProductImageGallery',
  props: {
    images: { type: Array as PropType<string[]>, default: () => [] },
    current: { type: String, default: '' },
    alt: { type: String, default: '' },
  },
  emits: { 'update:current': (_v: string) => true },
  setup(props, { emit }) {
    return () => (
      <div class="space-y-3">
        <div class="relative aspect-[4/3] overflow-hidden rounded-zs-lg border border-line bg-surface-muted shadow-zs">
          <SmartImage src={props.current || props.images[0]} alt={props.alt} eager />
          <div class="pointer-events-none absolute inset-0 rounded-zs-lg ring-1 ring-inset ring-white/30" />
        </div>
        {props.images.length > 1 && (
          <div class="zs-scroll-x flex gap-2">
            {props.images.map((img) => (
              <button
                key={img}
                type="button"
                class={cn(
                  'size-16 shrink-0 overflow-hidden rounded-zs-sm border-2 transition',
                  (props.current || props.images[0]) === img ? 'border-primary shadow-zs' : 'border-transparent opacity-70 hover:opacity-100',
                )}
                onClick={() => emit('update:current', img)}
              >
                <img src={img} alt="" class="size-full object-cover" />
              </button>
            ))}
          </div>
        )}
      </div>
    )
  },
})
