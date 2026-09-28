import { defineStore } from 'pinia'
import { computed, ref } from 'vue'
import { memberLevelAPI } from '@/api/catalog'
import { userOrderAPI } from '@/api/order'
import { userProfileAPI } from '@/api/user'
import type {
  ChangeEmailPayload,
  ChangeUserPasswordPayload,
  GoogleBindingData,
  Order,
  PublicMemberLevel,
  SendChangeEmailCodePayload,
  TelegramAuthPayload,
  TelegramBindingData,
  UpdateUserProfilePayload,
  UserLoginLogItem,
  UserProfileData,
} from '@/api/types'
import { computeUpgradeProgress, resolveNextLevel } from '@/utils/personal'
import { useUserAuthStore } from './userAuth'

const messageOf = (error: unknown, fallback: string) =>
  error instanceof Error && error.message.trim() !== '' ? error.message : fallback

/** Personal center data: profile, member levels, recent orders, login logs, external bindings. */
export const useUserProfileStore = defineStore('user-profile', () => {
  const userAuthStore = useUserAuthStore()

  const profile = ref<UserProfileData | null>(null)
  const recentOrders = ref<Order[]>([])
  const ordersTotal = ref(0)
  const recentLoginLogs = ref<UserLoginLogItem[]>([])
  const telegramBinding = ref<TelegramBindingData | null>(null)
  const googleBinding = ref<GoogleBindingData | null>(null)
  const memberLevels = ref<PublicMemberLevel[]>([])

  const loadingProfile = ref(false)
  const savingProfile = ref(false)
  const loadingOrders = ref(false)
  const loadingLoginLogs = ref(false)
  const loadingTelegramBinding = ref(false)
  const bindingTelegram = ref(false)
  const unbindingTelegram = ref(false)
  const loadingGoogleBinding = ref(false)
  const bindingGoogle = ref(false)
  const unbindingGoogle = ref(false)
  const sendingCode = ref(false)
  const changingEmail = ref(false)
  const changingPassword = ref(false)
  const profileError = ref('')
  const securityError = ref('')

  const displayName = computed(() => {
    const nick = profile.value?.nickname?.trim()
    if (nick) return nick
    return profile.value?.email || userAuthStore.user?.nickname || userAuthStore.user?.email || '-'
  })

  const currentLevel = computed(() => {
    const id = profile.value?.member_level_id
    if (!id) return null
    return memberLevels.value.find((l) => l.id === id) || null
  })

  const nextLevel = computed(() => resolveNextLevel(memberLevels.value, currentLevel.value))

  const upgradeProgress = computed(() =>
    computeUpgradeProgress(nextLevel.value, profile.value?.total_recharged, profile.value?.total_spent),
  )

  const clearProfileError = () => {
    profileError.value = ''
  }
  const clearSecurityError = () => {
    securityError.value = ''
  }

  const run = async (flag: { value: boolean }, fn: () => Promise<void>, onError: (e: unknown) => void): Promise<boolean> => {
    flag.value = true
    try {
      await fn()
      return true
    } catch (error) {
      onError(error)
      return false
    } finally {
      flag.value = false
    }
  }

  const loadMemberLevels = async () => {
    try {
      const res = await memberLevelAPI.list()
      memberLevels.value = Array.isArray(res.data) ? res.data : []
    } catch {
      memberLevels.value = []
    }
  }

  const loadProfile = () => {
    clearProfileError()
    return run(
      loadingProfile,
      async () => {
        const res = await userProfileAPI.current()
        profile.value = res.data
        userAuthStore.syncUserProfile(res.data)
      },
      (e) => {
        profile.value = null
        profileError.value = messageOf(e, 'load profile failed')
      },
    )
  }

  const saveProfile = (payload: UpdateUserProfilePayload) => {
    clearProfileError()
    return run(
      savingProfile,
      async () => {
        const res = await userProfileAPI.updateProfile(payload)
        profile.value = res.data
        userAuthStore.syncUserProfile(res.data)
      },
      (e) => {
        profileError.value = messageOf(e, 'save profile failed')
      },
    )
  }

  const securityFail = (e: unknown) => {
    securityError.value = messageOf(e, '')
  }

  const sendChangeEmailCode = (payload: SendChangeEmailCodePayload) => {
    clearSecurityError()
    return run(sendingCode, async () => void (await userProfileAPI.sendChangeEmailCode(payload)), securityFail)
  }

  const changeEmail = (payload: ChangeEmailPayload) => {
    clearSecurityError()
    return run(
      changingEmail,
      async () => {
        const res = await userProfileAPI.changeEmail(payload)
        if (res.data) {
          profile.value = res.data
          userAuthStore.syncUserProfile(res.data)
        }
      },
      securityFail,
    )
  }

  const changePassword = (payload: ChangeUserPasswordPayload) => {
    clearSecurityError()
    return run(changingPassword, async () => void (await userProfileAPI.changePassword(payload)), securityFail)
  }

  const loadRecentOrders = (limit = 5) =>
    run(
      loadingOrders,
      async () => {
        const res = await userOrderAPI.list({ page: 1, page_size: limit })
        recentOrders.value = Array.isArray(res.data) ? res.data : []
        ordersTotal.value = res.pagination?.total ?? recentOrders.value.length
      },
      () => {
        recentOrders.value = []
        ordersTotal.value = 0
      },
    )

  const loadRecentLoginLogs = (limit = 5) =>
    run(
      loadingLoginLogs,
      async () => {
        const res = await userProfileAPI.loginLogs({ page: 1, page_size: limit })
        recentLoginLogs.value = Array.isArray(res.data) ? res.data : []
      },
      () => {
        recentLoginLogs.value = []
      },
    )

  const loadTelegramBinding = () => {
    clearSecurityError()
    return run(
      loadingTelegramBinding,
      async () => {
        telegramBinding.value = (await userProfileAPI.getTelegramBinding()).data || { bound: false }
      },
      (e) => {
        telegramBinding.value = null
        securityFail(e)
      },
    )
  }

  const bindTelegram = (payload: TelegramAuthPayload) => {
    clearSecurityError()
    return run(
      bindingTelegram,
      async () => {
        telegramBinding.value = (await userProfileAPI.bindTelegram(payload)).data || { bound: true }
      },
      securityFail,
    )
  }

  const bindTelegramMiniApp = (initData: string) => {
    clearSecurityError()
    return run(
      bindingTelegram,
      async () => {
        telegramBinding.value = (await userProfileAPI.bindTelegramMiniApp({ init_data: initData })).data || { bound: true }
      },
      securityFail,
    )
  }

  const unbindTelegram = () => {
    clearSecurityError()
    return run(
      unbindingTelegram,
      async () => {
        await userProfileAPI.unbindTelegram()
        telegramBinding.value = { bound: false }
      },
      securityFail,
    )
  }

  const loadGoogleBinding = () => {
    clearSecurityError()
    return run(
      loadingGoogleBinding,
      async () => {
        googleBinding.value = (await userProfileAPI.getGoogleBinding()).data || { bound: false }
      },
      (e) => {
        googleBinding.value = null
        securityFail(e)
      },
    )
  }

  const bindGoogle = (credential: string) => {
    clearSecurityError()
    return run(
      bindingGoogle,
      async () => {
        googleBinding.value = (await userProfileAPI.bindGoogle({ credential })).data || { bound: true }
      },
      securityFail,
    )
  }

  const exchangeGoogleRedirectBind = () => {
    clearSecurityError()
    return run(
      bindingGoogle,
      async () => {
        googleBinding.value = (await userProfileAPI.googleRedirectBindExchange()).data || { bound: true }
      },
      securityFail,
    )
  }

  const unbindGoogle = () => {
    clearSecurityError()
    return run(
      unbindingGoogle,
      async () => {
        await userProfileAPI.unbindGoogle()
        googleBinding.value = { bound: false }
      },
      securityFail,
    )
  }

  return {
    profile,
    recentOrders,
    ordersTotal,
    recentLoginLogs,
    telegramBinding,
    googleBinding,
    memberLevels,
    currentLevel,
    nextLevel,
    upgradeProgress,
    loadingProfile,
    savingProfile,
    loadingOrders,
    loadingLoginLogs,
    loadingTelegramBinding,
    bindingTelegram,
    unbindingTelegram,
    loadingGoogleBinding,
    bindingGoogle,
    unbindingGoogle,
    sendingCode,
    changingEmail,
    changingPassword,
    profileError,
    securityError,
    displayName,
    clearProfileError,
    clearSecurityError,
    loadProfile,
    saveProfile,
    sendChangeEmailCode,
    changeEmail,
    changePassword,
    loadRecentOrders,
    loadMemberLevels,
    loadRecentLoginLogs,
    loadTelegramBinding,
    bindTelegram,
    bindTelegramMiniApp,
    unbindTelegram,
    loadGoogleBinding,
    bindGoogle,
    exchangeGoogleRedirectBind,
    unbindGoogle,
  }
})
