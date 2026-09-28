import { defineComponent } from 'vue'
import { useI18n } from 'vue-i18n'
import { CheckCircle2, Gift, RotateCcw, Ticket } from 'lucide-vue-next'
import { CaptchaField } from '@/components/common/CaptchaField'
import { Badge, Button, Card, Field, Input } from '@/components/ui'
import { useGiftCardPanel } from '@/composables/personal/useGiftCardPanel'
import { PanelAlertBox, PanelHeading } from './PanelParts'

/** Gift card redemption panel. */
export const GiftCardPanel = defineComponent({
  name: 'GiftCardPanel',
  setup() {
    const { t } = useI18n()
    const g = useGiftCardPanel()
    return () => (
      <div class="space-y-5">
        <Card>
          <PanelHeading title={t('personalCenter.giftCard.title')} description={t('personalCenter.giftCard.subtitle')} icon={Gift}>
            {{ actions: () => <Badge tone="accent">{t('personalCenter.tabs.giftCard')}</Badge> }}
          </PanelHeading>
          <PanelAlertBox alert={g.alert.value} />
          <form
            class="space-y-4"
            onSubmit={(e: Event) => {
              e.preventDefault()
              void g.submit()
            }}
          >
            <Field label={t('personalCenter.giftCard.codeLabel')} required>
              <Input v-model={g.form.code} size="lg" placeholder={t('personalCenter.giftCard.codePlaceholder')} autocomplete="off" inputClass="zs-num tracking-wider uppercase">
                {{ prefix: () => <Ticket class="size-4" /> }}
              </Input>
            </Field>
            {g.captcha.enabled.value && (
              <Field label={t('auth.common.captchaLabel')} required>
                <CaptchaField captcha={g.captcha} />
              </Field>
            )}
            <div class="flex flex-wrap gap-3">
              <Button type="submit" loading={g.submitting.value}>
                <Gift class="size-4" />
                {g.submitting.value ? t('personalCenter.giftCard.redeeming') : t('personalCenter.giftCard.redeemButton')}
              </Button>
              <Button variant="secondary" onClick={g.reset}>
                <RotateCcw class="size-4" />
                {t('personalCenter.giftCard.resetButton')}
              </Button>
            </div>
          </form>
        </Card>
        {g.result.value && (
          <Card>
            <div class="flex items-center gap-3">
              <span class="flex size-11 items-center justify-center rounded-full bg-success-soft text-success-text">
                <CheckCircle2 class="size-6" />
              </span>
              <h3 class="zs-title text-xl text-fg">{t('personalCenter.giftCard.successTitle')}</h3>
            </div>
            <div class="mt-5 grid grid-cols-1 gap-4 sm:grid-cols-3">
              {[
                [t('personalCenter.giftCard.successCode'), g.result.value.gift_card?.code || '-'],
                [t('personalCenter.giftCard.successAmount'), g.redeemedAmount.value],
                [t('personalCenter.giftCard.successBalance'), g.balance.value],
              ].map(([label, value]) => (
                <div key={label} class="zs-soft-bg rounded-zs border border-line p-4">
                  <div class="text-xs font-bold text-muted">{label}</div>
                  <div class="zs-num mt-1 break-all text-lg font-bold text-fg">{value}</div>
                </div>
              ))}
            </div>
          </Card>
        )}
      </div>
    )
  },
})
