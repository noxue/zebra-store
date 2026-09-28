import { defineComponent, onMounted, type FunctionalComponent } from 'vue'
import { useI18n } from 'vue-i18n'
import { RouterLink, useRouter } from 'vue-router'
import { Bot, ExternalLink, KeyRound, RefreshCw, Send, ShieldAlert, Wifi, WifiOff } from 'lucide-vue-next'
import { Badge, Button, Card, PageHeader } from '@/components/ui'
import { formatLicenseStatus, formatWarnings, formatWebhookStatus } from './telegramUtils'
import { LICENSE_PURCHASE_URL, formatRuntimeDate, useTelegramRuntimeStatus } from './useTelegramRuntimeStatus'

const InfoItem: FunctionalComponent<{ label: string; value: string | number; mono?: boolean; wide?: boolean }> = (props) => (
  <div class={props.wide ? 'md:col-span-2 xl:col-span-3' : ''}>
    <p class="text-xs text-muted">{props.label}</p>
    <p class={['mt-0.5 text-sm font-medium text-fg', props.mono && 'break-all font-mono']}>{props.value}</p>
  </div>
)

export default defineComponent({
  name: 'TelegramBot',
  setup() {
    const { t } = useI18n()
    const router = useRouter()
    const { loading, runtimeStatus, isConnected, fetchRuntimeStatus } = useTelegramRuntimeStatus()
    onMounted(() => void fetchRuntimeStatus())

    const features = () => [
      { title: t('telegramBot.overview.featureBasicSettings'), desc: t('telegramBot.overview.featureBasicSettingsDesc'), to: '/telegram-bot/settings', link: t('telegramBot.overview.goToSettings') },
      { title: t('telegramBot.overview.featureConnectionStatus'), desc: t('telegramBot.overview.featureConnectionStatusDesc'), to: '/telegram-bot/status', link: t('telegramBot.overview.goToStatus') },
      { title: t('telegramBot.overview.featureChannelClients'), desc: t('telegramBot.overview.featureChannelClientsDesc'), to: '/telegram-bot/channel-clients', link: t('telegramBot.overview.goToChannelClients') },
      { title: t('telegramBot.overview.featureBroadcasts'), desc: t('telegramBot.overview.featureBroadcastsDesc'), to: '/telegram-bot/broadcasts', link: t('telegramBot.overview.goToBroadcasts') },
    ]

    return () => {
      const rs = runtimeStatus.value
      const connected = isConnected.value
      return (
        <div class="space-y-6">
          <PageHeader title={t('telegramBot.overview.title')} subtitle={t('telegramBot.overview.subtitle')} />

          {/* License purchase notice */}
          <section class="rounded-zs-lg border border-line bg-warning-soft p-4 shadow-zs-sm sm:p-5">
            <div class="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
              <div class="flex items-start gap-3">
                <span class="flex h-9 w-9 shrink-0 items-center justify-center rounded-full bg-surface-solid text-warning-text">
                  <KeyRound class="h-5 w-5" />
                </span>
                <div class="space-y-1.5">
                  <p class="text-sm font-semibold text-warning-text">{t('telegramBot.licensePurchase.title')}</p>
                  <p class="text-sm text-fg/80">{t('telegramBot.licensePurchase.desc')}</p>
                  <p class="flex items-start gap-1.5 text-xs text-danger-text">
                    <ShieldAlert class="mt-0.5 h-3.5 w-3.5 shrink-0" />
                    <span>{t('telegramBot.licensePurchase.securityNote')}</span>
                  </p>
                </div>
              </div>
              <a
                href={LICENSE_PURCHASE_URL}
                target="_blank"
                rel="noopener noreferrer"
                class="zs-gradient-bg zs-text-shadow inline-flex h-8 shrink-0 items-center justify-center gap-1.5 rounded-full px-4 text-sm font-medium text-on-primary shadow-zs-sm transition hover:-translate-y-0.5"
              >
                {t('telegramBot.licensePurchase.action')}
                <ExternalLink class="h-3.5 w-3.5" />
              </a>
            </div>
          </section>

          {/* Connection status */}
          <Card>
            {{
              title: () => (
                <span class="flex items-center gap-3">
                  <span class="zs-gradient-bg flex h-10 w-10 items-center justify-center rounded-zs text-on-primary">
                    <Bot class="h-5 w-5" />
                  </span>
                  <span>
                    <span class="block">{t('telegramBot.overview.connectionTitle')}</span>
                    <span class="block font-sans text-xs font-normal text-muted">{t('telegramBot.overview.connectionDesc')}</span>
                  </span>
                </span>
              ),
              extra: () => (
                <Badge tone={connected ? 'success' : 'neutral'} size="md">
                  {connected ? <Wifi class="h-3 w-3" /> : <WifiOff class="h-3 w-3" />}
                  {connected ? t('telegramBot.overview.connected') : t('telegramBot.overview.notConnected')}
                </Badge>
              ),
              default: () =>
                connected && rs ? (
                  <div class="grid grid-cols-1 gap-4 md:grid-cols-2 xl:grid-cols-3">
                    <InfoItem label={t('telegramBot.status.botVersion')} value={rs.bot_version || '-'} />
                    <InfoItem label={t('telegramBot.status.webhookStatus')} value={formatWebhookStatus(t, rs.webhook_status)} />
                    <InfoItem label={t('telegramBot.status.lastSeenAt')} value={formatRuntimeDate(rs.last_seen_at)} />
                    <InfoItem label={t('telegramBot.status.configVersion')} value={rs.config_version ?? '-'} />
                    <InfoItem label={t('telegramBot.status.machineCode')} value={rs.machine_code || '-'} mono />
                    <InfoItem label={t('telegramBot.status.licenseStatusLabel')} value={formatLicenseStatus(t, rs.license_status)} />
                    <InfoItem label={t('telegramBot.status.licenseExpiresAt')} value={formatRuntimeDate(rs.license_expires_at)} />
                    <InfoItem label={t('telegramBot.status.licenseWarnings')} value={formatWarnings(t, rs.warnings)} wide />
                  </div>
                ) : (
                  <div class="rounded-zs border border-dashed border-line-strong p-6 text-center">
                    <WifiOff class="mx-auto mb-3 h-10 w-10 text-muted" />
                    <p class="mb-1 text-sm text-muted">{t('telegramBot.overview.notConnectedHint')}</p>
                    <p class="text-xs text-muted">{t('telegramBot.overview.notConnectedDesc')}</p>
                  </div>
                ),
            }}
          </Card>

          {/* Feature overview */}
          <div class="grid grid-cols-1 gap-4 md:grid-cols-2 xl:grid-cols-4">
            {features().map((f) => (
              <Card key={f.to} title={f.title} hover>
                <p class="min-h-[2.5rem] text-sm text-muted">{f.desc}</p>
                <RouterLink to={f.to} class="mt-3 inline-flex items-center gap-1 text-sm font-medium text-primary hover:underline">
                  {f.link}
                  <ExternalLink class="h-3 w-3" />
                </RouterLink>
              </Card>
            ))}
          </div>

          {/* Quick actions */}
          <Card title={t('telegramBot.overview.quickActions')}>
            <div class="flex flex-col gap-3 sm:flex-row">
              <Button size="sm" disabled={loading.value} onClick={() => void fetchRuntimeStatus()}>
                <RefreshCw class={['h-4 w-4', loading.value && 'animate-spin']} />
                {t('telegramBot.overview.refreshStatus')}
              </Button>
              <Button size="sm" onClick={() => void router.push('/telegram-bot/broadcasts/create')}>
                <Send class="h-4 w-4" />
                {t('telegramBot.overview.createBroadcast')}
              </Button>
            </div>
          </Card>
        </div>
      )
    }
  },
})
