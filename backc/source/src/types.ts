export interface HealthResponse {
  status: string;
  version: string;
  uptime_seconds: number;
}

export interface ServiceStatus {
  name: string;
  kind: string;
  runtime_state: string;
  healthy: boolean;
  address: string | null;
  pid: number | null;
  message: string;
  updated_at: string;
}

export interface LogsResponse { path: string; lines: string[]; }

export interface SystemResources {
  cpu_usage_percent: number;
  memory_total_kib: number;
  memory_used_kib: number;
  network: NetworkThroughput;
  disk_throughput: DiskThroughput;
  disks: DiskInfo[];
}
export interface NetworkThroughput { received_bytes_per_second: number; transmitted_bytes_per_second: number; total_bytes_per_second: number; }
export interface DiskThroughput { read_bytes_per_second: number; write_bytes_per_second: number; total_bytes_per_second: number; }
export interface DiskInfo { name: string; mount_point: string; total_bytes: number; available_bytes: number; }
export interface EventPayload { kind: string; generated_at: string; services: ServiceStatus[]; }

export interface SunshineHostInfo {
  id: string; name: string; host: string; web_port: number;
  mac_configured: boolean; broadcast_addr: string; username: string;
  password_set: boolean; verify_tls: boolean; web_url: string;
  reachable: boolean; connected: boolean; connection_error?: string | null;
}
export interface SunshineHostSaveRequest {
  name: string; host: string; web_port: number; mac_address?: string | null;
  broadcast_addr?: string; username: string; password?: string | null; verify_tls: boolean;
}
export interface SunshineApp {
  name: string; cmd?: string; index: number; image_path?: string | null;
  "image-path"?: string | null; working_dir?: string; "working-dir"?: string;
  output?: string; auto_detach?: boolean; "auto-detach"?: boolean;
  wait_all?: boolean; "wait-all"?: boolean; exit_timeout?: number;
  "exit-timeout"?: number; prep?: unknown[]; "prep-cmd"?: unknown[];
  detached?: unknown[]; elevated?: boolean; exclude_global_prep_cmd?: boolean;
  "exclude-global-prep-cmd"?: boolean; [key: string]: unknown;
}
export interface SunshineAppsResponse { apps?: SunshineApp[]; [key: string]: unknown; }
export interface SunshineClient { name?: string; uuid: string; enabled: boolean; cert?: string; [key: string]: unknown; }
export interface SunshineClientsResponse {
  named_certs?: SunshineClient[]; unnamed_certs?: SunshineClient[];
  named?: SunshineClient[]; unnamed?: SunshineClient[]; certs?: SunshineClient[];
  [key: string]: unknown;
}
export type SunshineConfig = Record<string, unknown>;

export interface DatabaseConfigResponse {
  configured: boolean; database_url: string; connected: boolean; restart_required: boolean;
}

// Read-only telemetry API. Nullable values mean the agent could not collect the
// metric; the UI deliberately distinguishes those values from a real zero.
export interface MonitoringCapability {
  name: string;
  available: boolean;
  source: string;
  error_kind: string | null;
  message: string | null;
}

export interface MonitoringHostSummary {
  id: string;
  name: string;
  os: string;
  os_version: string | null;
  kernel_version: string | null;
  arch: string;
  agent_version: string;
  registered_at: string;
  last_seen_at: string;
  latest_collected_at: string | null;
  status: "online" | "stale" | "offline";
  capabilities: MonitoringCapability[];
  cpu_usage_percent: number | null;
  memory_usage_percent: number | null;
  network_received_bytes_per_second: number | null;
  network_transmitted_bytes_per_second: number | null;
  disk_read_bytes_per_second: number | null;
  disk_written_bytes_per_second: number | null;
  max_temperature_celsius: number | null;
  gpu_utilization_percent: number | null;
  gpu_memory_usage_percent: number | null;
}

export interface MonitoringHostsResponse { hosts: MonitoringHostSummary[]; }

export interface MonitoringCpuReport {
  usage_percent: number;
  logical_count: number;
  physical_count: number | null;
  per_core_percent: number[];
}

export interface MonitoringMemoryReport {
  total_bytes: number;
  used_bytes: number;
  available_bytes: number;
  swap_total_bytes: number;
  swap_used_bytes: number;
}

export interface MonitoringNetworkReport {
  name: string;
  received_bytes_total: number;
  transmitted_bytes_total: number;
  received_bytes_per_second: number;
  transmitted_bytes_per_second: number;
  packets_received_total: number;
  packets_transmitted_total: number;
  receive_errors_total: number;
  transmit_errors_total: number;
}

export interface MonitoringDiskReport {
  name: string;
  mount_point: string;
  file_system: string;
  total_bytes: number;
  available_bytes: number;
  read_bytes_total: number;
  written_bytes_total: number;
  read_bytes_per_second: number;
  written_bytes_per_second: number;
  is_read_only: boolean;
}

export interface MonitoringTemperatureReport {
  id: string;
  label: string;
  celsius: number | null;
  max_celsius: number | null;
  critical_celsius: number | null;
  source: string;
}

export interface MonitoringGpuReport {
  id: string;
  vendor: string;
  name: string;
  utilization_percent: number | null;
  memory_total_bytes: number | null;
  memory_used_bytes: number | null;
  temperature_celsius: number | null;
  power_watts: number | null;
  core_clock_mhz: number | null;
  memory_clock_mhz: number | null;
  pcie_rx_bytes_per_second: number | null;
  pcie_tx_bytes_per_second: number | null;
  source: string;
}

export interface MonitoringAgentReport {
  schema_version: number;
  report_id: string;
  collected_at: string;
  host: {
    id: string; name: string; os: string; os_version: string | null;
    kernel_version: string | null; arch: string; agent_version: string;
  };
  interval_seconds: number;
  system: {
    uptime_seconds: number;
    cpu: MonitoringCpuReport;
    memory: MonitoringMemoryReport;
    networks: MonitoringNetworkReport[];
    disks: MonitoringDiskReport[];
    temperatures: MonitoringTemperatureReport[];
    gpus: MonitoringGpuReport[];
  };
  capabilities: MonitoringCapability[];
  agent: { spool_pending_batches: number; collector_errors: number };
}

export interface MonitoringHostDetailResponse {
  host: MonitoringHostSummary;
  latest: MonitoringAgentReport | null;
}

export interface MonitoringHistoryPoint {
  report_id: string;
  collected_at: string;
  received_at: string;
  cpu_usage_percent: number | null;
  memory_usage_percent: number | null;
  network_received_bytes_per_second: number | null;
  network_transmitted_bytes_per_second: number | null;
  disk_read_bytes_per_second: number | null;
  disk_written_bytes_per_second: number | null;
  max_temperature_celsius: number | null;
  gpu_utilization_percent: number | null;
  gpu_memory_usage_percent: number | null;
}

export interface MonitoringHistoryResponse {
  host_id: string;
  points: MonitoringHistoryPoint[];
}
