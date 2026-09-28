import { defineComponent, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { RefreshCw } from 'lucide-vue-next'
import { adminAPI } from '@/api/admin'
import { Input } from '@/components/ui'

export interface ImageCaptchaValue {
  captcha_id: string
  captcha_code: string
}

export const ImageCaptcha = defineComponent({
  name: 'ImageCaptcha',
  props: { modelValue: { type: Object as () => ImageCaptchaValue, default: () => ({ captcha_id: '', captcha_code: '' }) } },
  emits: { 'update:modelValue': (_v: ImageCaptchaValue) => true },
  setup(props, { emit, expose }) {
    const { t } = useI18n()
    const image = ref('')
    const loading = ref(false)
    const refresh = async () => {
      loading.value = true
      try {
        const { data } = await adminAPI.getImageCaptcha()
        image.value = data?.image_base64 ?? ''
        emit('update:modelValue', { captcha_id: data?.captcha_id ?? '', captcha_code: '' })
      } catch {
        image.value = ''
      } finally {
        loading.value = false
      }
    }
    expose({ refresh })
    onMounted(refresh)
    return () => (
      <div class="flex items-center gap-2">
        <Input
          modelValue={props.modelValue.captcha_code}
          placeholder={t('admin.login.captchaLabel')}
          onUpdate:modelValue={(v: string | number) => emit('update:modelValue', { ...props.modelValue, captcha_code: String(v) })}
        />
        <button
          type="button"
          onClick={refresh}
          class="relative h-9 w-28 shrink-0 overflow-hidden rounded-zs-sm border border-line-strong bg-surface-strong"
          title="refresh"
        >
          {image.value ? <img src={image.value} alt="captcha" class="h-full w-full object-cover" /> : <RefreshCw class={['mx-auto h-4 w-4 text-muted', loading.value && 'animate-spin']} />}
        </button>
      </div>
    )
  },
})

export default ImageCaptcha
