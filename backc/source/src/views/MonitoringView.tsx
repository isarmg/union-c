import { useEffect, useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import {
  Activity,
  CircuitBoard,
  Gauge,
  HardDrive,
  MonitorDot,
  Network,
  ShieldCheck,
  Thermometer,
} from "lucide-react";
import { api } from "../api";
import { queryKeys } from "../query-keys";
import type {
  MonitoringAgentReport,
  MonitoringCapability,
  MonitoringGpuReport,
  MonitoringHistoryPoint,
  MonitoringHostSummary,
} from "../types";
import { formatBytes, formatBytesPerSecond, formatDateTime, percent } from "../utils";
import {
  CardInner,
  CardRow,
  InlineNotice,
  LoadingBlock,
  Metric,
  ProgressBar,
  SectionHeader,
  StatusLed,
  TickerText,
  TruncatedText,
} from "../components/ui";

const NA = "N/A";

function isNumber(value: number | null | undefined): value is number {
  return typeof value === "number" && Number.isFinite(value);
}

function formatMetric(value: number | null | undefined, formatter: (value: number) => string): string {
  return isNumber(value) ? formatter(value) : NA;
}

function formatPercent(value: number | null | undefined): string {
  return formatMetric(value, (metric) => `${metric.toFixed(1)}%`);
}

function formatTemperature(value: number | null | undefined): string {
  return formatMetric(value, (metric) => `${metric.toFixed(1)} °C`);
}

function sumNullable(...values: Array<number | null | undefined>): number | null {
  const available = values.filter(isNumber);
  return available.length ? available.reduce((total, value) => total + value, 0) : null;
}

function metricTone(value: number | null | undefined, threshold = 85): "good" | "warn" | "neutral" {
  if (!isNumber(value)) return "neutral";
  return value >= threshold ? "warn" : "good";
}

function statusMeta(status: MonitoringHostSummary["status"]) {
  if (status === "online") return { label: "在线", tone: "good" as const };
  if (status === "stale") return { label: "数据过期", tone: "warn" as const };
  return { label: "离线", tone: "danger" as const };
}

function HostCard({ host, selected, onSelect }: {
  host: MonitoringHostSummary;
  selected: boolean;
  onSelect: () => void;
}) {
  const status = statusMeta(host.status);
  const network = sumNullable(
    host.network_received_bytes_per_second,
    host.network_transmitted_bytes_per_second,
  );
  return (
    <button
      className={`content-card monitoring-host-card${selected ? " selected" : ""}`}
      type="button"
      onClick={onSelect}
      aria-pressed={selected}
      aria-label={`查看主机 ${host.name}`}
    >
      <CardInner>
        <CardRow label="主机">
          <TruncatedText grow><TickerText>{host.name || NA}</TickerText></TruncatedText>
          <span title={status.label}><StatusLed tone={status.tone} /></span>
        </CardRow>
        <CardRow label="状态">{status.label}</CardRow>
        <CardRow label="系统">
          <TruncatedText><TickerText>{[host.os, host.arch].filter(Boolean).join(" · ") || NA}</TickerText></TruncatedText>
        </CardRow>
        <CardRow label="CPU">{formatPercent(host.cpu_usage_percent)}</CardRow>
        <CardRow label="GPU">{formatPercent(host.gpu_utilization_percent)}</CardRow>
        <CardRow label="网络">{formatMetric(network, formatBytesPerSecond)}</CardRow>
      </CardInner>
    </button>
  );
}

function LiveMetrics({ host, report }: {
  host: MonitoringHostSummary;
  report: MonitoringAgentReport | null | undefined;
}) {
  const network = sumNullable(
    host.network_received_bytes_per_second,
    host.network_transmitted_bytes_per_second,
  );
  const disk = sumNullable(host.disk_read_bytes_per_second, host.disk_written_bytes_per_second);
  const memory = report?.system.memory;
  return (
    <div className="content-grid metric-grid">
      <Metric
        label="CPU"
        value={formatPercent(host.cpu_usage_percent)}
        detail={report ? `${report.system.cpu.logical_count} 个逻辑核心` : NA}
        tone={metricTone(host.cpu_usage_percent)}
      />
      <Metric
        label="内存"
        value={formatPercent(host.memory_usage_percent)}
        detail={memory ? `${formatBytes(memory.used_bytes)} / ${formatBytes(memory.total_bytes)}` : NA}
        tone={metricTone(host.memory_usage_percent)}
      />
      <Metric
        label="GPU"
        value={formatPercent(host.gpu_utilization_percent)}
        detail={isNumber(host.gpu_memory_usage_percent) ? `显存 ${formatPercent(host.gpu_memory_usage_percent)}` : NA}
        tone={metricTone(host.gpu_utilization_percent)}
      />
      <Metric
        label="网络"
        value={formatMetric(network, formatBytesPerSecond)}
        detail={`收 ${formatMetric(host.network_received_bytes_per_second, formatBytesPerSecond)}  发 ${formatMetric(host.network_transmitted_bytes_per_second, formatBytesPerSecond)}`}
        tone="neutral"
      />
      <Metric
        label="磁盘 I/O"
        value={formatMetric(disk, formatBytesPerSecond)}
        detail={`读 ${formatMetric(host.disk_read_bytes_per_second, formatBytesPerSecond)}  写 ${formatMetric(host.disk_written_bytes_per_second, formatBytesPerSecond)}`}
        tone="neutral"
      />
      <Metric
        label="温度"
        value={formatTemperature(host.max_temperature_celsius)}
        detail={isNumber(host.max_temperature_celsius) ? "当前最高温度" : NA}
        tone={metricTone(host.max_temperature_celsius, 80)}
      />
    </div>
  );
}

function NotAvailableCard({ label }: { label: string }) {
  return (
    <article className="content-card monitoring-detail-card">
      <CardInner><CardRow label={label}>{NA}</CardRow></CardInner>
    </article>
  );
}

function GpuCard({ gpu }: { gpu: MonitoringGpuReport }) {
  const memoryUsage = isNumber(gpu.memory_used_bytes) && isNumber(gpu.memory_total_bytes)
    ? percent(gpu.memory_used_bytes, gpu.memory_total_bytes)
    : null;
  return (
    <article className="content-card monitoring-detail-card">
      <CardInner>
        <CardRow label="GPU">
          <TruncatedText><TickerText>{gpu.name || gpu.id || NA}</TickerText></TruncatedText>
        </CardRow>
        <CardRow label="占用">{formatPercent(gpu.utilization_percent)}</CardRow>
        <CardRow label="显存">
          {isNumber(gpu.memory_used_bytes) && isNumber(gpu.memory_total_bytes)
            ? `${formatBytes(gpu.memory_used_bytes)} / ${formatBytes(gpu.memory_total_bytes)}`
            : NA}
        </CardRow>
        <CardRow label="显存率">{isNumber(memoryUsage) ? <ProgressBar value={memoryUsage} /> : NA}</CardRow>
        <CardRow label="温度">{formatTemperature(gpu.temperature_celsius)}</CardRow>
        <CardRow label="功耗">{formatMetric(gpu.power_watts, (value) => `${value.toFixed(1)} W`)}</CardRow>
      </CardInner>
    </article>
  );
}

function HardwareDetails({ report }: { report: MonitoringAgentReport | null | undefined }) {
  const system = report?.system;
  return (
    <>
      <section className="section-band">
        <SectionHeader icon={Network} title="网络接口" />
        <div className="content-grid">
          {system?.networks.length ? system.networks.map((network) => (
            <article className="content-card monitoring-detail-card" key={network.name}>
              <CardInner>
                <CardRow label="接口"><TruncatedText><TickerText>{network.name || NA}</TickerText></TruncatedText></CardRow>
                <CardRow label="接收">{formatBytesPerSecond(network.received_bytes_per_second)}</CardRow>
                <CardRow label="发送">{formatBytesPerSecond(network.transmitted_bytes_per_second)}</CardRow>
                <CardRow label="收包">{network.packets_received_total.toLocaleString()}</CardRow>
                <CardRow label="发包">{network.packets_transmitted_total.toLocaleString()}</CardRow>
                <CardRow label="错误">{(network.receive_errors_total + network.transmit_errors_total).toLocaleString()}</CardRow>
              </CardInner>
            </article>
          )) : <NotAvailableCard label="网络" />}
        </div>
      </section>

      <section className="section-band">
        <SectionHeader icon={HardDrive} title="磁盘与文件系统" />
        <div className="content-grid">
          {system?.disks.length ? system.disks.map((disk) => {
            const used = Math.max(0, disk.total_bytes - disk.available_bytes);
            return (
              <article className="content-card monitoring-detail-card" key={`${disk.name}-${disk.mount_point}`}>
                <CardInner>
                  <CardRow label="设备"><TruncatedText><TickerText>{disk.name || NA}</TickerText></TruncatedText></CardRow>
                  <CardRow label="挂载"><TruncatedText><TickerText>{disk.mount_point || NA}</TickerText></TruncatedText></CardRow>
                  <CardRow label="占用"><ProgressBar value={percent(used, disk.total_bytes)} /></CardRow>
                  <CardRow label="容量">{`${formatBytes(used)} / ${formatBytes(disk.total_bytes)}`}</CardRow>
                  <CardRow label="吞吐">{formatBytesPerSecond(disk.read_bytes_per_second + disk.written_bytes_per_second)}</CardRow>
                  <CardRow label="模式">{disk.is_read_only ? "只读" : "读写"}</CardRow>
                </CardInner>
              </article>
            );
          }) : <NotAvailableCard label="磁盘" />}
        </div>
      </section>

      <section className="section-band">
        <SectionHeader icon={CircuitBoard} title="GPU" />
        <div className="content-grid">
          {system?.gpus.length ? system.gpus.map((gpu) => <GpuCard key={gpu.id} gpu={gpu} />) : <NotAvailableCard label="GPU" />}
        </div>
      </section>

      <section className="section-band">
        <SectionHeader icon={Thermometer} title="温度传感器" />
        <div className="content-grid">
          {system?.temperatures.length ? system.temperatures.map((sensor) => (
            <article className="content-card monitoring-detail-card" key={sensor.id}>
              <CardInner>
                <CardRow label="传感器"><TruncatedText><TickerText>{sensor.label || sensor.id || NA}</TickerText></TruncatedText></CardRow>
                <CardRow label="当前">{formatTemperature(sensor.celsius)}</CardRow>
                <CardRow label="上限">{formatTemperature(sensor.max_celsius)}</CardRow>
                <CardRow label="临界">{formatTemperature(sensor.critical_celsius)}</CardRow>
                <CardRow label="来源"><TruncatedText><TickerText>{sensor.source || NA}</TickerText></TruncatedText></CardRow>
              </CardInner>
            </article>
          )) : <NotAvailableCard label="温度" />}
        </div>
      </section>
    </>
  );
}

function CapabilityCard({ capability }: { capability: MonitoringCapability }) {
  const detail = capability.message || capability.error_kind || NA;
  return (
    <article className="content-card monitoring-detail-card">
      <CardInner>
        <CardRow label="能力">
          <TruncatedText grow><TickerText>{capability.name || NA}</TickerText></TruncatedText>
          <StatusLed tone={capability.available ? "good" : "danger"} />
        </CardRow>
        <CardRow label="状态">{capability.available ? "支持" : "不可用"}</CardRow>
        <CardRow label="来源"><TruncatedText><TickerText>{capability.source || NA}</TickerText></TruncatedText></CardRow>
        <CardRow label="说明" span={3}><TruncatedText muted>{detail}</TruncatedText></CardRow>
      </CardInner>
    </article>
  );
}

function CapabilityDetails({ capabilities }: { capabilities: MonitoringCapability[] }) {
  return (
    <section className="section-band">
      <SectionHeader icon={ShieldCheck} title="采集能力" description="不可采集与真实值为 0 含义不同；缺失指标统一显示 N/A。" />
      <div className="content-grid">
        {capabilities.length
          ? capabilities.map((capability) => <CapabilityCard key={capability.name} capability={capability} />)
          : <NotAvailableCard label="能力" />}
      </div>
    </section>
  );
}

function historyValues(points: MonitoringHistoryPoint[], read: (point: MonitoringHistoryPoint) => number | null): number[] {
  return points.map(read).filter(isNumber);
}

function HistoryMetrics({ points }: { points: MonitoringHistoryPoint[] }) {
  const cpu = historyValues(points, (point) => point.cpu_usage_percent);
  const memory = historyValues(points, (point) => point.memory_usage_percent);
  const gpu = historyValues(points, (point) => point.gpu_utilization_percent);
  const temperature = historyValues(points, (point) => point.max_temperature_celsius);
  const network = historyValues(points, (point) => sumNullable(
    point.network_received_bytes_per_second,
    point.network_transmitted_bytes_per_second,
  ));
  const disk = historyValues(points, (point) => sumNullable(
    point.disk_read_bytes_per_second,
    point.disk_written_bytes_per_second,
  ));
  const last = (values: number[]) => values.at(-1);
  const detail = points.length ? `${points.length} 个采样点` : NA;
  return (
    <div className="content-grid metric-grid">
      <Metric label="CPU" value={formatPercent(last(cpu))} detail={detail} tone={metricTone(last(cpu))} sparkData={cpu} sparkMax={100} />
      <Metric label="内存" value={formatPercent(last(memory))} detail={detail} tone={metricTone(last(memory))} sparkData={memory} sparkMax={100} sparkColor="var(--warn)" />
      <Metric label="GPU" value={formatPercent(last(gpu))} detail={detail} tone={metricTone(last(gpu))} sparkData={gpu} sparkMax={100} sparkColor="var(--accent)" />
      <Metric label="网络" value={formatMetric(last(network), formatBytesPerSecond)} detail={detail} tone="neutral" sparkData={network} sparkColor="var(--good)" />
      <Metric label="磁盘 I/O" value={formatMetric(last(disk), formatBytesPerSecond)} detail={detail} tone="neutral" sparkData={disk} />
      <Metric label="温度" value={formatTemperature(last(temperature))} detail={detail} tone={metricTone(last(temperature), 80)} sparkData={temperature} sparkColor="var(--danger)" />
    </div>
  );
}

export function MonitoringView() {
  const [selectedHostId, setSelectedHostId] = useState<string | null>(null);
  const hostsQuery = useQuery({
    queryKey: queryKeys.monitoring.hosts,
    queryFn: api.monitoringHosts,
    refetchInterval: 10_000,
  });
  const hosts = hostsQuery.data?.hosts ?? [];

  useEffect(() => {
    if (!hosts.length) {
      setSelectedHostId(null);
    } else if (!selectedHostId || !hosts.some((host) => host.id === selectedHostId)) {
      setSelectedHostId(hosts[0].id);
    }
  }, [hosts, selectedHostId]);

  const detailQuery = useQuery({
    queryKey: queryKeys.monitoring.host(selectedHostId ?? ""),
    queryFn: () => api.monitoringHost(selectedHostId!),
    enabled: Boolean(selectedHostId),
    refetchInterval: 10_000,
  });
  const historyQuery = useQuery({
    queryKey: queryKeys.monitoring.history(selectedHostId ?? ""),
    queryFn: () => api.monitoringHistory(selectedHostId!),
    enabled: Boolean(selectedHostId),
    refetchInterval: 30_000,
  });
  const selectedSummary = hosts.find((host) => host.id === selectedHostId);
  const selectedHost = detailQuery.data?.host ?? selectedSummary;
  const latest = detailQuery.data?.latest;
  const historyPoints = useMemo(
    () => [...(historyQuery.data?.points ?? [])].sort((left, right) => left.collected_at.localeCompare(right.collected_at)),
    [historyQuery.data],
  );
  const onlineCount = hosts.filter((host) => host.status === "online").length;

  return (
    <section className="view-stack monitoring-view">
      <section className="section-band">
        <SectionHeader icon={MonitorDot} title="主机监控" description={`只读采集 · ${onlineCount}/${hosts.length} 台在线`} />
        {hostsQuery.isLoading ? <LoadingBlock label="正在读取主机状态" /> : null}
        {hostsQuery.error ? <InlineNotice tone="danger" text={hostsQuery.error.message} /> : null}
        {!hostsQuery.isLoading && !hostsQuery.error && !hosts.length ? <div className="empty-state">暂无 Agent 上报数据</div> : null}
        <div className="content-grid monitoring-host-grid">
          {hosts.map((host) => (
            <HostCard key={host.id} host={host} selected={host.id === selectedHostId} onSelect={() => setSelectedHostId(host.id)} />
          ))}
        </div>
      </section>

      {selectedHost ? (
        <>
          <section className="section-band">
            <SectionHeader
              icon={Activity}
              title={selectedHost.name || "主机详情"}
              description={`${statusMeta(selectedHost.status).label} · ${selectedHost.os || NA} ${selectedHost.os_version ?? ""} · Agent ${selectedHost.agent_version || NA} · 最后上报 ${formatDateTime(selectedHost.last_seen_at)}`}
            />
            {detailQuery.error ? <InlineNotice tone="danger" text={detailQuery.error.message} /> : null}
            {detailQuery.isLoading ? <LoadingBlock label="正在读取实时指标" /> : null}
            <LiveMetrics host={selectedHost} report={latest} />
          </section>
          <HardwareDetails report={latest} />
          <CapabilityDetails capabilities={selectedHost.capabilities ?? []} />
          <section className="section-band">
            <SectionHeader icon={Gauge} title="历史趋势" description="最近采样点；页面只读取状态，不会向主机发送控制命令。" />
            {historyQuery.isLoading ? <LoadingBlock label="正在读取历史指标" /> : null}
            {historyQuery.error ? <InlineNotice tone="danger" text={historyQuery.error.message} /> : null}
            <HistoryMetrics points={historyPoints} />
          </section>
        </>
      ) : null}
    </section>
  );
}
