/*
 * 前端和后端共享的数据形状定义。
 *
 * TypeScript 的 interface 可以理解成“对象说明书”：
 * - 字段名必须和后端 JSON 返回的字段一致。
 * - 字段类型帮助编辑器检查错误，例如 number 不能当 string 用。
 * - 字段后面的 ? 表示可选字段，null 表示后端明确返回“没有值”。
 */

// /api/health 返回的管理后端健康状态。
export interface HealthResponse {
  // 后端运行状态，通常是 "ok"。
  status: string;
  // 当前程序版本。
  version: string;
  // 后端已经运行的秒数。
  uptime_seconds: number;
}

// 单个被管理服务的运行状态，例如 ram、sunshine、blog。
export interface ServiceStatus {
  // 服务内部名称，用于 API 和程序判断。
  name: string;
  // 服务类型，例如 process、external、job。
  kind: string;
  // 运行态文本，例如 running、stopped、unknown。
  runtime_state: string;
  // 健康检查是否通过。
  healthy: boolean;
  // 服务监听地址；没有地址时为 null。
  address: string | null;
  // 本机进程号；外部服务或未运行时可能为 null。
  pid: number | null;
  // 给用户看的状态说明。
  message: string;
  // 状态最后更新时间。
  updated_at: string;
}

// 启动、停止、重启等动作接口的通用返回。
export interface ActionResponse {
  ok: boolean;
  message: string;
  service: ServiceStatus | null;
}

// 日志读取接口返回最近若干行文本。
export interface LogsResponse {
  // 日志文件路径，方便排查实际读取的是哪个文件。
  path: string;
  // 按行切好的日志内容。
  lines: string[];
}

// 后端组装出来的 ram 启动命令，用于管理页展示和排错。
export interface RamCommandResponse {
  program: string;
  args: string[];
  command_line: string;
}

// ram 的完整运行配置。
export interface RamConfigResponse {
  // ram 对外提供文件访问的根目录。
  serve_path: string;
  // 监听地址，例如 127.0.0.1。
  bind: string;
  // 监听端口。
  port: number;
  // 反向代理路径前缀，例如 /files。
  path_prefix: string;
  // 管理端访问 ram 的本地 URL。
  local_url: string;
  // 健康检查 URL。
  health_url: string;
  // ram 自身访问日志路径。
  log_path: string;
  // 管理中心记录 ram 进程输出的日志路径。
  process_log_path: string;
  // 隐藏文件规则。
  hidden: string[];
  // ram 原始认证规则。
  auth_rules: string[];
  // basic 或 digest。
  auth_method: string;
  // 管理接口是否配置了认证。
  management_auth_configured: boolean;
  // 影响 ram 功能开关的详细字段。
  features: RamFeatures;
}

// ram 功能开关。字段名基本对应 ram 命令行参数。
export interface RamFeatures {
  allow_all: boolean;
  allow_upload: boolean;
  allow_delete: boolean;
  allow_search: boolean;
  allow_symlink: boolean;
  allow_archive: boolean;
  allow_hash: boolean;
  enable_cors: boolean;
  render_index: boolean;
  render_try_index: boolean;
  render_spa: boolean;
  compress: string;
  assets: string | null;
  tls_enabled: boolean;
}

// ram 健康检查返回。
export interface RamHealthResponse {
  reachable: boolean;
  status_code: number | null;
  url: string;
  // body 可能是任意 JSON，因此用 unknown，页面需要展示时再安全处理。
  body: unknown;
  message: string;
}

export interface RamInstanceInfo {
  id: string;
  name: string;
  host: string;
  port: number;
  use_tls: boolean;
  verify_tls: boolean;
  reachable: boolean;
  url: string;
  management_username: string | null;
  management_password_set: boolean;
}

export interface RamInstanceSaveRequest {
  name: string;
  host: string;
  port: number;
  use_tls: boolean;
  verify_tls: boolean;
}

// 读取 ram 某个目录或文件入口的返回。
export interface RamEntryResponse {
  url: string;
  path: string;
  status_code: number;
  body: unknown;
}

// 一个 ram 权限路径。permission 为 ro 表示只读，rw 表示可读写。
export interface RamAuthPath {
  path: string;
  permission: "ro" | "rw";
}

// 后端解析后的 ram 认证规则，用于页面展示。
export interface RamAuthRuleResponse {
  // username 为 null 且 anonymous 为 true 时，表示匿名访问规则。
  username: string | null;
  anonymous: boolean;
  // 出于安全考虑，后端不会把密码明文返回，只告诉前端是否设置过密码。
  password_set: boolean;
  paths: RamAuthPath[];
  // 原始规则文本，方便高级用户核对。
  raw: string;
}

// ram 权限管理页加载时的完整数据。
export interface RamAuthResponse {
  storage: string;
  auth_method: string;
  management_auth_configured: boolean;
  management_username: string | null;
  rules: RamAuthRuleResponse[];
}

// 前端保存 ram 认证规则时发给后端的数据。
export interface RamAuthRuleInput {
  username: string | null;
  // 密码可选：为空表示保留旧密码或匿名规则不需要密码。
  password?: string;
  paths: RamAuthPath[];
}

// 保存 ram 认证配置的请求体。
export interface RamAuthUpdateRequest {
  rules: RamAuthRuleInput[];
  management_username?: string | null;
  management_password?: string;
}

// 保存 ram 认证配置后的响应。
export interface RamAuthUpdateResponse {
  saved: boolean;
  applied: boolean;
  ram_reloaded: boolean;
  storage: string;
  management_auth_configured: boolean;
  management_username: string | null;
  rules: RamAuthRuleResponse[];
  message: string;
}

// Sunshine/Moonlight 串流服务状态。
export interface SunshineStatus {
  host: string;
  web_port: number;
  web_url: string;
  reachable: boolean;
  mac_configured: boolean;
  message: string;
}

// Wake-on-LAN 唤醒请求的返回。
export interface WakeResponse {
  ok: boolean;
  target: string;
  broadcast_addr: string;
}

// 博客文章列表项。列表页只需要摘要信息，不包含正文 content。
export interface BlogPost {
  id: string;
  title: string;
  description: string;
  relative_path: string;
  extension: string;
  draft: boolean;
  featured: boolean;
  pub_date: string | null;
  updated_date: string | null;
  author: string | null;
  category: string | null;
  series: string | null;
  hero_image: string | null;
  tags: string[];
  updated_at: string | null;
}

// 博客文章详情，比列表项多了正文 content。
export interface BlogPostDetail {
  post: BlogPost;
  content: string;
}

// 新建或保存博客文章时，前端提交给后端的数据。
export interface BlogPostSaveRequest {
  original_relative_path?: string | null;
  relative_path: string;
  title: string;
  description: string;
  pub_date: string;
  updated_date?: string | null;
  author?: string | null;
  category?: string | null;
  series?: string | null;
  hero_image?: string | null;
  tags: string[];
  draft: boolean;
  featured: boolean;
  content: string;
}

// 保存文章后的返回，包含后端重新读取/规范化后的文章信息。
export interface BlogPostWriteResponse {
  saved: boolean;
  post: BlogPost;
}

// 删除文章后的返回。
export interface BlogPostDeleteResponse {
  deleted: boolean;
  path: string;
}

// 标签或分类统计项。
export interface BlogTaxonomyItem {
  name: string;
  count: number;
}

// 单个分类下可选择的标签集合。
export interface BlogCategoryTags {
  category: string;
  tags: BlogTaxonomyItem[];
}

// 标签和分类的聚合统计。
export interface BlogTaxonomyResponse {
  tags: BlogTaxonomyItem[];
  categories: BlogTaxonomyItem[];
  category_tags: BlogCategoryTags[];
}

// 批量改名或删除标签/分类后的结果。
export interface BlogBulkEditResponse {
  changed: number;
}

// 博客静态站点构建任务结果。
export interface BlogBuildResponse {
  job_id: string;
  success: boolean;
  exit_code: number | null;
  duration_ms: number;
  log_path: string;
  log_tail: string[];
  /** 本次构建前从文件系统自动导入为草稿的孤立文章数量。 */
  adopted_as_drafts: number;
}

// 博客前台首页与站点展示配置。
export interface BlogHomeConfig {
  site_url: string;
  site_name: string;
  site_title: string;
  site_description: string;
  hero_title: string;
  hero_subtitle: string;
  background_image: string;
  announcement: string;
  avatar_image: string;
  footer_note: string;
}

// 发布/取消发布文章后的结果。
export interface PublishResponse {
  path: string;
  changed: boolean;
}

// 系统资源监控数据。
export interface SystemResources {
  cpu_usage_percent: number;
  memory_total_kib: number;
  memory_used_kib: number;
  network: NetworkThroughput;
  disk_throughput: DiskThroughput;
  disks: DiskInfo[];
}

// 网络吞吐量，单位为 bytes/s。
export interface NetworkThroughput {
  received_bytes_per_second: number;
  transmitted_bytes_per_second: number;
  total_bytes_per_second: number;
}

// 磁盘 IO 吞吐量，单位为 bytes/s。
export interface DiskThroughput {
  read_bytes_per_second: number;
  write_bytes_per_second: number;
  total_bytes_per_second: number;
}

// 单个磁盘/挂载点信息。
export interface DiskInfo {
  name: string;
  mount_point: string;
  total_bytes: number;
  available_bytes: number;
}

// SSE 推送的数据包。
export interface EventPayload {
  kind: string;
  generated_at: string;
  services: ServiceStatus[];
}

// ─── Sunshine 多主机管理 ──────────────────────────────────────────────────────

// 主机列表项（脱敏，不含密码明文）。
export interface SunshineHostInfo {
  id: string;
  name: string;
  host: string;
  web_port: number;
  mac_configured: boolean;
  broadcast_addr: string;
  username: string;
  password_set: boolean;
  verify_tls: boolean;
  web_url: string;
  reachable: boolean;
  connected: boolean;
  connection_error?: string | null;
}

// 新建或更新主机的请求体。
export interface SunshineHostSaveRequest {
  name: string;
  host: string;
  web_port: number;
  mac_address?: string | null;
  broadcast_addr?: string;
  username: string;
  password?: string | null;
  verify_tls: boolean;
}

// ─── Sunshine API 管理 ────────────────────────────────────────────────────────

// Sunshine 单个应用条目。
export interface SunshineApp {
  name: string;
  cmd?: string;
  index: number;
  image_path?: string | null;
  "image-path"?: string | null;
  working_dir?: string;
  "working-dir"?: string;
  output?: string;
  auto_detach?: boolean;
  "auto-detach"?: boolean;
  wait_all?: boolean;
  "wait-all"?: boolean;
  exit_timeout?: number;
  "exit-timeout"?: number;
  prep?: unknown[];
  "prep-cmd"?: unknown[];
  detached?: unknown[];
  elevated?: boolean;
  exclude_global_prep_cmd?: boolean;
  "exclude-global-prep-cmd"?: boolean;
  [key: string]: unknown;
}

// Sunshine 应用列表响应。
export interface SunshineAppsResponse {
  apps?: SunshineApp[];
  [key: string]: unknown;
}

// Sunshine 已配对客户端。
export interface SunshineClient {
  name?: string;
  uuid: string;
  enabled: boolean;
  cert?: string;
  [key: string]: unknown;
}

// Sunshine 客户端列表响应。
export interface SunshineClientsResponse {
  named_certs?: SunshineClient[];
  unnamed_certs?: SunshineClient[];
  named?: SunshineClient[];
  unnamed?: SunshineClient[];
  certs?: SunshineClient[];
  [key: string]: unknown;
}

// Sunshine 配置（任意 key-value 映射）。
export type SunshineConfig = Record<string, unknown>;

// Sunshine API 日志响应。
export interface SunshineApiLogsResponse {
  content?: string;
  [key: string]: unknown;
}

// ─── Proxmox VE ───────────────────────────────────────────────────────────────

export interface PveHostInfo {
  id: string;
  name: string;
  host: string;
  port: number;
  token_id: string;
  token_secret_set: boolean;
  verify_tls: boolean;
  web_url: string;
  connected: boolean;
  connection_error?: string | null;
}

export interface PveHostSaveRequest {
  name: string;
  host: string;
  port: number;
  token_id: string;
  token_secret?: string | null;
  verify_tls: boolean;
}

/** 一条 cluster/resources 资源条目（VM、CT、节点或存储） */
export interface PveResource {
  id: string;
  type: "qemu" | "lxc" | "node" | "storage" | "pool" | string;
  vmid?: number;
  name?: string;
  node?: string;
  status?: string;
  /** CPU 占用率（0–1） */
  cpu?: number;
  maxcpu?: number;
  /** 内存使用字节 */
  mem?: number;
  maxmem?: number;
  /** 磁盘使用字节 */
  disk?: number;
  maxdisk?: number;
  /** 在线时长（秒） */
  uptime?: number;
  storage?: string;
  pool?: string;
  template?: number;
  [key: string]: unknown;
}

export interface PveNodeInfo {
  node: string;
  status: "online" | "offline" | string;
  cpu?: number;
  maxcpu?: number;
  mem?: number;
  maxmem?: number;
  disk?: number;
  maxdisk?: number;
  uptime?: number;
  level?: string;
  [key: string]: unknown;
}

export interface PveTaskInfo {
  upid: string;
  node: string;
  type: string;
  user: string;
  status?: string;
  starttime?: number;
  endtime?: number;
  id?: string;
  [key: string]: unknown;
}

export interface PveStorageInfo {
  storage: string;
  type: string;
  node?: string;
  active?: number;
  enabled?: number;
  used?: number;
  avail?: number;
  total?: number;
  content?: string;
  shared?: number;
  [key: string]: unknown;
}

export interface PveContentItem {
  volid: string;
  content: string;
  format?: string;
  size?: number;
  vmid?: number;
  notes?: string;
  [key: string]: unknown;
}

export interface PveSnapshot {
  name: string;
  description?: string;
  snaptime?: number;
  vmstate?: number;
  parent?: string;
  [key: string]: unknown;
}

export interface PveSnapshotRequest {
  snapname: string;
  description?: string;
  vmstate?: boolean;
}

export interface PveMigrateRequest {
  target: string;
  online?: boolean;
  with_local_disks?: boolean;
}
export interface DatabaseConfigResponse {
  configured: boolean;
  database_url: string;
  connected: boolean;
  restart_required: boolean;
}
