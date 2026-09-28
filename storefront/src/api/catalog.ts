import { api } from './client'
import type {
  Banner,
  CaptchaImage,
  Category,
  Post,
  PostListParams,
  Product,
  ProductListParams,
  PublicMemberLevel,
  SiteConfig,
} from './types'

export const configAPI = {
  get: () => api.get<SiteConfig>('/public/config'),
}

export const productAPI = {
  list: (params?: ProductListParams) => api.get<Product[]>('/public/products', { params }),
  detail: (slug: string) => api.get<Product>(`/public/products/${encodeURIComponent(slug)}`),
}

export const postAPI = {
  list: (params?: PostListParams) => api.get<Post[]>('/public/posts', { params }),
  detail: (slug: string) => api.get<Post>(`/public/posts/${encodeURIComponent(slug)}`),
}

export const bannerAPI = {
  list: (params?: { position?: string; limit?: number }) => api.get<Banner[]>('/public/banners', { params }),
}

export const categoryAPI = {
  list: () => api.get<Category[]>('/public/categories'),
}

export const memberLevelAPI = {
  list: () => api.get<PublicMemberLevel[]>('/public/member-levels'),
}

export const captchaAPI = {
  image: () => api.get<CaptchaImage>('/public/captcha/image'),
}
