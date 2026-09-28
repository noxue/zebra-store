import { computed, onMounted, reactive, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { affiliateAPI } from '@/api/affiliate'
import { errorMessage } from '@/api/client'
import type { AffiliateCommissionData, AffiliateDashboardData, AffiliateWithdrawData } from '@/api/types'
import { useClipboard } from '@/composables/useClipboard'
import { useAppStore } from '@/stores/app'
import {
  buildPromotionUrl,
  commissionStatusKey,
  commissionStatusTone,
  formatPercent,
  withdrawStatusKey,
  withdrawStatusTone,
} from '@/utils/personal'
import { usePagedList } from './usePagedList'
import { usePanelAlert } from './usePanelAlert'

/** Affiliate: open, dashboard stats, promotion link, commissions, withdraws. */
export function useAffiliatePanel() {
  const { t } = useI18n()
  const appStore = useAppStore()
  const { copy } = useClipboard()
  const { alert, show, clear } = usePanelAlert()

  const loading = ref(true)
  const opening = ref(false)
  const submitting = ref(false)
  const dashboard = ref<AffiliateDashboardData | null>(null)
  const commissions = usePagedList<AffiliateCommissionData>((p) => affiliateAPI.commissions(p))
  const withdraws = usePagedList<AffiliateWithdrawData>((p) => affiliateAPI.withdraws(p))
  const form = reactive({ amount: '', channel: '', account: '' })

  const opened = computed(() => !!dashboard.value?.opened)
  const channelOptions = computed(() =>
    (appStore.config?.affiliate?.withdraw_channels || []).map((c) => String(c || '').trim()).filter(Boolean),
  )
  const promotionUrl = computed(() =>
    buildPromotionUrl(typeof window !== 'undefined' ? window.location.origin : '', dashboard.value?.affiliate_code, dashboard.value?.promotion_path),
  )
  const conversionRate = computed(() => formatPercent(dashboard.value?.conversion_rate))

  const loadDashboard = async () => {
    try {
      dashboard.value = (await affiliateAPI.dashboard()).data || null
    } catch (err) {
      show('error', errorMessage(err, t('personalCenter.affiliate.errors.loadFailed')))
    }
  }
  const reloadLists = async () => {
    if (!opened.value) return
    await Promise.all([commissions.load(1), withdraws.load(1)])
  }

  const open = async () => {
    opening.value = true
    clear()
    try {
      await affiliateAPI.open()
      await loadDashboard()
      await reloadLists()
      show('success', t('personalCenter.affiliate.openSuccess'))
    } catch (err) {
      show('error', errorMessage(err, t('personalCenter.affiliate.errors.openFailed')))
    } finally {
      opening.value = false
    }
  }

  const submitWithdraw = async () => {
    clear()
    if (!form.amount.trim()) return show('warning', t('personalCenter.affiliate.errors.withdrawAmountRequired'))
    if (!form.channel.trim()) return show('warning', t('personalCenter.affiliate.errors.withdrawChannelRequired'))
    if (!form.account.trim()) return show('warning', t('personalCenter.affiliate.errors.withdrawAccountRequired'))
    submitting.value = true
    try {
      await affiliateAPI.applyWithdraw({ amount: form.amount.trim(), channel: form.channel.trim(), account: form.account.trim() })
      form.amount = ''
      form.account = ''
      show('success', t('personalCenter.affiliate.withdrawSuccess'))
      await Promise.all([loadDashboard(), withdraws.load(1)])
    } catch (err) {
      show('error', errorMessage(err, t('personalCenter.affiliate.errors.withdrawFailed')))
    } finally {
      submitting.value = false
    }
  }

  const copyPromotionUrl = () => {
    if (promotionUrl.value) void copy(promotionUrl.value, 'promotion')
  }

  const commissionLabel = (s?: string) => {
    const k = commissionStatusKey(s)
    return k ? t(`personalCenter.affiliate.commissionStatus.${k}`) : s || '-'
  }
  const withdrawLabel = (s?: string) => {
    const k = withdrawStatusKey(s)
    return k ? t(`personalCenter.affiliate.withdrawStatus.${k}`) : s || '-'
  }

  onMounted(async () => {
    loading.value = true
    await loadDashboard()
    await reloadLists()
    loading.value = false
  })

  return {
    alert,
    loading,
    opening,
    submitting,
    dashboard,
    opened,
    channelOptions,
    promotionUrl,
    conversionRate,
    commissions,
    withdraws,
    form,
    open,
    submitWithdraw,
    copyPromotionUrl,
    commissionLabel,
    commissionTone: commissionStatusTone,
    withdrawLabel,
    withdrawTone: withdrawStatusTone,
  }
}
