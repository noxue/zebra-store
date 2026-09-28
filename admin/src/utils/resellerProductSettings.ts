export const resellerProductSettingsPermission = 'GET:/admin/resellers/product-settings'

type AdminResellerProductSettingLike = {
  reseller_id: number
  profile?: { user?: { email?: string; display_name?: string } }
}

export const buildResellerProductSettingStatusClass = (listed?: boolean) => {
  if (listed === false) return 'border-danger/40 bg-danger-soft text-danger-text'
  return 'border-success/40 bg-success-soft text-success-text'
}

export const getAdminResellerProductSettingOwnerLabel = (row: AdminResellerProductSettingLike) => {
  const email = row.profile?.user?.email?.trim()
  if (email) return email
  const displayName = row.profile?.user?.display_name?.trim()
  if (displayName) return displayName
  return `#${row.reseller_id}`
}
