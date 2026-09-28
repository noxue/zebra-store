import { computed, reactive, ref, watch } from 'vue'
import i18n from '@/i18n'
import { adminAPI } from '@/api/admin'
import type { AdminAuthzAdmin, AdminAuthzPolicy, AdminPermissionCatalogItem } from '@/api/types'
import { notifyError, notifySuccess } from '@/utils/notify'
import { confirmAction } from '@/utils/confirm'
import { filterAdmins, filterCatalog, groupCatalog, hasCoveredPolicy, normalizeRoles, stripRolePrefix } from './authzUtils'

type TextKey = string
const tx = (key: TextKey, payload?: Record<string, string | number>) => (payload ? i18n.global.t(`admin.authz.${key}`, payload) : i18n.global.t(`admin.authz.${key}`))

/** Page logic for 权限管理: admins CRUD, roles, role policies + permission catalog, admin-role assignment. */
export function useAuthz() {
  // ---- loading flags ----
  const loadingRoles = ref(false)
  const loadingPolicies = ref(false)
  const loadingCatalog = ref(false)
  const loadingAdmins = ref(false)
  const savingAdminRoles = ref(false)
  const savingAdminForm = ref(false)

  // ---- roles & policies ----
  const roles = ref<string[]>([])
  const immutableRoles = ref<Set<string>>(new Set())
  const selectedRole = ref('')
  const newRole = ref('')
  const policies = ref<AdminAuthzPolicy[]>([])
  const policyForm = reactive({ object: '/admin/', action: 'GET' })

  // ---- catalog ----
  const catalogKeyword = ref('')
  const permissionCatalog = ref<AdminPermissionCatalogItem[]>([])
  const collapsedModules = ref<string[]>([])

  // ---- admins ----
  const admins = ref<AdminAuthzAdmin[]>([])
  const adminKeyword = ref('')
  const selectedAdminId = ref(0)
  const selectedAdminRoles = ref<string[]>([])
  const adminFormMode = ref<'create' | 'edit'>('create')
  const adminForm = reactive({ id: 0, username: '', password: '', isSuper: false })

  const isRoleImmutable = (role: string) => immutableRoles.value.has(role)
  const selectedRoleImmutable = computed(() => isRoleImmutable(selectedRole.value))
  const selectedAdmin = computed(() => admins.value.find((a) => a.id === selectedAdminId.value) || null)
  const filteredAdmins = computed(() => filterAdmins(admins.value, adminKeyword.value))
  const filteredCatalog = computed(() => filterCatalog(permissionCatalog.value, catalogKeyword.value))
  const groupedCatalog = computed(() => groupCatalog(filteredCatalog.value))
  const isCovered = (item: AdminPermissionCatalogItem) => hasCoveredPolicy(policies.value, item)

  // ---- catalog collapse ----
  const isModuleCollapsed = (module: string) => collapsedModules.value.includes(module)
  const toggleModule = (module: string) => {
    collapsedModules.value = isModuleCollapsed(module) ? collapsedModules.value.filter((m) => m !== module) : [...collapsedModules.value, module]
  }
  const expandAllModules = () => (collapsedModules.value = [])
  const collapseAllModules = () => (collapsedModules.value = groupedCatalog.value.map((g) => g.module))

  // ---- admin form ----
  const resetAdminForm = () => {
    adminFormMode.value = 'create'
    Object.assign(adminForm, { id: 0, username: '', password: '', isSuper: false })
  }
  const openAdminEditForm = (admin: AdminAuthzAdmin) => {
    adminFormMode.value = 'edit'
    Object.assign(adminForm, { id: admin.id, username: admin.username, password: '', isSuper: Boolean(admin.is_super) })
  }
  const pickAdminForRoles = (admin: AdminAuthzAdmin) => {
    selectedAdminId.value = admin.id
  }

  // ---- fetchers ----
  async function fetchRoles() {
    loadingRoles.value = true
    try {
      const res = await adminAPI.listAuthzRoles()
      const n = normalizeRoles(res.data)
      roles.value = n.roles
      immutableRoles.value = n.immutable
      if (selectedRole.value && !roles.value.includes(selectedRole.value)) {
        selectedRole.value = ''
        policies.value = []
      }
      if (!selectedRole.value && roles.value.length) selectedRole.value = roles.value[0] ?? ''
    } catch {
      /* already notified */
    } finally {
      loadingRoles.value = false
    }
  }

  async function fetchRolePolicies() {
    if (!selectedRole.value) {
      policies.value = []
      return
    }
    loadingPolicies.value = true
    try {
      const res = await adminAPI.getAuthzRolePolicies(selectedRole.value)
      policies.value = Array.isArray(res.data) ? res.data : []
    } catch {
      policies.value = []
    } finally {
      loadingPolicies.value = false
    }
  }

  async function fetchPermissionCatalog() {
    loadingCatalog.value = true
    try {
      const res = await adminAPI.listAuthzPermissionCatalog()
      permissionCatalog.value = Array.isArray(res.data) ? res.data : []
    } catch {
      permissionCatalog.value = []
    } finally {
      loadingCatalog.value = false
    }
  }

  async function fetchAdmins() {
    loadingAdmins.value = true
    try {
      const res = await adminAPI.listAuthzAdmins()
      admins.value = Array.isArray(res.data) ? res.data : []
      if (selectedAdminId.value && !admins.value.some((a) => a.id === selectedAdminId.value)) selectedAdminId.value = 0
      if (!selectedAdminId.value && admins.value.length) selectedAdminId.value = admins.value[0]?.id ?? 0
      if (adminFormMode.value === 'edit' && adminForm.id && !admins.value.some((a) => a.id === adminForm.id)) resetAdminForm()
    } catch {
      /* already notified */
    } finally {
      loadingAdmins.value = false
    }
  }

  async function fetchSelectedAdminRoles() {
    if (!selectedAdminId.value) {
      selectedAdminRoles.value = []
      return
    }
    try {
      const res = await adminAPI.getAuthzAdminRoles(selectedAdminId.value)
      selectedAdminRoles.value = Array.isArray(res.data) ? res.data : []
    } catch {
      selectedAdminRoles.value = []
    }
  }

  // ---- roles ----
  const isRoleChecked = (role: string) => selectedAdminRoles.value.includes(role)
  const toggleAdminRole = (role: string, checked: boolean) => {
    const next = new Set(selectedAdminRoles.value)
    if (checked) next.add(role)
    else next.delete(role)
    selectedAdminRoles.value = Array.from(next)
  }

  async function createRole() {
    const role = newRole.value.trim()
    if (!role) {
      notifyError(tx('roleRequired'))
      return
    }
    try {
      await adminAPI.createAuthzRole({ role })
      newRole.value = ''
      await fetchRoles()
      const created = role.startsWith('role:') ? role : `role:${role}`
      if (roles.value.includes(created)) selectedRole.value = created
      await fetchRolePolicies()
      notifySuccess(tx('roleCreated'))
    } catch {
      /* already notified */
    }
  }

  async function deleteRole(role: string) {
    if (isRoleImmutable(role)) {
      notifyError(tx('immutableRoleHint'))
      return
    }
    const ok = await confirmAction({ description: tx('confirmDeleteRole', { role: stripRolePrefix(role) }), confirmText: tx('delete'), variant: 'destructive' })
    if (!ok) return
    try {
      await adminAPI.deleteAuthzRole(role)
      if (selectedRole.value === role) selectedRole.value = ''
      await fetchRoles()
      await fetchRolePolicies()
      await fetchSelectedAdminRoles()
      notifySuccess(tx('roleDeleted'))
    } catch {
      /* already notified */
    }
  }

  // ---- policies ----
  const guardPolicyEdit = () => {
    if (!selectedRole.value) {
      notifyError(tx('selectRoleFirst'))
      return false
    }
    if (selectedRoleImmutable.value) {
      notifyError(tx('immutableRoleHint'))
      return false
    }
    return true
  }

  async function grantPolicy() {
    if (!guardPolicyEdit()) return
    const object = policyForm.object.trim()
    if (!object) {
      notifyError(tx('objectRequired'))
      return
    }
    try {
      await adminAPI.grantAuthzPolicy({ role: selectedRole.value, object, action: policyForm.action })
      await fetchRolePolicies()
      notifySuccess(tx('policyGranted'))
    } catch {
      /* already notified */
    }
  }

  async function grantCatalogPolicy(item: AdminPermissionCatalogItem) {
    if (!guardPolicyEdit() || isCovered(item)) return
    try {
      await adminAPI.grantAuthzPolicy({ role: selectedRole.value, object: item.object, action: item.method })
      policyForm.object = item.object
      policyForm.action = item.method
      await fetchRolePolicies()
      notifySuccess(tx('policyGranted'))
    } catch {
      /* already notified */
    }
  }

  async function revokePolicy(item: AdminAuthzPolicy) {
    if (selectedRoleImmutable.value) {
      notifyError(tx('immutableRoleHint'))
      return
    }
    const ok = await confirmAction({
      description: tx('confirmRevokePolicy', { object: item.object, action: item.action }),
      confirmText: tx('delete'),
      variant: 'destructive',
    })
    if (!ok) return
    try {
      await adminAPI.revokeAuthzPolicy({ role: selectedRole.value, object: item.object, action: item.action })
      await fetchRolePolicies()
      notifySuccess(tx('policyRevoked'))
    } catch {
      /* already notified */
    }
  }

  // ---- admins ----
  async function submitAdminForm() {
    const username = adminForm.username.trim()
    if (!username) {
      notifyError(tx('adminUsernameRequired'))
      return
    }
    const password = adminForm.password.trim()
    if (adminFormMode.value === 'create') {
      if (!password) {
        notifyError(tx('adminPasswordRequired'))
        return
      }
      savingAdminForm.value = true
      try {
        const res = await adminAPI.createAuthzAdmin({ username, password, is_super: adminForm.isSuper })
        const createdId = Number(res.data?.id || 0)
        await fetchAdmins()
        if (createdId > 0) selectedAdminId.value = createdId
        await fetchSelectedAdminRoles()
        resetAdminForm()
        notifySuccess(tx('adminCreated'))
      } catch {
        /* already notified */
      } finally {
        savingAdminForm.value = false
      }
      return
    }
    if (!adminForm.id) {
      notifyError(tx('selectAdminFirst'))
      return
    }
    savingAdminForm.value = true
    try {
      const payload: { username: string; is_super: boolean; password?: string } = { username, is_super: adminForm.isSuper }
      if (password) payload.password = password
      await adminAPI.updateAuthzAdmin(adminForm.id, payload)
      await fetchAdmins()
      if (selectedAdminId.value === adminForm.id) await fetchSelectedAdminRoles()
      resetAdminForm()
      notifySuccess(tx('adminUpdated'))
    } catch {
      /* already notified */
    } finally {
      savingAdminForm.value = false
    }
  }

  async function deleteAdmin(admin: AdminAuthzAdmin) {
    const ok = await confirmAction({ description: tx('confirmDeleteAdmin', { username: admin.username }), confirmText: tx('delete'), variant: 'destructive' })
    if (!ok) return
    try {
      await adminAPI.deleteAuthzAdmin(admin.id)
      if (selectedAdminId.value === admin.id) {
        selectedAdminId.value = 0
        selectedAdminRoles.value = []
      }
      if (adminFormMode.value === 'edit' && adminForm.id === admin.id) resetAdminForm()
      await fetchAdmins()
      if (selectedAdminId.value) await fetchSelectedAdminRoles()
      notifySuccess(tx('adminDeleted'))
    } catch {
      /* already notified */
    }
  }

  async function resetAdmin2FA(admin: AdminAuthzAdmin) {
    const ok = await confirmAction({ description: tx('adminTotpResetConfirm', { name: admin.username }), variant: 'destructive' })
    if (!ok) return
    try {
      await adminAPI.resetAdmin2FA(admin.id)
      notifySuccess(tx('adminTotpResetSuccess'))
      await fetchAdmins()
    } catch {
      /* already notified */
    }
  }

  async function saveAdminRoles() {
    if (!selectedAdminId.value) {
      notifyError(tx('selectAdminFirst'))
      return
    }
    savingAdminRoles.value = true
    try {
      await adminAPI.setAuthzAdminRoles(selectedAdminId.value, { roles: selectedAdminRoles.value })
      await fetchSelectedAdminRoles()
      await fetchAdmins()
      notifySuccess(tx('adminRolesSaved'))
    } catch {
      /* already notified */
    } finally {
      savingAdminRoles.value = false
    }
  }

  // ---- lifecycle ----
  let initialized = false
  watch(selectedRole, () => initialized && void fetchRolePolicies())
  watch(selectedAdminId, () => initialized && void fetchSelectedAdminRoles())
  watch(catalogKeyword, expandAllModules)

  async function init() {
    await Promise.all([fetchRoles(), fetchAdmins(), fetchPermissionCatalog()])
    initialized = true
    await Promise.all([fetchRolePolicies(), fetchSelectedAdminRoles()])
  }

  return {
    // state
    loadingRoles,
    loadingPolicies,
    loadingCatalog,
    loadingAdmins,
    savingAdminRoles,
    savingAdminForm,
    roles,
    selectedRole,
    newRole,
    policies,
    policyForm,
    catalogKeyword,
    permissionCatalog,
    admins,
    adminKeyword,
    selectedAdminId,
    selectedAdminRoles,
    adminFormMode,
    adminForm,
    // derived
    selectedRoleImmutable,
    selectedAdmin,
    filteredAdmins,
    filteredCatalog,
    groupedCatalog,
    // helpers
    isRoleImmutable,
    isCovered,
    isModuleCollapsed,
    isRoleChecked,
    // actions
    toggleModule,
    expandAllModules,
    collapseAllModules,
    resetAdminForm,
    openAdminEditForm,
    pickAdminForRoles,
    toggleAdminRole,
    createRole,
    deleteRole,
    grantPolicy,
    grantCatalogPolicy,
    revokePolicy,
    submitAdminForm,
    deleteAdmin,
    resetAdmin2FA,
    saveAdminRoles,
    init,
  }
}

export type AuthzPage = ReturnType<typeof useAuthz>
