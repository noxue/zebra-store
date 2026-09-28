import { computed, reactive, ref, type Ref } from 'vue'
import i18n from '@/i18n'
import { adminAPI } from '@/api/admin'
import type {
  AdminMemberLevel,
  AdminOrder,
  AdminPayment,
  AdminUser,
  AdminUserOAuthIdentity,
  AdminWalletAccount,
  AdminWalletTransaction,
} from '@/api/types'
import { useListPage } from '@/composables/useListPage'
import { confirmAction } from '@/utils/confirm'
import { notifySuccess } from '@/utils/notify'
import { formatOAuthIdentityAccount, managedOAuthProvider } from '@/utils/oauthIdentity'
import { fetchAllMemberLevels, fetchSiteCurrency, validateWalletAdjust } from './usersUtils'

export interface UserDetailCouponUsage {
  id: number
  coupon_code?: string
  coupon_id?: number
  coupon_type?: string
  scope_products?: Array<{ title: Record<string, string> }>
  order_id?: number
  discount_amount?: number | string
  created_at: string
}

export type UserDetailTab = 'orders' | 'payments' | 'coupons' | 'wallet'

const errMessage = (err: unknown, fallback: string) => (err instanceof Error && err.message ? err.message : fallback)

/** Page logic for 用户详情 (profile, OAuth, 2FA, member level, wallet, history tabs). */
export function useUserDetail(userId: Ref<number>) {
  const t = i18n.global.t
  const validId = () => Number.isFinite(userId.value) && userId.value > 0

  const user = ref<AdminUser | null>(null)
  const userError = ref('')
  const siteCurrency = ref('CNY')
  const memberLevels = ref<AdminMemberLevel[]>([])
  const memberLevelUpdating = ref(false)
  const activeTab = ref<UserDetailTab>('orders')

  const fetchUser = async () => {
    if (!validId()) return
    userError.value = ''
    try {
      user.value = (await adminAPI.getUser(userId.value)).data
    } catch (err) {
      userError.value = errMessage(err, t('admin.userDetail.fetchFailed'))
    }
  }

  const changeMemberLevel = async (value: unknown) => {
    const levelId = Number(value)
    if (!Number.isFinite(levelId) || !user.value) return
    memberLevelUpdating.value = true
    try {
      await adminAPI.setUserMemberLevel(userId.value, levelId)
      user.value = { ...user.value, member_level_id: levelId }
      notifySuccess(t('admin.common.operationSuccess'))
    } catch {
      /* already notified */
    } finally {
      memberLevelUpdating.value = false
    }
  }

  // ---- tab lists ----
  const orders = useListPage<AdminOrder>({
    fetchFn: (page, pageSize) => adminAPI.getOrders({ page, page_size: pageSize, user_id: userId.value }),
  })
  const payments = useListPage<AdminPayment>({
    fetchFn: (page, pageSize) => adminAPI.getPayments({ page, page_size: pageSize, user_id: userId.value }),
  })
  const coupons = useListPage<UserDetailCouponUsage>({
    fetchFn: async (page, pageSize) => {
      const res = await adminAPI.getUserCouponUsages(userId.value, { page, page_size: pageSize })
      return { items: (Array.isArray(res.data) ? res.data : []) as unknown as UserDetailCouponUsage[], pagination: res.pagination }
    },
  })
  const walletTx = useListPage<AdminWalletTransaction>({
    fetchFn: (page, pageSize) => adminAPI.getUserWalletTransactions(userId.value, { page, page_size: pageSize }),
  })

  // ---- wallet ----
  const walletAccount = ref<AdminWalletAccount | null>(null)
  const walletError = ref('')
  const walletSuccess = ref('')
  const walletSubmitting = ref(false)
  const walletForm = reactive({ operation: 'add' as 'add' | 'subtract', amount: '', remark: '' })

  const fetchWalletAccount = async () => {
    walletAccount.value = (await adminAPI.getUserWallet(userId.value)).data?.account || null
  }

  const loadWalletData = async (page = walletTx.pagination.value.page) => {
    if (!validId()) return
    walletError.value = ''
    try {
      await Promise.all([fetchWalletAccount(), walletTx.fetchData(page)])
    } catch (err) {
      walletError.value = errMessage(err, t('admin.userDetail.wallet.errors.loadFailed'))
    }
  }

  const submitWalletAdjust = async () => {
    if (!validId()) return
    walletError.value = ''
    walletSuccess.value = ''
    const invalid = validateWalletAdjust(walletForm.amount, walletForm.remark)
    if (invalid) {
      walletError.value = t(`admin.userDetail.wallet.errors.${invalid}`)
      return
    }
    walletSubmitting.value = true
    try {
      const res = await adminAPI.adjustUserWallet(userId.value, {
        operation: walletForm.operation,
        amount: walletForm.amount.trim(),
        remark: walletForm.remark.trim(),
      })
      const account = (res.data as { account?: AdminWalletAccount } | undefined)?.account
      if (account) walletAccount.value = account
      walletForm.amount = ''
      walletForm.remark = ''
      await Promise.all([fetchUser(), walletTx.fetchData(1)])
      walletSuccess.value = t('admin.userDetail.wallet.adjustSuccess')
      notifySuccess(walletSuccess.value)
    } catch (err) {
      walletError.value = errMessage(err, t('admin.userDetail.wallet.errors.adjustFailed'))
    } finally {
      walletSubmitting.value = false
    }
  }

  // ---- tabs ----
  const changeTab = (tab: UserDetailTab) => {
    activeTab.value = tab
    if (!validId()) return
    if (tab === 'orders') void orders.fetchData(orders.pagination.value.page)
    else if (tab === 'payments') void payments.fetchData(payments.pagination.value.page)
    else if (tab === 'coupons') void coupons.fetchData(coupons.pagination.value.page)
    else {
      walletSuccess.value = ''
      void loadWalletData(walletTx.pagination.value.page)
    }
  }

  // ---- OAuth ----
  const oauthIdentities = computed<AdminUserOAuthIdentity[]>(() => (Array.isArray(user.value?.oauth_identities) ? user.value.oauth_identities : []))
  const oauthUnbindingId = ref<number | null>(null)
  const oauthError = ref('')
  const oauthSuccess = ref('')

  const unbindOAuthIdentity = async (identity: AdminUserOAuthIdentity) => {
    const provider = managedOAuthProvider(identity)
    if (!validId() || !provider) return
    const isGoogle = provider === 'google'
    const ok = await confirmAction({
      description: t(isGoogle ? 'admin.userDetail.oauth.confirmUnbindGoogle' : 'admin.userDetail.oauth.confirmUnbindTelegram', {
        account: formatOAuthIdentityAccount(identity),
      }),
      confirmText: t(isGoogle ? 'admin.userDetail.oauth.unbindGoogle' : 'admin.userDetail.oauth.unbindTelegram'),
      variant: 'destructive',
    })
    if (!ok) return
    oauthError.value = ''
    oauthSuccess.value = ''
    oauthUnbindingId.value = identity.id
    try {
      if (isGoogle) await adminAPI.unbindUserGoogle(userId.value)
      else await adminAPI.unbindUserTelegram(userId.value)
      await fetchUser()
      oauthSuccess.value = t(isGoogle ? 'admin.userDetail.oauth.unbindGoogleSuccess' : 'admin.userDetail.oauth.unbindTelegramSuccess')
    } catch (err) {
      oauthError.value = errMessage(err, t(isGoogle ? 'admin.userDetail.oauth.unbindGoogleFailed' : 'admin.userDetail.oauth.unbindTelegramFailed'))
    } finally {
      oauthUnbindingId.value = null
    }
  }

  // ---- 2FA ----
  const twofaEnabledAt = computed(() => (typeof user.value?.totp_enabled_at === 'string' ? user.value.totp_enabled_at : ''))
  const twofaEnabled = computed(() => Boolean(twofaEnabledAt.value))
  const twofaResetting = ref(false)
  const twofaError = ref('')
  const twofaSuccess = ref('')

  const resetUser2FA = async () => {
    if (!validId() || !twofaEnabled.value) return
    const email = user.value?.email || `#${userId.value}`
    const ok = await confirmAction({
      description: t('admin.userDetail.twofa.confirmReset', { email }),
      confirmText: t('admin.userDetail.twofa.reset'),
      variant: 'destructive',
    })
    if (!ok) return
    twofaError.value = ''
    twofaSuccess.value = ''
    twofaResetting.value = true
    try {
      await adminAPI.resetUser2FA(userId.value)
      twofaSuccess.value = t('admin.userDetail.twofa.resetSuccess')
      await fetchUser()
    } catch (err) {
      twofaError.value = errMessage(err, t('admin.userDetail.twofa.resetFailed'))
    } finally {
      twofaResetting.value = false
    }
  }

  /** Initial load (mount). */
  const init = async () => {
    fetchSiteCurrency().then((c) => (siteCurrency.value = c))
    void fetchUser()
    if (validId()) void orders.fetchData(1)
    memberLevels.value = await fetchAllMemberLevels()
  }

  /** Reload everything when the route id changes. */
  const reloadForUser = () => {
    if (!validId()) return
    user.value = null
    void fetchUser()
    void orders.fetchData(1)
    void payments.fetchData(1)
    void coupons.fetchData(1)
    void loadWalletData(1)
  }

  return {
    user,
    userError,
    siteCurrency,
    memberLevels,
    memberLevelUpdating,
    changeMemberLevel,
    activeTab,
    changeTab,
    orders,
    payments,
    coupons,
    walletTx,
    walletAccount,
    walletError,
    walletSuccess,
    walletSubmitting,
    walletForm,
    loadWalletData,
    submitWalletAdjust,
    oauthIdentities,
    oauthUnbindingId,
    oauthError,
    oauthSuccess,
    unbindOAuthIdentity,
    twofaEnabled,
    twofaEnabledAt,
    twofaResetting,
    twofaError,
    twofaSuccess,
    resetUser2FA,
    init,
    reloadForUser,
  }
}
