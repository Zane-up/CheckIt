import { useSyncExternalStore } from "react";
import { t as zh } from "./zh";
import { en } from "./en";

export type Language = "zh-CN" | "en";
let language: Language = "zh-CN";
const listeners = new Set<() => void>();
export function setLanguage(value: string | undefined) {
  const next = value === "en" ? "en" : "zh-CN";
  if (language === next) return;
  language = next;
  document.documentElement.lang = next;
  listeners.forEach(fn => fn());
}
export const getLanguage = () => language;
export const dateLocale = () => language === "en" ? "en-US" : "zh-CN";
export function tr(text: string, ...values: unknown[]): string {
  const template = language === "en" ? en[text] ?? text : text;
  return template.replace(/\{(\d+)\}/g, (_, index: string) => String(values[Number(index)]));
}
export function errorText(error: unknown): string {
  const text = error instanceof Error ? error.message : String(error);
  if (language !== "en" || !/\p{Script=Han}/u.test(text)) return text;
  if (en[text]) return en[text];
  console.error("CheckIt operation failed:", error);
  return "The operation failed. Please retry. Technical details are available in the application log.";
}
function translateObject<T extends object>(source: T): T {
  return new Proxy(source, { get(target, property) {
    const value = Reflect.get(target, property);
    return typeof value === "string" ? tr(value) : value && typeof value === "object" ? translateObject(value) : value;
  } });
}
export const t = translateObject(zh);
function subscribe(fn: () => void) { listeners.add(fn); return () => { listeners.delete(fn); }; }
export function useI18n() {
  const locale = useSyncExternalStore(subscribe, getLanguage, getLanguage);
  return { locale, t, tr };
}
export function widgetDate(now: number) {
  const date = new Date(now);
  return language === "en" ? date.toLocaleDateString("en-US", {month:"short",day:"numeric",weekday:"short"})
    : `${date.getMonth()+1}月${date.getDate()}日 周${"日一二三四五六"[date.getDay()]}`;
}
