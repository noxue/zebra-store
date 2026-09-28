import { defineComponent, type PropType } from 'vue'
import { useI18n } from 'vue-i18n'
import { Check, Copy } from 'lucide-vue-next'
import { useClipboard } from '@/composables/useClipboard'
import { Button } from '@/components/ui'

export const CopyButton = defineComponent({
  name: 'CopyButton',
  props: {
    value: { type: String, required: true },
    label: { type: String, default: '' },
    size: { type: String as PropType<'xs' | 'sm' | 'md'>, default: 'sm' },
    variant: { type: String as PropType<'soft' | 'secondary' | 'ghost'>, default: 'soft' },
  },
  setup(props) {
    const { t } = useI18n()
    const { copy, copiedKey } = useClipboard()
    return () => (
      <Button size={props.size} variant={props.variant} onClick={() => void copy(props.value, props.value)}>
        {copiedKey.value === props.value ? <Check class="size-3.5" /> : <Copy class="size-3.5" />}
        {props.label || (copiedKey.value === props.value ? t('zs.copied') : t('zs.copy'))}
      </Button>
    )
  },
})
