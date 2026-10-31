import { defineComponent } from 'vue'
import { useI18n } from 'vue-i18n'
import { Card, EmptyState, Modal } from '@/components/ui'
import { useContact } from '@/composables/useContact'
import { getImageUrl } from '@/utils/image'

export default defineComponent({
  name: 'ContactView',
  setup() {
    const { t } = useI18n()
    const s = useContact()
    const cardClass = 'rounded-zs border border-line bg-surface-strong p-6 text-fg'
    return () => {
      const c = s.contact.value
      const links = [
        { key: 'telegram', label: 'Telegram', href: c.telegram },
        { key: 'whatsapp', label: 'WhatsApp', href: c.whatsapp },
        { key: 'support', label: t('about.supportLink'), href: c.support_url },
        { key: 'email', label: c.email, href: c.email ? `mailto:${c.email}` : '' },
      ].filter((row) => row.href)
      const accounts = [
        { key: 'wechat', label: t('about.wechatId'), value: c.wechat_id },
        { key: 'qq', label: t('about.qqNumber'), value: c.qq_number },
        { key: 'group', label: t('about.qqGroup'), value: c.qq_group },
      ].filter((row) => row.value)
      const codes = [
        { key: 'wechat', label: t('about.wechatQrCode'), image: c.wechat_qr_code },
        { key: 'telegram', label: t('about.telegramQrCode'), image: c.telegram_qr_code },
        { key: 'group', label: t('about.wechatGroupQrCode'), image: c.wechat_group_qr_code },
      ].filter((row) => row.image)
      return (
        <div class="zs-page max-w-5xl! space-y-6 py-10 sm:py-14">
          <h1 class="zs-title text-3xl text-fg">{s.title.value}</h1>
          {s.text.value && <p class="whitespace-pre-line text-muted">{s.text.value}</p>}
          {s.hasContact.value ? (
            <div class="grid gap-4 sm:grid-cols-2">
              {codes.map((row) => (
                <div key={`qr-${row.key}`} class={cardClass}>
                  <h2 class="mb-4 font-bold">{row.label}</h2>
                  <button type="button" class="block w-full cursor-zoom-in rounded-zs focus-visible:outline-2 focus-visible:outline-offset-4 focus-visible:outline-primary" aria-label={row.label} aria-haspopup="dialog" onClick={() => s.openQrPreview({ label: row.label, image: row.image! })}>
                    <img src={getImageUrl(row.image)} alt={row.label} class="mx-auto size-56 max-w-full rounded-zs object-contain" />
                  </button>
                </div>
              ))}
              {links.map((row) => <a key={row.key} href={row.href} target="_blank" rel="noopener noreferrer" class={`${cardClass} zs-card-hover break-all font-bold`}>{row.label}</a>)}
              {accounts.map((row) => <div key={row.key} class={cardClass}>{row.label}: <span class="select-all break-all">{row.value}</span></div>)}
            </div>
          ) : <Card><EmptyState title={t('customerSupport.empty')} /></Card>}
          <Modal open={Boolean(s.qrPreview.value)} title={s.qrPreview.value?.label} size="lg" onClose={s.closeQrPreview}>
            {s.qrPreview.value && <img src={getImageUrl(s.qrPreview.value.image)} alt={s.qrPreview.value.label} class="mx-auto block h-auto max-h-[65dvh] w-full max-w-md rounded-zs object-contain" />}
          </Modal>
        </div>
      )
    }
  },
})
