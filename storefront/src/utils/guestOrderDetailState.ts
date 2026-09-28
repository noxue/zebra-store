export type GuestOrderDetailViewState = 'loading' | 'auth' | 'detail' | 'empty'

interface GuestOrderDetailViewStateInput {
  loading: boolean
  order: unknown
  showAuthForm: boolean
}

export const resolveGuestOrderDetailViewState = ({
  loading,
  order,
  showAuthForm,
}: GuestOrderDetailViewStateInput): GuestOrderDetailViewState => {
  if (loading) return 'loading'
  if (showAuthForm) return 'auth'
  if (order) return 'detail'
  return 'empty'
}
