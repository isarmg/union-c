/**
 * React Query 缓存键的唯一来源。
 *
 * 查询和 mutation 刷新必须使用同一组键。集中定义后，新增参数或调整命名时
 * 不需要在多个视图中同步修改字符串字面量。
 */
export const queryKeys = {
  health: ["health"] as const,
  services: ["services"] as const,
  systemResources: ["system-resources"] as const,
  auth: {
    me: ["auth-me"] as const,
  },
  settings: {
    database: ["settings-database"] as const,
  },
  ram: {
    instances: ["ram-instances"] as const,
    instanceAuth: (id: string) => ["ram-instance-auth", id] as const,
    auth: ["ram-auth"] as const,
    config: ["ram-config"] as const,
    command: ["ram-command"] as const,
  },
  logs: {
    ram: ["logs", "ram"] as const,
    sunshine: (hostId: string) => ["logs", "sunshine", hostId] as const,
    blog: ["logs", "blog"] as const,
  },
  blog: {
    home: ["blog-home"] as const,
    posts: ["blog-posts"] as const,
    taxonomy: ["blog-taxonomy"] as const,
    details: ["blog-post-detail"] as const,
    detail: (path: string | null) => ["blog-post-detail", path] as const,
  },
  sunshine: {
    hosts: ["sunshine-hosts"] as const,
    apps: (hostId: string) => ["sunshine-apps", hostId] as const,
    clients: (hostId: string) => ["sunshine-clients", hostId] as const,
    config: (hostId: string) => ["sunshine-config", hostId] as const,
  },
  pve: {
    hosts: ["pve-hosts"] as const,
    resources: (hostId: string) => ["pve-resources", hostId] as const,
    nodes: (hostId: string) => ["pve-nodes", hostId] as const,
    storage: (hostId: string, node: string | null) =>
      ["pve-storage", hostId, node] as const,
    content: (hostId: string, node: string | null, storage: string | null) =>
      ["pve-content", hostId, node, storage] as const,
    tasks: (hostId: string) => ["pve-tasks", hostId] as const,
    snapshots: (
      hostId: string,
      node: string,
      vmid: number,
      type: "qemu" | "lxc",
    ) => ["pve-snapshots", hostId, node, vmid, type] as const,
    config: (
      hostId: string,
      node: string,
      vmid: number,
      type: "qemu" | "lxc",
    ) => ["pve-config", hostId, node, vmid, type] as const,
  },
} as const;
