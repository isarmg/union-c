export const queryKeys = {
  health: ["health"] as const,
  services: ["services"] as const,
  systemResources: ["system-resources"] as const,
  auth: { me: ["auth-me"] as const },
  settings: { database: ["settings-database"] as const },
  monitoring: {
    hosts: ["monitoring-hosts"] as const,
    host: (hostId: string) => ["monitoring-host", hostId] as const,
    history: (hostId: string) => ["monitoring-history", hostId] as const,
  },
  logs: { sunshine: (hostId: string) => ["logs", "sunshine", hostId] as const },
  sunshine: {
    hosts: ["sunshine-hosts"] as const,
    apps: (hostId: string) => ["sunshine-apps", hostId] as const,
    clients: (hostId: string) => ["sunshine-clients", hostId] as const,
    config: (hostId: string) => ["sunshine-config", hostId] as const,
  },
} as const;
