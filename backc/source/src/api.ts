import type {
  DatabaseConfigResponse, HealthResponse, LogsResponse, MonitoringHistoryResponse,
  MonitoringHostDetailResponse, MonitoringHostsResponse, ServiceStatus,
  SunshineApp, SunshineAppsResponse, SunshineClientsResponse, SunshineConfig,
  SunshineHostInfo, SunshineHostSaveRequest, SystemResources,
} from "./types";
import { monitoringHostPath, pathSegment, sunshineHostPath } from "./api-paths";

const REQUEST_TIMEOUT_MS = 15_000;
type ApiRequestInit = RequestInit & { timeoutMs?: number; suppressAuthExpired?: boolean };

export class ApiError extends Error {
  constructor(message: string, readonly code: string | undefined, readonly status: number) {
    super(message); this.name = "ApiError";
  }
}

async function readApiError(response: Response): Promise<ApiError> {
  const text = await response.text().catch(() => "");
  const fallback = text || `${response.status} ${response.statusText}`;
  try {
    const payload = JSON.parse(text) as Record<string, unknown>;
    const code = typeof payload.code === "string" ? payload.code : undefined;
    const message = ["message", "error", "detail"]
      .map((key) => payload[key]).find((value) => typeof value === "string" && value.trim());
    return new ApiError(typeof message === "string" ? message : fallback, code, response.status);
  } catch { return new ApiError(fallback, undefined, response.status); }
}

async function request<T>(path: string, init?: ApiRequestInit): Promise<T> {
  const { timeoutMs = REQUEST_TIMEOUT_MS, suppressAuthExpired = false, ...fetchInit } = init ?? {};
  const controller = new AbortController();
  let didTimeout = false;
  const timeoutId = timeoutMs > 0 ? window.setTimeout(() => { didTimeout = true; controller.abort(); }, timeoutMs) : undefined;
  const callerSignal = fetchInit.signal;
  const abortFromCaller = () => controller.abort(callerSignal?.reason);
  if (callerSignal?.aborted) abortFromCaller();
  else callerSignal?.addEventListener("abort", abortFromCaller, { once: true });
  let response: Response;
  try {
    const shouldSendJson = Boolean(fetchInit.body) && !(fetchInit.body instanceof FormData);
    response = await fetch(path, {
      ...fetchInit, credentials: "include", signal: controller.signal,
      headers: {
        ...(shouldSendJson ? { "Content-Type": "application/json" } : undefined),
        ...(!fetchInit.method || ["GET", "HEAD", "OPTIONS"].includes(fetchInit.method.toUpperCase()) ? undefined : { "X-CSRF-Token": "1" }),
        ...fetchInit.headers,
      },
    });
  } catch (error) {
    if (error instanceof DOMException && error.name === "AbortError") throw new Error(didTimeout ? "请求超时，请检查 UnionC 是否可用" : "请求已取消");
    throw error;
  } finally {
    if (timeoutId !== undefined) window.clearTimeout(timeoutId);
    callerSignal?.removeEventListener("abort", abortFromCaller);
  }
  if (response.status === 401) {
    if (!suppressAuthExpired) window.dispatchEvent(new Event("unionc:auth-expired"));
    throw new ApiError("认证已失效，请重新登录", "unauthorized", 401);
  }
  if (!response.ok) throw await readApiError(response);
  if (response.status === 204) return undefined as T;
  return await response.json() as T;
}

export const api = {
  authenticate: () => request<{ username: string }>("/api/auth/me", { suppressAuthExpired: true }),
  login: async (username: string, password: string) => {
    try {
      return await request<{ username: string }>("/api/auth/login", { method: "POST", body: JSON.stringify({ username, password }), suppressAuthExpired: true });
    } catch (error) {
      if (error instanceof ApiError && error.status === 401) throw new ApiError("账号或密码错误", error.code, 401);
      throw error;
    }
  },
  logout: () => request<void>("/api/auth/logout", { method: "POST" }),
  me: () => request<{ username: string }>("/api/auth/me"),
  changePassword: (current_password: string, new_password: string) => request<void>("/api/auth/change-password", { method: "POST", body: JSON.stringify({ current_password, new_password }) }),
  health: () => request<HealthResponse>("/api/health"),
  services: () => request<ServiceStatus[]>("/api/services"),
  systemResources: () => request<SystemResources>("/api/system/resources"),
  issueSseTicket: () => request<{ ticket: string }>("/api/events/ticket", { method: "POST" }),
  databaseConfig: () => request<DatabaseConfigResponse>("/api/settings/database"),
  saveDatabaseConfig: (database_url: string) => request<DatabaseConfigResponse>("/api/settings/database", { method: "PUT", body: JSON.stringify({ database_url }) }),

  monitoringHosts: () => request<MonitoringHostsResponse>("/api/monitoring/hosts"),
  monitoringHost: (id: string) => request<MonitoringHostDetailResponse>(monitoringHostPath(id)),
  monitoringHistory: (id: string) => request<MonitoringHistoryResponse>(`${monitoringHostPath(id)}/history`),

  sunshineHosts: () => request<SunshineHostInfo[]>("/api/services/sunshine/hosts"),
  sunshineCreateHost: (body: SunshineHostSaveRequest) => request<SunshineHostInfo>("/api/services/sunshine/hosts", { method: "POST", body: JSON.stringify(body) }),
  sunshineUpdateHost: (id: string, body: SunshineHostSaveRequest) => request<SunshineHostInfo>(sunshineHostPath(id), { method: "PUT", body: JSON.stringify(body) }),
  sunshineDeleteHost: (id: string) => request<void>(sunshineHostPath(id), { method: "DELETE" }),
  sunshineHostWake: (id: string) => request<unknown>(`${sunshineHostPath(id)}/wake`, { method: "POST" }),
  sunshineHostLogs: (id: string, lines = 300) => request<LogsResponse>(`${sunshineHostPath(id)}/logs?lines=${pathSegment(lines)}`),
  sunshineApps: (id: string) => request<SunshineAppsResponse>(`${sunshineHostPath(id)}/apps`),
  sunshineSaveApp: (id: string, app: Partial<SunshineApp>) => request<unknown>(`${sunshineHostPath(id)}/apps`, { method: "POST", body: JSON.stringify(app) }),
  sunshineCloseApp: (id: string) => request<unknown>(`${sunshineHostPath(id)}/apps/close`, { method: "POST" }),
  sunshineDeleteApp: (id: string, index: number) => request<unknown>(`${sunshineHostPath(id)}/apps/${pathSegment(index)}`, { method: "DELETE" }),
  sunshineClients: (id: string) => request<SunshineClientsResponse>(`${sunshineHostPath(id)}/clients`),
  sunshineUnpairClient: (id: string, uuid: string) => request<unknown>(`${sunshineHostPath(id)}/clients/unpair`, { method: "POST", body: JSON.stringify({ uuid }) }),
  sunshineUnpairAll: (id: string) => request<unknown>(`${sunshineHostPath(id)}/clients/unpair-all`, { method: "POST" }),
  sunshineUpdateClient: (id: string, uuid: string, enabled: boolean) => request<unknown>(`${sunshineHostPath(id)}/clients/update`, { method: "POST", body: JSON.stringify({ uuid, enabled }) }),
  sunshineConfig: (id: string) => request<SunshineConfig>(`${sunshineHostPath(id)}/config`),
  sunshineSaveConfig: (id: string, config: SunshineConfig) => request<unknown>(`${sunshineHostPath(id)}/config`, { method: "POST", body: JSON.stringify(config) }),
  sunshinePin: (id: string, pin: string, name: string) => request<unknown>(`${sunshineHostPath(id)}/pin`, { method: "POST", body: JSON.stringify({ pin, name }) }),
  sunshineRestart: (id: string) => request<unknown>(`${sunshineHostPath(id)}/restart`, { method: "POST" }),
  sunshineResetDisplay: (id: string) => request<unknown>(`${sunshineHostPath(id)}/reset-display`, { method: "POST" }),
};
