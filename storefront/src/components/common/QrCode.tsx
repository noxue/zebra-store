import { defineComponent, ref, watchEffect } from 'vue'
import QRCode from 'qrcode'

/** Locally rendered QR code (no network). */
export const QrCode = defineComponent({
  name: 'QrCode',
  props: {
    value: { type: String, required: true },
    size: { type: Number, default: 220 },
  },
  setup(props) {
    const dataUrl = ref('')
    watchEffect(async () => {
      if (!props.value) {
        dataUrl.value = ''
        return
      }
      try {
        dataUrl.value = await QRCode.toDataURL(props.value, { width: props.size * 2, margin: 1, errorCorrectionLevel: 'M' })
      } catch {
        dataUrl.value = ''
      }
    })
    return () => (
      <div class="inline-flex rounded-zs-lg border-4 border-primary-soft bg-white p-3 shadow-zs" style={{ width: `${props.size + 30}px` }}>
        {dataUrl.value ? <img src={dataUrl.value} alt="QR" width={props.size} height={props.size} class="block size-full" /> : <div class="zs-skeleton" style={{ width: `${props.size}px`, height: `${props.size}px` }} />}
      </div>
    )
  },
})
