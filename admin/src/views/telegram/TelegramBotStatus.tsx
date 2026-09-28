import { defineComponent, onMounted, type FunctionalComponent } from 'vue'
import { useI18n } from 'vue-i18n'
import { RefreshCw, Wifi, WifiOff } from 'lucide-vue-next'
import { Badge, Button, Card, PageHeader } from '@/components/ui'
import { formatLicenseStatus, formatWarning, formatWebhookStatus, licenseStatusTone } from './telegramUtils'
import { formatRuntimeDate, useTelegramRuntimeStatus } from './useTelegramRuntimeStatus'

const Tile: FunctionalComponent<{ label: string; wide?: boolean }> = (props, { slots }) => (
  <div class={['rounded-zs border border-line bg-surface-strong p-4', props.wide && 'md:col-span-2']}>
    <p class="mb-1 text-xs text-muted">{props.label}</p>
    {slots.default?.()}
  </div>
)

export default defineComponent({
  name: 'TelegramBotStatus',
  setup() {
    const { t } = useI18n()
    const { loading, runtimeStatus, isConnected, fetchRuntimeStatus } = useTelegramRuntimeStatus()
    onMounted(() => void fetchRuntimeStatus())

    return () => {
      const rs = runtimeStatus.value
      const connected = isConnected.value
      const connectedLabel = connected ? t('telegramBot.overview.connected') : t('telegramBot.overview.notConnected')
      return (
        <div class="space-y-6">
          <PageHeader title={t('telegramBot.status.title')} subtitle={t('telegramBot.status.subtitle')}>
            {{
              actions: () => (
                <Button size="sm" disabled={loading.value} onClick={() => void fetchRuntimeStatus()}>
                  <RefreshCw class={['h-4 w-4', loading.value && 'animate-spin']} />
                  {t('telegramBot.overview.refreshStatus')}
                </Button>
              ),
            }}
          </PageHeader>

          <Card title={t('telegramBot.status.connectionStatus')} description={t('telegramBot.status.connectionStatusDesc')}>
            {{
              extra: () => (
                <Badge tone={connected ? 'success' : 'neutral'} size="md">
                  {connected ? <Wifi class="h-3 w-3" /> : <WifiOff class="h-3 w-3" />}
                  {connectedLabel}
                </Badge>
              ),
              default: () =>
                rs ? (
                  <div class="grid grid-cols-1 gap-4 md:grid-cols-2">
                    <Tile label={t('telegramBot.status.connected')}>
                      <Badge tone={connected ? 'success' : 'danger'} dot>
                        {connectedLabel}
                      </Badge>
                    </Tile>
                    <Tile label={t('telegramBot.status.botVersion')}>
                      <p class="text-lg font-semibold text-fg">{rs.bot_version || '-'}</p>
                    </Tile>
                    <Tile label={t('telegramBot.status.webhookStatus')}>
                      <p class="text-lg font-semibold text-fg">{formatWebhookStatus(t, rs.webhook_status)}</p>
                    </Tile>
                    <Tile label={t('telegramBot.status.configVersion')}>
                      <p class="zs-num text-lg font-semibold text-fg">{rs.config_version ?? '-'}</p>
                    </Tile>
                    <Tile label={t('telegramBot.status.lastSeenAt')}>
                      <p class="text-lg font-semibold text-fg">{formatRuntimeDate(rs.last_seen_at)}</p>
                    </Tile>
                    <Tile label={t('telegramBot.status.lastConfigSyncAt')}>
                      <p class="text-lg font-semibold text-fg">{formatRuntimeDate(rs.last_config_sync_at)}</p>
                    </Tile>
                    <Tile label={t('telegramBot.status.machineCode')} wide>
                      <p class="break-all font-mono text-sm font-semibold text-fg">{rs.machine_code || '-'}</p>
                    </Tile>
                    <Tile label={t('telegramBot.status.licenseStatusLabel')}>
                      <Badge tone={licenseStatusTone(rs.license_status)}>{formatLicenseStatus(t, rs.license_status)}</Badge>
                    </Tile>
                    <Tile label={t('telegramBot.status.licenseExpiresAt')}>
                      <p class="text-lg font-semibold text-fg">{formatRuntimeDate(rs.license_expires_at)}</p>
                    </Tile>
                    <Tile label={t('telegramBot.status.licenseWarnings')} wide>
                      {rs.warnings?.length ? (
                        <div class="flex flex-wrap gap-2">
                          {rs.warnings.map((w) => (
                            <Badge key={w} tone="warning">
                              {formatWarning(t, w)}
                            </Badge>
                          ))}
                        </div>
                      ) : (
                        <p class="text-sm font-semibold text-fg">{t('telegramBot.status.licenseWarningsEmpty')}</p>
                      )}
                    </Tile>
                  </div>
                ) : (
                  <div class="rounded-zs border border-dashed border-line-strong p-6 text-center">
                    <WifiOff class="mx-auto mb-3 h-10 w-10 text-muted" />
                    <p class="text-sm text-muted">{t('telegramBot.overview.notConnectedHint')}</p>
                  </div>
                ),
            }}
          </Card>
        </div>
      )
    }
  },
})
