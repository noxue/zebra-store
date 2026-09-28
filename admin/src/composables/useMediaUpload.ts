import { ref } from 'vue'
import i18n from '@/i18n'
import { adminAPI } from '@/api/admin'
import { notifyError } from '@/utils/notify'
import { DEFAULT_UPLOAD_MAX_SIZE_BYTES, formatFileSizeLimit, splitFilesBySize } from '@/utils/upload'

const UPLOAD_CONCURRENCY = 3

/** Upload files (3 at a time) into the media library; returns the URLs that succeeded. */
export function useMediaUpload() {
  const uploading = ref(false)
  const progress = ref({ current: 0, total: 0 })

  const uploadFiles = async (files: File[], scene: string): Promise<string[]> => {
    const t = i18n.global.t
    const { accepted, rejected } = splitFilesBySize(files)
    if (rejected.length > 0) {
      notifyError(t('admin.media.errors.fileTooLarge', { count: rejected.length, max: formatFileSizeLimit(DEFAULT_UPLOAD_MAX_SIZE_BYTES) }))
    }
    if (accepted.length === 0) return []
    uploading.value = true
    progress.value = { current: 0, total: accepted.length }
    const urls: string[] = []
    let failed = 0
    for (let i = 0; i < accepted.length; i += UPLOAD_CONCURRENCY) {
      const batch = accepted.slice(i, i + UPLOAD_CONCURRENCY)
      await Promise.allSettled(
        batch.map(async (file) => {
          try {
            const res = await adminAPI.upload(file, scene)
            if (res.data?.url) urls.push(res.data.url)
          } catch {
            failed++
          } finally {
            progress.value = { ...progress.value, current: progress.value.current + 1 }
          }
        }),
      )
    }
    if (failed > 0) {
      notifyError(
        failed === accepted.length
          ? t('admin.media.errors.uploadFailed', { message: String(accepted.length) })
          : t('admin.media.errors.uploadPartialFailed', { fail: failed, total: accepted.length }),
      )
    }
    uploading.value = false
    return urls
  }

  return { uploading, progress, uploadFiles }
}
