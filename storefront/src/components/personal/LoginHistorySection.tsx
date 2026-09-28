import { defineComponent } from 'vue'
import { useI18n } from 'vue-i18n'
import { History } from 'lucide-vue-next'
import type { UserLoginLogItem } from '@/api/types'
import { Badge, Card, DataTable, Pagination, columns } from '@/components/ui'
import { useLoginHistory } from '@/composables/personal/useLoginHistory'
import { formatDateTime } from '@/utils/format'
import { DashedNote, PanelHeading } from './PanelParts'

export const LoginHistorySection = defineComponent({
  name: 'LoginHistorySection',
  setup() {
    const { t } = useI18n()
    const h = useLoginHistory(10)
    const cols = () =>
      columns<UserLoginLogItem>([
        { key: 'created_at', title: t('personalCenter.security.loginLogsTime'), render: (r) => <span class="zs-num text-xs">{formatDateTime(r.created_at)}</span> },
        { key: 'status', title: t('personalCenter.security.loginLogsStatus'), render: (r) => <Badge tone={h.statusTone(r.status)}>{h.statusLabel(r.status)}</Badge> },
        { key: 'client_ip', title: t('personalCenter.security.loginLogsIp'), render: (r) => <span class="zs-num text-xs">{r.client_ip || '-'}</span> },
        { key: 'login_source', title: t('zsPersonal.loginSource'), render: (r) => <span class="text-xs text-muted">{r.login_source || '-'}</span> },
        { key: 'fail_reason', title: t('personalCenter.security.loginLogsReason'), render: (r) => <span class="text-xs text-muted">{h.reasonLabel(r.fail_reason)}</span> },
      ])
    return () => (
      <Card>
        <PanelHeading title={t('personalCenter.security.loginLogsTitle')} description={t('personalCenter.security.loginLogsTip')} icon={History} />
        {!h.logs.loading.value && h.logs.rows.value.length === 0 ? (
          <DashedNote>{t('personalCenter.security.loginLogsEmpty')}</DashedNote>
        ) : (
          <DataTable columns={cols()} rows={h.logs.rows.value} loading={h.logs.loading.value && h.logs.rows.value.length === 0} />
        )}
        <div class="mt-3">
          <Pagination page={h.logs.pagination.page} totalPages={h.logs.pagination.total_page} onChange={(pg: number) => void h.logs.load(pg)} />
        </div>
      </Card>
    )
  },
})
