import { defineComponent, Teleport, TransitionGroup } from 'vue'
import { CheckCircle2, Info, X, XCircle } from 'lucide-vue-next'
import { useNoticeStore } from '@/stores/notice'
import { cn } from './cn'

/** Toast stack (top-right). */
export const NoticeHost = defineComponent({
  name: 'ZsNoticeHost',
  setup() {
    const store = useNoticeStore()
    return () => (
      <Teleport to="body">
        <div class="pointer-events-none fixed right-4 top-4 z-[70] flex w-[min(92vw,380px)] flex-col gap-2">
          <TransitionGroup
            enterActiveClass="transition duration-300 [transition-timing-function:var(--zs-ease-bounce)]"
            enterFromClass="opacity-0 translate-x-6"
            leaveActiveClass="transition duration-200"
            leaveToClass="opacity-0 translate-x-6"
          >
            {store.items.map((item) => {
              const Icon = item.type === 'success' ? CheckCircle2 : item.type === 'error' ? XCircle : Info
              return (
                <div
                  key={item.id}
                  role="status"
                  class={cn(
                    'pointer-events-auto flex items-start gap-2.5 rounded-zs border bg-surface-solid p-3 pr-2 text-sm shadow-zs',
                    item.type === 'success' ? 'border-success/50' : item.type === 'error' ? 'border-danger/50' : 'border-accent/50',
                  )}
                >
                  <Icon
                    class={cn(
                      'mt-0.5 h-4 w-4 shrink-0',
                      item.type === 'success' ? 'text-success-text' : item.type === 'error' ? 'text-danger-text' : 'text-info-text',
                    )}
                  />
                  <p class="min-w-0 flex-1 break-words text-fg">{item.message}</p>
                  <button type="button" class="rounded-full p-0.5 text-muted hover:text-primary" onClick={() => store.remove(item.id)} aria-label="close">
                    <X class="h-3.5 w-3.5" />
                  </button>
                </div>
              )
            })}
          </TransitionGroup>
        </div>
      </Teleport>
    )
  },
})

export default NoticeHost
