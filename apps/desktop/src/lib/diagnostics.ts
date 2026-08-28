import { providerLabel, type FoundationStatus, type ProviderId } from "./foundation";
import { localeCode, type UiLocale } from "./i18n";

export interface ProviderHealth {
  provider: ProviderId;
  lastAttempt: string | null;
  lastSuccess: string | null;
  lastErrorCode: string | null;
  lastErrorMessage: string | null;
  latencyMs: number | null;
  consecutiveFailures: number;
}

export interface DiagnosticsStatus {
  generatedAt: string;
  foundation: FoundationStatus;
  providers: ProviderHealth[];
}

export interface DiagnosticsExportResult {
  path: string;
  bytes: number;
}

export type ProviderCondition = "ok" | "degraded" | "error" | "unchecked";

export interface ProviderDiagnosticRow extends ProviderHealth {
  label: string;
  condition: ProviderCondition;
}

const EXPECTED_PROVIDERS: ProviderId[] = [
  "relics_run",
  "frame_forge_mirror",
  "warframe_market",
];

export function providerCondition(health: ProviderHealth): ProviderCondition {
  if (!health.lastAttempt) return "unchecked";
  if (!health.lastErrorCode) return "ok";
  return health.lastSuccess ? "degraded" : "error";
}

export function providerDiagnosticRows(status: DiagnosticsStatus, locale: UiLocale = "ru"): ProviderDiagnosticRow[] {
  return EXPECTED_PROVIDERS.map((provider) => {
    const health = status.providers.find((row) => row.provider === provider) ?? {
      provider,
      lastAttempt: null,
      lastSuccess: null,
      lastErrorCode: null,
      lastErrorMessage: null,
      latencyMs: null,
      consecutiveFailures: 0,
    };
    return {
      ...health,
      label: providerLabel(provider, locale),
      condition: providerCondition(health),
    };
  });
}

export function providerConditionLabel(condition: ProviderCondition, locale: UiLocale = "ru"): string {
  return (locale === "en" ? {
    ok: "Working",
    degraded: "Saved data available",
    error: "Unavailable",
    unchecked: "Not checked yet",
  } : {
    ok: "Работает",
    degraded: "Есть сохранённые данные",
    error: "Недоступен",
    unchecked: "Ещё не проверялся",
  })[condition];
}

export function formatDiagnosticDate(value: string | null, locale: UiLocale = "ru"): string {
  if (!value) return "—";
  const date = new Date(value);
  return Number.isNaN(date.getTime())
    ? locale === "en" ? "Invalid date" : "Некорректная дата"
    : new Intl.DateTimeFormat(localeCode(locale), {
        dateStyle: "short",
        timeStyle: "medium",
      }).format(date);
}

export function formatLatency(value: number | null, locale: UiLocale = "ru"): string {
  return value === null ? "—" : `${value.toLocaleString(localeCode(locale))} ${locale === "en" ? "ms" : "мс"}`;
}
