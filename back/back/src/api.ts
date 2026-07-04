/*
 * back 的后端 API 封装。
 *
 * 初学者可以把这个文件理解成“前端访问后端的电话簿”：
 * - 每个函数对应一个后端接口，例如 api.health() 会请求 /api/health。
 * - 页面组件不直接写 fetch，而是调用这里的函数，这样超时、错误处理、JSON 解析都能统一。
 * - TypeScript 的泛型 <T> 用来告诉编辑器“这个接口返回什么形状的数据”，从而减少字段写错的概率。
 */
import type {
  ActionResponse,
  BlogBuildResponse,
  PveContentItem,
  PveHostInfo,
  PveHostSaveRequest,
  PveMigrateRequest,
  PveNodeInfo,
  PveResource,
  PveSnapshot,
  PveSnapshotRequest,
  PveStorageInfo,
  PveTaskInfo,
  BlogBulkEditResponse,
  BlogHomeConfig,
  BlogPostDeleteResponse,
  BlogPostDetail,
  BlogPostSaveRequest,
  BlogPostWriteResponse,
  BlogTaxonomyResponse,
  BlogPost,
  RamAuthResponse,
  RamAuthUpdateRequest,
  RamAuthUpdateResponse,
  RamCommandResponse,
  RamConfigResponse,
  RamEntryResponse,
  RamHealthResponse,
  RamInstanceInfo,
  RamInstanceSaveRequest,
  HealthResponse,
  LogsResponse,
  PublishResponse,
  ServiceStatus,
  SunshineApiLogsResponse,
  SunshineApp,
  SunshineAppsResponse,
  SunshineClientsResponse,
  SunshineConfig,
  SunshineHostInfo,
  SunshineHostSaveRequest,
  SunshineStatus,
  SystemResources,
  WakeResponse
  , DatabaseConfigResponse
} from "./types";

// 发送 JSON 请求体时使用的 HTTP 头。后端看到这个头后，会按 JSON 解析 body。
const jsonHeaders = {
  "Content-Type": "application/json"
};


// 所有请求共用的超时时间。15 秒内没有响应就主动取消，避免页面一直转圈。
const REQUEST_TIMEOUT_MS = 15_000;
const SESSION_TOKEN_KEY = "union.session-token";

function readSessionToken(): string | null {
  try {
    return window.sessionStorage.getItem(SESSION_TOKEN_KEY);
  } catch {
    return null;
  }
}

function writeSessionToken(token: string | null): void {
  try {
    if (token) {
      window.sessionStorage.setItem(SESSION_TOKEN_KEY, token);
    } else {
      window.sessionStorage.removeItem(SESSION_TOKEN_KEY);
    }
  } catch {
    // 禁用 Web Storage 时仍可继续使用服务端设置的 HttpOnly Cookie。
  }
}

type ApiRequestInit = RequestInit & {
  timeoutMs?: number;
  suppressAuthExpired?: boolean;
};

/**
 * 统一请求函数。
 *
 * @param path 后端接口路径，例如 "/api/health"。
 * @param init fetch 的配置，例如 method、body、headers。
 * @returns 解析后的 JSON 数据，类型由调用方传入的泛型 T 决定。
 */
async function request<T>(path: string, init?: ApiRequestInit): Promise<T> {
  const { timeoutMs = REQUEST_TIMEOUT_MS, suppressAuthExpired = false, ...fetchInit } = init ?? {};
  // AbortController 是浏览器提供的“取消请求”工具。
  // 这里既用它实现本地超时，也允许调用方通过 init.signal 主动取消请求。
  const controller = new AbortController();
  let didTimeout = false;
  const timeoutId =
    timeoutMs > 0
      ? window.setTimeout(() => {
          didTimeout = true;
          controller.abort();
        }, timeoutMs)
      : undefined;
  const callerSignal = fetchInit.signal;
  const abortFromCaller = () => controller.abort(callerSignal?.reason);

  // 如果调用方已经取消了请求，立刻同步取消；否则监听它后续的取消事件。
  if (callerSignal?.aborted) {
    abortFromCaller();
  } else {
    callerSignal?.addEventListener("abort", abortFromCaller, { once: true });
  }

  let response: Response;

  try {
    // fetch 是浏览器原生 HTTP 请求函数。
    // 有请求体时自动补 Content-Type: application/json，让后端知道 body 是 JSON 字符串。
    const shouldSendJsonHeader =
      Boolean(fetchInit.body) && !(fetchInit.body instanceof FormData);
    const sessionToken = readSessionToken();
    response = await fetch(path, {
      ...fetchInit,
      credentials: "include",
      signal: controller.signal,
      headers: {
        ...(shouldSendJsonHeader ? jsonHeaders : undefined),
        ...(!fetchInit.method || ["GET", "HEAD", "OPTIONS"].includes(fetchInit.method.toUpperCase())
          ? undefined
          : { "X-CSRF-Token": "1" }),
        ...(sessionToken ? { Authorization: `Bearer ${sessionToken}` } : undefined),
        ...fetchInit.headers
      }
    });
  } catch (error) {
    // fetch 被取消时会抛 AbortError。这里把浏览器原始错误翻译成用户能看懂的中文。
    if (error instanceof DOMException && error.name === "AbortError") {
      throw new Error(
        didTimeout ? "请求超时，请检查管理后端是否可用" : "请求已取消"
      );
    }
    throw error;
  } finally {
    // 无论成功还是失败，都要清理定时器和事件监听，避免内存里残留无用回调。
    if (timeoutId !== undefined) {
      window.clearTimeout(timeoutId);
    }
    callerSignal?.removeEventListener("abort", abortFromCaller);
  }

  // HTTP 状态码不是 2xx 时，fetch 本身不会抛错，需要我们手动判断。
  if (response.status === 401) {
    writeSessionToken(null);
    if (!suppressAuthExpired) {
      window.dispatchEvent(new Event("union:auth-expired"));
    }
    throw new ApiError("认证已失效，请重新登录", "unauthorized", response.status);
  }
  if (!response.ok) {
    throw await readApiError(response);
  }

  // 204 No Content 表示“成功，但没有响应体”。此时不能调用 response.json()。
  if (response.status === 204) {
    return undefined as T;
  }

  // 后端接口统一返回 JSON，这里把响应体解析成调用方期望的类型。
  return (await response.json()) as T;
}

/**
 * 从错误响应里尽量提取可读消息。
 *
 * 后端通常会返回 { message: "..." }，但代理或框架也可能返回 { error } 或 { detail }。
 * 如果响应不是 JSON，就直接展示原始文本或 HTTP 状态。
 */
export class ApiError extends Error {
  constructor(
    message: string,
    readonly code: string | undefined,
    readonly status: number,
  ) {
    super(message);
    this.name = "ApiError";
  }
}

async function readApiError(response: Response): Promise<ApiError> {
  const fallback = `${response.status} ${response.statusText}`;
  const text = await response.text().catch(() => "");
  if (!text) {
    return new ApiError(fallback, undefined, response.status);
  }

  try {
    const payload = JSON.parse(text) as Record<string, unknown>;
    const code = typeof payload.code === "string" ? payload.code : undefined;
    // 优先使用后端约定的 message，同时兼容常见的 error/detail 字段。
    for (const key of ["message", "error", "detail"]) {
      const value = payload[key];
      if (typeof value === "string" && value.trim()) {
        return new ApiError(value, code, response.status);
      }
    }
  } catch {
    return new ApiError(text, undefined, response.status);
  }

  return new ApiError(text, undefined, response.status);
}

// api 对象把所有后端接口集中导出。
// 页面组件只需要关心“我要做什么”，不用关心 URL、HTTP method 和 JSON 细节。
export const api = {
  authenticate: async () => {
    return request<{ username: string }>("/api/auth/me", {
      suppressAuthExpired: true,
    });
  },
  login: async (username: string, password: string) => {
    const result = await request<{ username: string; token: string }>("/api/auth/login", {
      method: "POST",
      body: JSON.stringify({ username, password })
    });
    writeSessionToken(result.token);
    return result;
  },
  // 管理后端自身状态。
  health: () => request<HealthResponse>("/api/health"),

  // 服务总览和 ram 文件服务控制。
  services: () => request<ServiceStatus[]>("/api/services"),
  startRam: () =>
    request<ActionResponse>("/api/services/ram/start", { method: "POST" }),
  stopRam: () =>
    request<ActionResponse>("/api/services/ram/stop", { method: "POST" }),
  restartRam: () =>
    request<ActionResponse>("/api/services/ram/restart", { method: "POST" }),
  ramConfig: () => request<RamConfigResponse>("/api/services/ram/config"),
  ramCommand: () =>
    request<RamCommandResponse>("/api/services/ram/command"),
  ramAuth: () => request<RamAuthResponse>("/api/services/ram/auth"),
  updateRamAuth: (payload: RamAuthUpdateRequest) =>
    request<RamAuthUpdateResponse>("/api/services/ram/auth", {
      method: "POST",
      body: JSON.stringify(payload)
    }),
  ramHealth: () => request<RamHealthResponse>("/api/services/ram/health"),
  ramEntry: (path = "/") =>
    request<RamEntryResponse>(
      `/api/services/ram/entry?path=${encodeURIComponent(path)}`
    ),
  ramLogs: (lines = 300) =>
    request<LogsResponse>(`/api/services/ram/logs?lines=${lines}`),
  ramInstances: () => request<RamInstanceInfo[]>("/api/services/ram/instances"),
  createRamInstance: (payload: RamInstanceSaveRequest) => request<RamInstanceInfo>("/api/services/ram/instances", { method: "POST", body: JSON.stringify(payload) }),
  updateRamInstance: (id: string, payload: RamInstanceSaveRequest) => request<RamInstanceInfo>(`/api/services/ram/instances/${id}`, { method: "PUT", body: JSON.stringify(payload) }),
  deleteRamInstance: (id: string) => request<void>(`/api/services/ram/instances/${id}`, { method: "DELETE" }),
  ramInstanceAuth: (id: string) => request<RamAuthResponse>(`/api/services/ram/instances/${id}/auth`),
  updateRamInstanceAuth: (id: string, payload: RamAuthUpdateRequest) => request<RamAuthUpdateResponse>(`/api/services/ram/instances/${id}/auth`, { method: "POST", body: JSON.stringify(payload) }),
  blogLogs: (lines = 300) =>
    request<LogsResponse>(`/api/blog/logs?lines=${lines}`),

  // Sunshine 多主机管理 — CRUD。
  sunshineHosts: () =>
    request<SunshineHostInfo[]>("/api/services/sunshine/hosts"),
  sunshineCreateHost: (req: SunshineHostSaveRequest) =>
    request<SunshineHostInfo>("/api/services/sunshine/hosts", {
      method: "POST",
      body: JSON.stringify(req)
    }),
  sunshineUpdateHost: (id: string, req: SunshineHostSaveRequest) =>
    request<SunshineHostInfo>(`/api/services/sunshine/hosts/${id}`, {
      method: "PUT",
      body: JSON.stringify(req)
    }),
  sunshineDeleteHost: (id: string) =>
    request<void>(`/api/services/sunshine/hosts/${id}`, { method: "DELETE" }),

  // Sunshine 多主机管理 — 单主机状态、WOL、日志。
  sunshineHostStatus: (id: string) =>
    request<SunshineStatus>(`/api/services/sunshine/hosts/${id}/status`),
  sunshineHostWake: (id: string) =>
    request<WakeResponse>(`/api/services/sunshine/hosts/${id}/wake`, { method: "POST" }),
  sunshineHostLogs: (id: string, lines = 300) =>
    request<LogsResponse>(`/api/services/sunshine/hosts/${id}/logs?lines=${lines}`),

  // Sunshine 多主机管理 — API 代理（应用）。
  sunshineApps: (id: string) =>
    request<SunshineAppsResponse>(`/api/services/sunshine/hosts/${id}/apps`),
  sunshineSaveApp: (id: string, app: Partial<SunshineApp>) =>
    request<unknown>(`/api/services/sunshine/hosts/${id}/apps`, {
      method: "POST", body: JSON.stringify(app)
    }),
  sunshineCloseApp: (id: string) =>
    request<unknown>(`/api/services/sunshine/hosts/${id}/apps/close`, { method: "POST" }),
  sunshineDeleteApp: (id: string, index: number) =>
    request<unknown>(`/api/services/sunshine/hosts/${id}/apps/${index}`, { method: "DELETE" }),

  // Sunshine 多主机管理 — API 代理（客户端）。
  sunshineClients: (id: string) =>
    request<SunshineClientsResponse>(`/api/services/sunshine/hosts/${id}/clients`),
  sunshineUnpairClient: (id: string, uuid: string) =>
    request<unknown>(`/api/services/sunshine/hosts/${id}/clients/unpair`, {
      method: "POST", body: JSON.stringify({ uuid })
    }),
  sunshineUnpairAll: (id: string) =>
    request<unknown>(`/api/services/sunshine/hosts/${id}/clients/unpair-all`, { method: "POST" }),
  sunshineUpdateClient: (id: string, uuid: string, enabled: boolean) =>
    request<unknown>(`/api/services/sunshine/hosts/${id}/clients/update`, {
      method: "POST", body: JSON.stringify({ uuid, enabled })
    }),

  // Sunshine 多主机管理 — API 代理（配置）。
  sunshineConfig: (id: string) =>
    request<SunshineConfig>(`/api/services/sunshine/hosts/${id}/config`),
  sunshineSaveConfig: (id: string, config: SunshineConfig) =>
    request<unknown>(`/api/services/sunshine/hosts/${id}/config`, {
      method: "POST", body: JSON.stringify(config)
    }),

  // Sunshine 多主机管理 — API 代理（系统）。
  sunshineApiLogs: (id: string) =>
    request<SunshineApiLogsResponse>(`/api/services/sunshine/hosts/${id}/api-logs`),
  sunshinePin: (id: string, pin: string, name: string) =>
    request<unknown>(`/api/services/sunshine/hosts/${id}/pin`, {
      method: "POST", body: JSON.stringify({ pin, name })
    }),
  sunshineChangePassword: (id: string, payload: Record<string, string>) =>
    request<unknown>(`/api/services/sunshine/hosts/${id}/password`, {
      method: "POST", body: JSON.stringify(payload)
    }),
  sunshineRestart: (id: string) =>
    request<unknown>(`/api/services/sunshine/hosts/${id}/restart`, { method: "POST" }),
  sunshineResetDisplay: (id: string) =>
    request<unknown>(`/api/services/sunshine/hosts/${id}/reset-display`, { method: "POST" }),
  // 博客内容管理。这里的 path 都会 encodeURIComponent，避免中文、空格、斜杠等字符破坏 URL。
  blogPosts: () => request<BlogPost[]>("/api/blog/posts"),
  blogPostDetail: (path: string) =>
    request<BlogPostDetail>(
      `/api/blog/posts/detail?path=${encodeURIComponent(path)}`
    ),
  saveBlogPost: (payload: BlogPostSaveRequest) =>
    request<BlogPostWriteResponse>("/api/blog/posts/save", {
      method: "POST",
      body: JSON.stringify(payload)
    }),
  deleteBlogPost: (path: string) =>
    request<BlogPostDeleteResponse>(
      `/api/blog/posts?path=${encodeURIComponent(path)}`,
      { method: "DELETE" }
    ),
  blogHome: () => request<BlogHomeConfig>("/api/blog/home"),
  saveBlogHome: (payload: BlogHomeConfig) =>
    request<BlogHomeConfig>("/api/blog/home", {
      method: "POST",
      body: JSON.stringify(payload)
    }),
  blogTaxonomy: () => request<BlogTaxonomyResponse>("/api/blog/taxonomy"),
  buildBlog: () =>
    request<BlogBuildResponse>("/api/blog/build", { method: "POST" }),
  publishPost: (path: string) =>
    request<PublishResponse>("/api/blog/publish", {
      method: "POST",
      body: JSON.stringify({ path })
    }),
  unpublishPost: (path: string) =>
    request<PublishResponse>("/api/blog/unpublish", {
      method: "POST",
      body: JSON.stringify({ path })
    }),
  addBlogTag: (name: string, category?: string | null) =>
    request<BlogBulkEditResponse>("/api/blog/tags/add", {
      method: "POST",
      body: JSON.stringify({ name, category: category || null })
    }),
  renameBlogTag: (from: string, to: string, category?: string | null) =>
    request<BlogBulkEditResponse>("/api/blog/tags/rename", {
      method: "POST",
      body: JSON.stringify({ from, to, category: category || null })
    }),
  deleteBlogTag: (tag: string, category?: string | null) =>
    request<BlogBulkEditResponse>("/api/blog/tags/delete", {
      method: "POST",
      body: JSON.stringify({ tag, category: category || null })
    }),
  addBlogCategory: (name: string) =>
    request<BlogBulkEditResponse>("/api/blog/categories/add", {
      method: "POST",
      body: JSON.stringify({ name })
    }),
  renameBlogCategory: (from: string, to: string) =>
    request<BlogBulkEditResponse>("/api/blog/categories/rename", {
      method: "POST",
      body: JSON.stringify({ from, to })
    }),
  deleteBlogCategory: (category: string) =>
    request<BlogBulkEditResponse>("/api/blog/categories/delete", {
      method: "POST",
      body: JSON.stringify({ category })
    }),

  // 系统资源监控，例如 CPU、内存和磁盘空间。
  systemResources: () => request<SystemResources>("/api/system/resources"),

  // 签发一个 60 秒有效的 SSE ticket，避免长效 session token 出现在服务器日志里。
  issueSseTicket: () =>
    request<{ ticket: string }>("/api/events/ticket", { method: "POST" }),

  // Proxmox VE 多主机管理。
  pveHosts: () => request<PveHostInfo[]>("/api/pve/hosts"),
  pveCreateHost: (req: PveHostSaveRequest) =>
    request<PveHostInfo>("/api/pve/hosts", { method: "POST", body: JSON.stringify(req) }),
  pveUpdateHost: (id: string, req: PveHostSaveRequest) =>
    request<PveHostInfo>(`/api/pve/hosts/${id}`, { method: "PUT", body: JSON.stringify(req) }),
  pveDeleteHost: (id: string) =>
    request<void>(`/api/pve/hosts/${id}`, { method: "DELETE" }),

  pveResources: (id: string) =>
    request<PveResource[]>(`/api/pve/hosts/${id}/resources`),
  pveNodes: (id: string) =>
    request<PveNodeInfo[]>(`/api/pve/hosts/${id}/nodes`),
  pveTasks: (id: string) =>
    request<PveTaskInfo[]>(`/api/pve/hosts/${id}/tasks`),

  pveNodeStatus: (id: string, node: string) =>
    request<unknown>(`/api/pve/hosts/${id}/nodes/${node}/status`),
  pveNodeStorage: (id: string, node: string) =>
    request<PveStorageInfo[]>(`/api/pve/hosts/${id}/nodes/${node}/storage`),
  pveStorageContent: (id: string, node: string, storage: string) =>
    request<PveContentItem[]>(`/api/pve/hosts/${id}/nodes/${node}/storage/${storage}/content`),
  pveNodeTasks: (id: string, node: string) =>
    request<PveTaskInfo[]>(`/api/pve/hosts/${id}/nodes/${node}/tasks`),

  // VM (QEMU) 操作。
  pveVms: (id: string, node: string) =>
    request<PveResource[]>(`/api/pve/hosts/${id}/nodes/${node}/vms`),
  pveVmStatus: (id: string, node: string, vmid: number) =>
    request<unknown>(`/api/pve/hosts/${id}/nodes/${node}/vms/${vmid}/status`),
  pveVmConfig: (id: string, node: string, vmid: number) =>
    request<Record<string, unknown>>(`/api/pve/hosts/${id}/nodes/${node}/vms/${vmid}/config`),
  pveVmStart: (id: string, node: string, vmid: number) =>
    request<unknown>(`/api/pve/hosts/${id}/nodes/${node}/vms/${vmid}/start`, { method: "POST" }),
  pveVmStop: (id: string, node: string, vmid: number) =>
    request<unknown>(`/api/pve/hosts/${id}/nodes/${node}/vms/${vmid}/stop`, { method: "POST" }),
  pveVmShutdown: (id: string, node: string, vmid: number) =>
    request<unknown>(`/api/pve/hosts/${id}/nodes/${node}/vms/${vmid}/shutdown`, { method: "POST" }),
  pveVmReboot: (id: string, node: string, vmid: number) =>
    request<unknown>(`/api/pve/hosts/${id}/nodes/${node}/vms/${vmid}/reboot`, { method: "POST" }),
  pveVmSuspend: (id: string, node: string, vmid: number) =>
    request<unknown>(`/api/pve/hosts/${id}/nodes/${node}/vms/${vmid}/suspend`, { method: "POST" }),
  pveVmResume: (id: string, node: string, vmid: number) =>
    request<unknown>(`/api/pve/hosts/${id}/nodes/${node}/vms/${vmid}/resume`, { method: "POST" }),
  pveVmReset: (id: string, node: string, vmid: number) =>
    request<unknown>(`/api/pve/hosts/${id}/nodes/${node}/vms/${vmid}/reset`, { method: "POST" }),
  pveVmDelete: (id: string, node: string, vmid: number, purge = false) =>
    request<unknown>(`/api/pve/hosts/${id}/nodes/${node}/vms/${vmid}?purge=${purge ? "true" : "false"}`, { method: "DELETE" }),
  pveVmMigrate: (id: string, node: string, vmid: number, req: PveMigrateRequest) =>
    request<unknown>(`/api/pve/hosts/${id}/nodes/${node}/vms/${vmid}/migrate`, {
      method: "POST", body: JSON.stringify(req)
    }),
  pveVmSnapshots: (id: string, node: string, vmid: number) =>
    request<PveSnapshot[]>(`/api/pve/hosts/${id}/nodes/${node}/vms/${vmid}/snapshots`),
  pveVmSnapshotCreate: (id: string, node: string, vmid: number, req: PveSnapshotRequest) =>
    request<unknown>(`/api/pve/hosts/${id}/nodes/${node}/vms/${vmid}/snapshots`, {
      method: "POST", body: JSON.stringify(req)
    }),
  pveVmSnapshotDelete: (id: string, node: string, vmid: number, snap: string) =>
    request<unknown>(`/api/pve/hosts/${id}/nodes/${node}/vms/${vmid}/snapshots/${snap}`, { method: "DELETE" }),
  pveVmSnapshotRollback: (id: string, node: string, vmid: number, snap: string) =>
    request<unknown>(`/api/pve/hosts/${id}/nodes/${node}/vms/${vmid}/snapshots/${snap}/rollback`, { method: "POST" }),

  // Container (LXC) 操作。
  pveContainers: (id: string, node: string) =>
    request<PveResource[]>(`/api/pve/hosts/${id}/nodes/${node}/containers`),
  pveCtStatus: (id: string, node: string, vmid: number) =>
    request<unknown>(`/api/pve/hosts/${id}/nodes/${node}/containers/${vmid}/status`),
  pveCtConfig: (id: string, node: string, vmid: number) =>
    request<Record<string, unknown>>(`/api/pve/hosts/${id}/nodes/${node}/containers/${vmid}/config`),
  pveCtStart: (id: string, node: string, vmid: number) =>
    request<unknown>(`/api/pve/hosts/${id}/nodes/${node}/containers/${vmid}/start`, { method: "POST" }),
  pveCtStop: (id: string, node: string, vmid: number) =>
    request<unknown>(`/api/pve/hosts/${id}/nodes/${node}/containers/${vmid}/stop`, { method: "POST" }),
  pveCtShutdown: (id: string, node: string, vmid: number) =>
    request<unknown>(`/api/pve/hosts/${id}/nodes/${node}/containers/${vmid}/shutdown`, { method: "POST" }),
  pveCtReboot: (id: string, node: string, vmid: number) =>
    request<unknown>(`/api/pve/hosts/${id}/nodes/${node}/containers/${vmid}/reboot`, { method: "POST" }),
  pveCtDelete: (id: string, node: string, vmid: number) =>
    request<unknown>(`/api/pve/hosts/${id}/nodes/${node}/containers/${vmid}`, { method: "DELETE" }),
  pveCtSnapshots: (id: string, node: string, vmid: number) =>
    request<PveSnapshot[]>(`/api/pve/hosts/${id}/nodes/${node}/containers/${vmid}/snapshots`),
  pveCtSnapshotCreate: (id: string, node: string, vmid: number, req: PveSnapshotRequest) =>
    request<unknown>(`/api/pve/hosts/${id}/nodes/${node}/containers/${vmid}/snapshots`, {
      method: "POST", body: JSON.stringify(req)
    }),
  pveCtSnapshotDelete: (id: string, node: string, vmid: number, snap: string) =>
    request<unknown>(`/api/pve/hosts/${id}/nodes/${node}/containers/${vmid}/snapshots/${snap}`, { method: "DELETE" }),
  pveCtSnapshotRollback: (id: string, node: string, vmid: number, snap: string) =>
    request<unknown>(`/api/pve/hosts/${id}/nodes/${node}/containers/${vmid}/snapshots/${snap}/rollback`, { method: "POST" }),

  // 认证与账号管理。
  logout: async () => {
    try {
      await request<void>("/api/auth/logout", { method: "POST" });
    } finally {
      writeSessionToken(null);
    }
  },
  me: () => {
    if (!readSessionToken()) {
      return Promise.reject(new Error("尚未登录"));
    }
    return request<{ username: string }>("/api/auth/me");
  },
  changePassword: (current_password: string, new_password: string) =>
    request<void>("/api/auth/change-password", {
      method: "POST",
      body: JSON.stringify({ current_password, new_password })
    }),
  databaseConfig: () => request<DatabaseConfigResponse>("/api/settings/database"),
  saveDatabaseConfig: (database_url: string) =>
    request<DatabaseConfigResponse>("/api/settings/database", {
      method: "PUT",
      body: JSON.stringify({ database_url })
    })
};
