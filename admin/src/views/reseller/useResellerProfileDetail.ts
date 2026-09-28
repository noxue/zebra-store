import { computed, reactive, ref, type Ref } from 'vue'
import { adminAPI } from '@/api/admin'
import type { AdminResellerDomain, AdminResellerProfileDetail, AdminResellerProfileUpdatePayload } from '@/api/types'
import i18n from '@/i18n'
import { confirmAction } from '@/utils/confirm'
import { notifyError, notifySuccess } from '@/utils/notify'

export const isActiveVerifiedDomain = (d: AdminResellerDomain) => d.status === 'active' && d.verification_status === 'verified'
export const canSetPrimaryDomain = (d: AdminResellerDomain) => !d.is_primary && isActiveVerifiedDomain(d)

export function useResellerProfileDetail(profileId: Ref<number>) {
  const t = i18n.global.t
  const loading = ref(true)
  const saving = ref(false)
  const savingSystemDomain = ref(false)
  const operatingDomainId = ref<number | null>(null)
  const detail = ref<AdminResellerProfileDetail | null>(null)
  const showEditDialog = ref(false)
  const systemDomainForm = reactive({ subdomain: '' })

  const profile = computed(() => detail.value?.profile ?? null)
  const domains = computed(() => detail.value?.domains ?? [])
  const primaryDomain = computed(() => domains.value.filter(isActiveVerifiedDomain).find((d) => d.is_primary) ?? null)
  const systemDomain = computed(() => domains.value.find((d) => d.type === 'subdomain') ?? null)

  const fetchDetail = async () => {
    if (!profileId.value) {
      loading.value = false
      return
    }
    loading.value = true
    try {
      const res = await adminAPI.getResellerProfile(profileId.value)
      detail.value = res.data ?? null
      systemDomainForm.subdomain = systemDomain.value?.domain || ''
    } catch {
      detail.value = null
    } finally {
      loading.value = false
    }
  }

  const submitEdit = async (payload: AdminResellerProfileUpdatePayload) => {
    if (!profile.value) return
    saving.value = true
    try {
      await adminAPI.updateResellerProfile(profile.value.id, payload)
      showEditDialog.value = false
      notifySuccess(t('admin.resellerProfileDetail.saveSuccess'))
      await fetchDetail()
    } catch {
      /* already notified */
    } finally {
      saving.value = false
    }
  }

  const submitSystemDomain = async () => {
    if (!profile.value) return
    const raw = systemDomainForm.subdomain.trim()
    if (!raw) return notifyError(t('admin.resellerProfileDetail.systemDomain.emptyPrompt'))
    savingSystemDomain.value = true
    try {
      await adminAPI.assignResellerSystemDomain(profile.value.id, {
        subdomain: raw,
      })
      notifySuccess(t('admin.resellerProfileDetail.systemDomain.saveSuccess'))
      await fetchDetail()
    } catch {
      /* already notified */
    } finally {
      savingSystemDomain.value = false
    }
  }

  const setPrimary = async (domain: AdminResellerDomain) => {
    if (domain.is_primary) return
    if (
      !(await confirmAction({
        description: t('admin.resellerProfileDetail.setPrimaryConfirm', {
          domain: domain.domain,
          id: domain.reseller_id,
        }),
      }))
    )
      return
    operatingDomainId.value = domain.id
    try {
      await adminAPI.setPrimaryResellerDomain(domain.id)
      notifySuccess(t('admin.resellerProfileDetail.setPrimarySuccess'))
      await fetchDetail()
    } catch {
      /* already notified */
    } finally {
      operatingDomainId.value = null
    }
  }

  return {
    loading,
    saving,
    savingSystemDomain,
    operatingDomainId,
    detail,
    profile,
    domains,
    primaryDomain,
    systemDomain,
    showEditDialog,
    systemDomainForm,
    fetchDetail,
    submitEdit,
    submitSystemDomain,
    setPrimary,
  }
}
