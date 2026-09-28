import { defineComponent, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { Upload } from 'lucide-vue-next'

/** Styled file picker with drag & drop. Emits the chosen File (or null). */
export const FileInput = defineComponent({
  name: 'ZsFileInput',
  props: {
    accept: String,
    disabled: Boolean,
    placeholder: String,
  },
  emits: { change: (_f: File | null) => true },
  setup(props, { emit }) {
    const { t } = useI18n()
    const input = ref<HTMLInputElement | null>(null)
    const name = ref('')
    const dragging = ref(false)
    const pick = (file: File | null) => {
      name.value = file?.name ?? ''
      emit('change', file)
    }
    return () => (
      <div
        class={[
          'flex cursor-pointer items-center gap-3 rounded-zs border-2 border-dashed px-4 py-3 transition-colors',
          dragging.value ? 'border-primary bg-primary-soft' : 'border-line-strong bg-surface-strong hover:border-primary',
          props.disabled ? 'pointer-events-none opacity-50' : '',
        ]}
        onClick={() => input.value?.click()}
        onDragover={(e: DragEvent) => {
          e.preventDefault()
          dragging.value = true
        }}
        onDragleave={() => (dragging.value = false)}
        onDrop={(e: DragEvent) => {
          e.preventDefault()
          dragging.value = false
          pick(e.dataTransfer?.files?.[0] ?? null)
        }}
      >
        <span class="zs-gradient-bg flex h-9 w-9 items-center justify-center rounded-full text-white">
          <Upload class="h-4 w-4" />
        </span>
        <span class="min-w-0 flex-1 truncate text-sm text-muted">{name.value || props.placeholder || t('admin.common.chooseFile')}</span>
        <input
          ref={input}
          type="file"
          class="hidden"
          accept={props.accept}
          onChange={(e: Event) => {
            const el = e.target as HTMLInputElement
            pick(el.files?.[0] ?? null)
            el.value = ''
          }}
        />
      </div>
    )
  },
})

export default FileInput
