import { defineComponent, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { CircleAlert, ExternalLink, Globe2, Link2, Plus, RotateCw } from 'lucide-vue-next'
import type { ResellerDomainData } from '@/api/types'
import { useResellerDomains } from '@/composables/reseller/useResellerDomains'
import { CopyButton } from '@/components/common/CopyButton'
import { ResellerAlert, ResellerPageHeader, ResellerPageState } from '@/components/reseller/ConsoleParts'
import { Badge, Button, Card, DataTable, Input, columns } from '@/components/ui'
import { domainStatusTone, domainVerificationKey, domainVerificationTone, formatResellerConsoleDate, isActiveVerifiedDomain } from '@/utils/reseller/console'
import { getResellerDomainStatusKey } from '@/utils/reseller/management'

export default defineComponent({
  name: 'ResellerDomains',
  setup() {
    const { t } = useI18n()
    const d = useResellerDomains()
    onMounted(() => {
      if (!d.profile.snapshot.value) void d.profile.load()
    })

    const statusLabel = (s?: string) => t(`personalCenter.reseller.domainStatus.${getResellerDomainStatusKey(s)}`)
    const verifyLabel = (s?: string) => t(`personalCenter.reseller.domainVerification.${domainVerificationKey(s)}`)
    const typeLabel = (type?: string) => (type === 'subdomain' || type === 'custom' ? t(`personalCenter.reseller.domainType.${type}`) : type || '-')

    const domainColumns = columns<ResellerDomainData>([
      {
        key: 'domain',
        title: t('resellerConsole.domains.tableDomain'),
        render: (row) => (
          <div class="flex flex-wrap items-center gap-2">
            <span class="break-all font-mono text-sm font-bold">{row.domain}</span>
            {row.is_primary && isActiveVerifiedDomain(row) && <Badge tone="accent" size="xs">{t('personalCenter.reseller.primaryDomain')}</Badge>}
          </div>
        ),
      },
      { key: 'status', title: t('resellerConsole.domains.tableStatus'), render: (row) => <Badge tone={domainStatusTone(row.status)}>{statusLabel(row.status)}</Badge> },
      {
        key: 'verification_status',
        title: t('resellerConsole.domains.tableVerification'),
        render: (row) => (
          <div class="space-y-1">
            <Badge tone={domainVerificationTone(row.verification_status)}>{verifyLabel(row.verification_status)}</Badge>
            {row.verification_token && row.verification_status !== 'verified' && (
              <div class="flex items-center gap-1.5 text-xs text-muted">
                {t('personalCenter.reseller.verificationToken')}: <code class="rounded bg-surface-muted px-1.5 py-0.5 font-mono">{row.verification_token}</code>
              </div>
            )}
          </div>
        ),
      },
      { key: 'updated_at', title: t('resellerConsole.domains.tableUpdated'), render: (row) => <span class="whitespace-nowrap text-xs text-muted">{formatResellerConsoleDate(row.updated_at)}</span> },
      {
        key: 'actions',
        title: t('resellerConsole.domains.tableActions'),
        align: 'right',
        render: (row) => (
          <div class="flex justify-end gap-2">
            <CopyButton value={row.domain} size="xs" label={t('resellerConsole.common.copy')} />
            {isActiveVerifiedDomain(row) && (
              <Button size="xs" variant="ghost" href={`https://${row.domain}`} target="_blank">
                <ExternalLink class="size-3.5" />
                {t('resellerConsole.domains.visit')}
              </Button>
            )}
          </div>
        ),
      },
    ])

    return () => (
      <div class="space-y-5">
        <ResellerPageHeader title={t('resellerConsole.domains.title')} description={t('resellerConsole.domains.description')}>
          {{
            actions: () => (
              <Button variant="secondary" size="sm" onClick={() => void d.profile.load()}>
                <RotateCw class="size-4" />
                {t('orders.filters.refresh')}
              </Button>
            ),
          }}
        </ResellerPageHeader>
        <ResellerAlert alert={d.alert.value} />
        {d.profile.loading.value && !d.profile.snapshot.value ? (
          <ResellerPageState loading title={t('resellerConsole.common.loading')} />
        ) : (
          <>
            <Card>
              <div class="flex flex-col gap-4 xl:flex-row xl:items-center xl:justify-between">
                <div class="flex min-w-0 items-start gap-3">
                  <span class="zs-gradient-bg flex size-11 shrink-0 items-center justify-center rounded-zs text-on-primary">
                    <Globe2 class="size-5" />
                  </span>
                  <div class="min-w-0">
                    <div class="flex flex-wrap items-center gap-2">
                      <h2 class="zs-title text-lg text-fg">{t('resellerConsole.domains.workspaceTitle')}</h2>
                      {d.primaryDomain.value && <Badge tone="accent">{t('resellerConsole.domains.primaryBadge')}</Badge>}
                    </div>
                    <p class="mt-1 text-sm text-muted">{t('resellerConsole.domains.workspaceDescription')}</p>
                    <div class="mt-2 break-all font-mono text-sm font-bold text-fg">{d.primaryDomain.value?.domain || t('resellerConsole.domains.noPrimaryDomain')}</div>
                  </div>
                </div>
                <div class="grid grid-cols-3 overflow-hidden rounded-zs border border-line bg-surface-strong">
                  {[
                    { label: t('resellerConsole.domains.primaryTitle'), value: d.primaryDomain.value ? 1 : 0, cls: 'text-fg' },
                    { label: t('resellerConsole.domains.activeTitle'), value: d.activeDomains.value.length, cls: 'text-success-text' },
                    { label: t('resellerConsole.domains.reviewTitle'), value: d.pendingDomains.value.length, cls: 'text-warning-text' },
                  ].map((cell, i) => (
                    <div key={i} class="border-line px-4 py-3 [&:not(:first-child)]:border-l">
                      <div class="text-xs font-bold text-muted">{cell.label}</div>
                      <div class={['zs-num mt-1 text-xl font-bold', cell.cls]}>{cell.value}</div>
                    </div>
                  ))}
                </div>
              </div>
            </Card>

            <Card>
              <div class="grid gap-4 lg:grid-cols-[200px_minmax(0,1fr)]">
                <div class="flex items-start gap-3">
                  <span class="flex size-9 shrink-0 items-center justify-center rounded-zs-sm bg-accent-soft text-accent-text">
                    <Link2 class="size-4" />
                  </span>
                  <div>
                    <h3 class="text-sm font-bold text-fg">{t('resellerConsole.domains.systemDomainTitle')}</h3>
                    <p class="mt-1 text-xs leading-relaxed text-muted">
                      {d.systemDomain.value ? t('resellerConsole.domains.systemAssignedDesc') : t('resellerConsole.domains.systemWaitingDesc')}
                    </p>
                  </div>
                </div>
                {d.systemDomain.value ? (
                  <div class="flex flex-col gap-3 xl:flex-row xl:items-start xl:justify-between">
                    <div class="min-w-0">
                      <div class="break-all font-mono text-base font-bold text-fg">{d.systemDomain.value.domain}</div>
                      <div class="mt-2 flex flex-wrap gap-2">
                        <Badge tone={domainStatusTone(d.systemDomain.value.status)}>{statusLabel(d.systemDomain.value.status)}</Badge>
                        <Badge tone={domainVerificationTone(d.systemDomain.value.verification_status)}>{verifyLabel(d.systemDomain.value.verification_status)}</Badge>
                        <Badge tone="neutral">{typeLabel(d.systemDomain.value.type)}</Badge>
                      </div>
                      <div class="mt-2 text-xs text-muted">
                        {t('personalCenter.reseller.updatedAt')} {formatResellerConsoleDate(d.systemDomain.value.updated_at)}
                      </div>
                    </div>
                    <div class="flex gap-2">
                      <CopyButton value={d.systemDomain.value.domain} label={t('resellerConsole.common.copy')} />
                      <Button size="sm" variant="ghost" href={`https://${d.systemDomain.value.domain}`} target="_blank">
                        <ExternalLink class="size-4" />
                        {t('resellerConsole.domains.visit')}
                      </Button>
                    </div>
                  </div>
                ) : (
                  <div class="flex items-start gap-3 rounded-zs border border-dashed border-line-strong bg-surface-muted px-4 py-4">
                    <CircleAlert class="mt-0.5 size-4 shrink-0 text-warning-text" />
                    <div>
                      <h4 class="text-sm font-bold text-fg">{t('resellerConsole.domains.systemEmptyTitle')}</h4>
                      <p class="mt-1 text-sm leading-relaxed text-muted">{t('resellerConsole.domains.systemEmptyDescription')}</p>
                    </div>
                  </div>
                )}
              </div>
              <div class="zs-divider my-5" />
              <div class="grid gap-4 lg:grid-cols-[200px_minmax(0,1fr)]">
                <div class="flex items-start gap-3">
                  <span class="flex size-9 shrink-0 items-center justify-center rounded-zs-sm bg-primary-soft text-primary-text">
                    <Plus class="size-4" />
                  </span>
                  <div>
                    <h3 class="text-sm font-bold text-fg">{t('personalCenter.reseller.customDomainTitle')}</h3>
                    <p class="mt-1 text-xs leading-relaxed text-muted">{t('resellerConsole.domains.submitCustomDesc')}</p>
                  </div>
                </div>
                <div>
                  {d.canSubmitDomain.value ? (
                    <form
                      class="grid gap-3 md:grid-cols-[minmax(0,1fr)_auto]"
                      onSubmit={(e: Event) => {
                        e.preventDefault()
                        void d.submitDomain()
                      }}
                    >
                      <Input v-model={d.domainInput.value} disabled={d.submitting.value} placeholder={t('personalCenter.reseller.customDomainPlaceholder')} />
                      <Button type="submit" loading={d.submitting.value} disabled={!d.domainInput.value.trim()}>
                        {d.submitting.value ? t('personalCenter.reseller.submittingDomain') : t('personalCenter.reseller.submitDomain')}
                      </Button>
                    </form>
                  ) : (
                    <p class="rounded-zs border border-dashed border-line-strong bg-surface-muted px-4 py-3 text-sm text-muted">{t('resellerConsole.domains.submitDisabledDesc')}</p>
                  )}
                  <p class="mt-2 text-xs leading-relaxed text-muted">{t('resellerConsole.domains.verifyDesc')}</p>
                </div>
              </div>
            </Card>

            <Card>
              <div class="mb-4 flex items-end justify-between gap-2">
                <div>
                  <h3 class="zs-title text-lg text-fg">{t('resellerConsole.domains.customDomains')}</h3>
                  <p class="mt-1 text-sm text-muted">{t('resellerConsole.domains.customDescription')}</p>
                </div>
                <Badge tone="neutral">{String(d.customDomains.value.length)}</Badge>
              </div>
              <DataTable columns={domainColumns} rows={d.customDomains.value} emptyText={t('resellerConsole.domains.customEmptyDescription')} />
            </Card>
          </>
        )}
      </div>
    )
  },
})
