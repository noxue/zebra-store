import { defineComponent, onBeforeUnmount, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { Check, Languages } from 'lucide-vue-next'
import { LOCALE_LABELS, SUPPORTED_LOCALES } from '@/i18n'
import { useAppStore } from '@/stores/app'
import { cn } from '@/components/ui'

/** Globe button + popover with the three storefront languages. */
export const LanguageSwitcher = defineComponent({
  name: 'LanguageSwitcher',
  props: { align: { type: String, default: 'right' } },
  setup(props) {
    const { t } = useI18n()
    const appStore = useAppStore()
    const open = ref(false)
    const root = ref<HTMLElement | null>(null)
    const onDoc = (e: MouseEvent) => {
      if (root.value && !root.value.contains(e.target as Node)) open.value = false
    }
    onMounted(() => document.addEventListener('click', onDoc))
    onBeforeUnmount(() => document.removeEventListener('click', onDoc))
    return () => (
      <div ref={root} class="relative">
        <button
          type="button"
          aria-label={t('navbar.selectLanguage')}
          class="flex h-10 items-center gap-1 rounded-full px-3 text-sm font-bold text-muted transition hover:bg-primary-soft hover:text-primary-text"
          onClick={() => {
            open.value = !open.value
          }}
        >
          <Languages class="size-[18px]" />
          <span class="text-xs">{LOCALE_LABELS[appStore.locale as keyof typeof LOCALE_LABELS]?.slice(0, 2) ?? ''}</span>
        </button>
        {open.value && (
          <div class={cn('zs-pop absolute top-12 z-50 w-40 overflow-hidden rounded-zs border border-line bg-surface-solid p-1.5 shadow-zs-lg', props.align === 'left' ? 'left-0' : 'right-0')}>
            {SUPPORTED_LOCALES.map((code) => (
              <button
                key={code}
                type="button"
                class={cn(
                  'flex w-full items-center justify-between rounded-zs-sm px-3 py-2 text-sm transition',
                  appStore.locale === code ? 'bg-primary-soft font-bold text-primary-text' : 'text-fg hover:bg-surface-muted',
                )}
                onClick={() => {
                  appStore.setLocale(code)
                  open.value = false
                }}
              >
                {LOCALE_LABELS[code]}
                {appStore.locale === code && <Check class="size-4" />}
              </button>
            ))}
          </div>
        )}
      </div>
    )
  },
})
