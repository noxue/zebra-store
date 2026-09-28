import { computed, reactive } from 'vue'

export type ValidationRule = (value: unknown) => string | true

export const rules = {
  required:
    (msg = 'This field is required'): ValidationRule =>
    (v) => {
      if (v === null || v === undefined || (typeof v === 'string' && v.trim() === '')) return msg
      if (Array.isArray(v) && v.length === 0) return msg
      return true
    },
  minLength:
    (min: number, msg?: string): ValidationRule =>
    (v) => (typeof v === 'string' && v.length < min ? msg || `Minimum ${min} characters` : true),
  maxLength:
    (max: number, msg?: string): ValidationRule =>
    (v) => (typeof v === 'string' && v.length > max ? msg || `Maximum ${max} characters` : true),
  min:
    (min: number, msg?: string): ValidationRule =>
    (v) => {
      if (v === '' || v === null || v === undefined) return true
      const n = Number(v)
      return !Number.isNaN(n) && n < min ? msg || `Minimum value is ${min}` : true
    },
  max:
    (max: number, msg?: string): ValidationRule =>
    (v) => {
      if (v === '' || v === null || v === undefined) return true
      const n = Number(v)
      return !Number.isNaN(n) && n > max ? msg || `Maximum value is ${max}` : true
    },
  email:
    (msg = 'Invalid email'): ValidationRule =>
    (v) => (typeof v === 'string' && v && !/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(v) ? msg : true),
  url:
    (msg = 'Invalid URL'): ValidationRule =>
    (v) => {
      if (typeof v !== 'string' || !v) return true
      try {
        new URL(v)
        return true
      } catch {
        return msg
      }
    },
  numeric:
    (msg = 'Must be a number'): ValidationRule =>
    (v) => (v === '' || v === null || v === undefined || !Number.isNaN(Number(v)) ? true : msg),
  money:
    (msg = 'Invalid amount'): ValidationRule =>
    (v) => (v === '' || v === null || v === undefined || /^-?\d+(\.\d{1,2})?$/.test(String(v).trim()) ? true : msg),
}

export type FieldSchema<T> = Partial<Record<keyof T & string, ValidationRule[]>>

export function useFormValidation<T extends object>(schema: FieldSchema<T>) {
  const errors = reactive<Record<string, string>>({})

  const runRules = (field: string, value: unknown): string => {
    const fieldRules = (schema as Record<string, ValidationRule[] | undefined>)[field] ?? []
    for (const rule of fieldRules) {
      const result = rule(value)
      if (result !== true) return result
    }
    return ''
  }

  const validateField = (field: keyof T & string, value: unknown): boolean => {
    errors[field] = runRules(field, value)
    return errors[field] === ''
  }

  const validate = (data: T): boolean => {
    let valid = true
    for (const field of Object.keys(schema)) {
      const message = runRules(field, (data as Record<string, unknown>)[field])
      errors[field] = message
      if (message) valid = false
    }
    return valid
  }

  const clearErrors = () => {
    for (const key of Object.keys(errors)) errors[key] = ''
  }

  const hasErrors = computed(() => Object.values(errors).some((e) => !!e))

  return { errors, validate, validateField, clearErrors, hasErrors }
}
