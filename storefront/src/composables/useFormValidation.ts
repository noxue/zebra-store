import { computed, reactive } from 'vue'
import { useI18n } from 'vue-i18n'
import { isValidEmail } from '@/utils/auth/validation'

export type ValidationRule = (value: string) => string | null

/** Field-level validation with touched tracking (port of the original). */
export function useFormValidation<F extends string>(fields: F[]) {
  const { t } = useI18n()
  const errors = reactive<Record<string, string>>({})
  const touched = reactive<Record<string, boolean>>({})
  const rules: Partial<Record<F, ValidationRule[]>> = {}

  const addRule = (field: F, rule: ValidationRule) => {
    ;(rules[field] ||= []).push(rule)
  }

  const validateField = (field: F, value: string): string | null => {
    for (const rule of rules[field] || []) {
      const error = rule(value)
      if (error) {
        errors[field] = error
        return error
      }
    }
    errors[field] = ''
    return null
  }

  const touchField = (field: F, value: string) => {
    touched[field] = true
    validateField(field, value)
  }

  const validateAll = (values: Partial<Record<F, string>>): boolean => {
    let valid = true
    for (const field of fields) {
      touched[field] = true
      if (validateField(field, values[field] || '')) valid = false
    }
    return valid
  }

  const getError = (field: F): string => (touched[field] ? errors[field] || '' : '')
  const hasError = (field: F): boolean => getError(field) !== ''
  const isValid = computed(() => fields.every((f) => !errors[f]))
  const clearErrors = () => {
    Object.keys(errors).forEach((k) => (errors[k] = ''))
    Object.keys(touched).forEach((k) => (touched[k] = false))
  }

  const requiredRule = (): ValidationRule => (value) => (value.trim() ? null : t('formValidation.required'))
  const emailRule = (): ValidationRule => (value) => (!value.trim() || isValidEmail(value) ? null : t('formValidation.emailInvalid'))
  const minLengthRule =
    (min: number): ValidationRule =>
    (value) =>
      !value || value.length >= min ? null : t('formValidation.passwordTooShort', { min })

  return { errors, touched, addRule, validateField, touchField, validateAll, getError, hasError, isValid, clearErrors, requiredRule, emailRule, minLengthRule }
}
