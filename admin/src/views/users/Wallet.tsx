import { defineComponent, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { Check, Save } from 'lucide-vue-next'
import { Button, Card, Loader, PageHeader, Switch, cn } from '@/components/ui'
import { useWalletConfig } from './useWalletConfig'

export default defineComponent({
  name: 'WalletConfigView',
  setup() {
    const { t } = useI18n()
    const { form, channels, loading, saving, init, toggleChannel, save } = useWalletConfig()
    onMounted(() => void init())

    return () => (
      <div class="space-y-6">
        <PageHeader title={t('admin.settings.wallet.title')} subtitle={t('admin.settings.wallet.subtitle')} />

        <Card>
          {{
            default: () =>
              loading.value ? (
                <Loader />
              ) : (
                <div class="space-y-5">
                  <div class="flex items-center justify-between gap-4">
                    <div>
                      <p class="text-sm font-medium text-fg">{t('admin.settings.wallet.walletOnlyPayment')}</p>
                      <p class="mt-0.5 text-xs text-muted">{t('admin.settings.wallet.walletOnlyPaymentTip')}</p>
                    </div>
                    <Switch v-model={form.wallet_only_payment} />
                  </div>

                  <div class="border-t border-line pt-5">
                    <p class="mb-2 text-xs font-medium text-muted">{t('admin.settings.wallet.rechargeChannels')}</p>
                    {channels.value.length > 0 ? (
                      <div class="flex flex-wrap gap-2">
                        {channels.value.map((ch) => {
                          const on = form.recharge_channel_ids.includes(ch.id)
                          return (
                            <button
                              key={ch.id}
                              type="button"
                              aria-pressed={on}
                              onClick={() => toggleChannel(ch.id)}
                              class={cn(
                                'inline-flex items-center gap-1.5 rounded-full border px-3 py-1.5 text-xs transition-colors',
                                on ? 'border-primary bg-primary-soft text-primary' : 'border-line text-muted hover:border-primary hover:text-fg',
                              )}
                            >
                              <span
                                class={cn(
                                  'inline-flex h-4 w-4 items-center justify-center rounded-[5px] border',
                                  on ? 'zs-gradient-bg border-transparent text-on-primary' : 'border-line-strong bg-surface-solid',
                                )}
                              >
                                {on && <Check class="h-3 w-3" />}
                              </span>
                              {ch.name}
                              <span class="font-mono text-[10px] opacity-60">#{ch.id}</span>
                            </button>
                          )
                        })}
                      </div>
                    ) : (
                      <p class="text-xs text-muted">{t('admin.settings.wallet.noChannels')}</p>
                    )}
                    <p class="mt-2 text-xs text-muted">{t('admin.settings.wallet.rechargeChannelsTip')}</p>
                  </div>
                </div>
              ),
            footer: () => (
              <div class="flex justify-end">
                <Button variant="primary" loading={saving.value} onClick={save}>
                  <Save class="h-4 w-4" />
                  {saving.value ? t('admin.settings.actions.saving') : t('admin.settings.actions.save')}
                </Button>
              </div>
            ),
          }}
        </Card>
      </div>
    )
  },
})
