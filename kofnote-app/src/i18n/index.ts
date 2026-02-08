import en from './locales/en'
import zhTW from './locales/zh-TW'

export type UiLanguage = 'en' | 'zh-TW'

export const SUPPORTED_LANGUAGES: UiLanguage[] = ['en', 'zh-TW']

export function isSupportedLanguage(value: string | null): value is UiLanguage {
  return value === 'en' || value === 'zh-TW'
}

const dictionaries: Record<UiLanguage, Record<string, string>> = {
  en: en as Record<string, string>,
  'zh-TW': zhTW as Record<string, string>,
}

export type I18nKey = keyof typeof en

type InterpolationValue = string | number

type InterpolationMap = Record<string, InterpolationValue>

function formatTemplate(template: string, values?: InterpolationMap): string {
  if (!values) {
    return template
  }

  return template.replace(/\{\{(.*?)\}\}/g, (_match, raw) => {
    const key = String(raw).trim()
    const value = values[key]
    return value === undefined ? '' : String(value)
  })
}

export function translate(language: UiLanguage, key: I18nKey | string, values?: InterpolationMap): string {
  const langDict = dictionaries[language]
  const fallbackDict = dictionaries.en
  const template = langDict[key] ?? fallbackDict[key] ?? key
  return formatTemplate(template, values)
}

export function getLanguageLabel(target: UiLanguage, displayLanguage: UiLanguage): string {
  const key = `lang.${target}`
  return translate(displayLanguage, key)
}
