import { defineComponent } from 'vue'
import { useConfirmStore } from '@/stores/confirm'
import { Button } from './Button'
import { Dialog } from './Dialog'
import { Mascot } from './Mascot'
import { cn } from './cn'

/** Renders the promise-based confirm dialog from stores/confirm. */
export const ConfirmHost = defineComponent({
  name: 'ZsConfirmHost',
  setup() {
    const store = useConfirmStore()
    return () => {
      const o = store.options
      return (
        <Dialog modelValue={store.open} size="sm" title={o.title} onUpdate:modelValue={(v: boolean) => !v && store.cancel()}>
          {{
            default: () => (
              <div class="flex items-start gap-3">
                <div class="shrink-0">
                  <Mascot size={56} mood={o.variant === 'destructive' ? 'surprised' : 'wink'} />
                </div>
                <p class="pt-1 text-sm leading-relaxed text-fg">
                  {typeof o.description === 'string'
                    ? o.description
                    : o.description.map((seg, i) => (
                        <span
                          key={i}
                          class={cn(seg.strong && 'font-semibold', seg.tone === 'danger' && 'text-danger-text', seg.tone === 'muted' && 'text-muted')}
                        >
                          {seg.text}
                        </span>
                      ))}
                </p>
              </div>
            ),
            footer: () => (
              <>
                <Button onClick={() => store.cancel()}>{o.cancelText}</Button>
                <Button variant={o.variant === 'destructive' ? 'danger' : 'primary'} onClick={() => store.confirm()}>
                  {o.confirmText}
                </Button>
              </>
            ),
          }}
        </Dialog>
      )
    }
  },
})

export default ConfirmHost
