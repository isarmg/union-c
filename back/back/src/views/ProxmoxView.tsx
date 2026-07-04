/**
 * Proxmox VE 多主机管理视图。
 *
 * 布局：左侧主机卡片列表 + 右侧选中主机的集群详情面板。
 * 集群详情分三个 tab：资源总览（VM/CT）、存储、任务日志。
 */

import { useEffect, useRef, useState } from "react";
import {
  AlertTriangle,
  Boxes,
  Check,
  ChevronDown,
  ChevronRight,
  Copy,
  Database,
  ExternalLink,
  HardDrive,
  ListTodo,
  Loader2,
  Monitor,
  Plus,
  Power,
  RefreshCw,
  RotateCcw,
  Server,
  SkipForward,
  Square,
  Trash2,
  X,
  Zap,
} from "lucide-react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "../api";
import { queryKeys } from "../query-keys";
import {
  calculateAdjacentPanelPosition,
  defaultAdjacentPanelSize,
  type AdjacentPanelPosition
} from "../layout";
import type {
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
} from "../types";
import { ContentTitle, InlineNotice, MutationError } from "../components/ui";
import { PveHostCard } from "../components/PveHostCard";

// ─── 格式化工具 ───────────────────────────────────────────────────────────────

function fmtBytes(bytes: number | undefined): string {
  if (!bytes || bytes === 0) return "—";
  const gb = bytes / 1024 / 1024 / 1024;
  if (gb >= 1) return `${gb.toFixed(1)} GB`;
  const mb = bytes / 1024 / 1024;
  return `${mb.toFixed(0)} MB`;
}

function fmtUptime(seconds: number | undefined): string {
  if (!seconds) return "—";
  const d = Math.floor(seconds / 86400);
  const h = Math.floor((seconds % 86400) / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  if (d > 0) return `${d}d ${h}h`;
  if (h > 0) return `${h}h ${m}m`;
  return `${m}m`;
}

function fmtCpu(usage: number | undefined): string {
  if (usage === undefined) return "—";
  return `${(usage * 100).toFixed(1)}%`;
}

function fmtTimestamp(ts: number | undefined): string {
  if (!ts) return "—";
  return new Date(ts * 1000).toLocaleString();
}

const RE_IPV4 = /^((25[0-5]|2[0-4]\d|1\d{2}|[1-9]\d|\d)\.){3}(25[0-5]|2[0-4]\d|1\d{2}|[1-9]\d|\d)$/;
const RE_IPV6 = /^\[?([0-9a-fA-F]{0,4}:){2,7}[0-9a-fA-F]{0,4}\]?$/;
const RE_DOMAIN = /^(?!-)[A-Za-z0-9-]{1,63}(?<!-)(\.[A-Za-z0-9-]{1,63}(?<!-))*\.?$/;

function isValidHost(value: string): boolean {
  const host = value.trim();
  return RE_IPV4.test(host) || RE_IPV6.test(host) || RE_DOMAIN.test(host);
}

// ─── 状态样式 ─────────────────────────────────────────────────────────────────

type VmStatus = "running" | "stopped" | "paused" | "suspended" | string;

function statusClass(status: VmStatus | undefined): string {
  switch (status) {
    case "running": return "pve-status-running";
    case "stopped": return "pve-status-stopped";
    case "paused":
    case "suspended": return "pve-status-paused";
    default: return "pve-status-unknown";
  }
}

function statusLabel(status: VmStatus | undefined): string {
  switch (status) {
    case "running": return "运行中";
    case "stopped": return "已停止";
    case "paused": return "已暂停";
    case "suspended": return "已挂起";
    default: return status ?? "未知";
  }
}

// ─── 快照面板 ─────────────────────────────────────────────────────────────────

function SnapshotPanel({
  hostId,
  node,
  vmid,
  type,
}: {
  hostId: string;
  node: string;
  vmid: number;
  type: "qemu" | "lxc";
}) {
  const qc = useQueryClient();
  const [creating, setCreating] = useState(false);
  const [snapName, setSnapName] = useState("");
  const [snapDesc, setSnapDesc] = useState("");
  const [vmstate, setVmstate] = useState(false);

  const snapsKey = queryKeys.pve.snapshots(hostId, node, vmid, type);
  const snapsQuery = useQuery({
    queryKey: snapsKey,
    queryFn: () =>
      type === "qemu"
        ? api.pveVmSnapshots(hostId, node, vmid)
        : api.pveCtSnapshots(hostId, node, vmid),
  });

  const createMut = useMutation({
    mutationFn: (req: PveSnapshotRequest) =>
      type === "qemu"
        ? api.pveVmSnapshotCreate(hostId, node, vmid, req)
        : api.pveCtSnapshotCreate(hostId, node, vmid, req),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: snapsKey });
      setCreating(false);
      setSnapName("");
      setSnapDesc("");
    },
  });

  const deleteMut = useMutation({
    mutationFn: (snap: string) =>
      type === "qemu"
        ? api.pveVmSnapshotDelete(hostId, node, vmid, snap)
        : api.pveCtSnapshotDelete(hostId, node, vmid, snap),
    onSuccess: () => void qc.invalidateQueries({ queryKey: snapsKey }),
  });

  const rollbackMut = useMutation({
    mutationFn: (snap: string) =>
      type === "qemu"
        ? api.pveVmSnapshotRollback(hostId, node, vmid, snap)
        : api.pveCtSnapshotRollback(hostId, node, vmid, snap),
    onSuccess: () => void qc.invalidateQueries({ queryKey: snapsKey }),
  });

  const snaps: PveSnapshot[] = (snapsQuery.data ?? []).filter((s: PveSnapshot) => s.name !== "current");

  return (
    <div className="pve-snapshots">
      <div className="pve-snap-header">
        <span>快照</span>
        <button className="action-button compact" onClick={() => setCreating(!creating)}>
          <Plus size={13} /> 新建
        </button>
      </div>

      {creating && (
        <div className="pve-snap-form">
          <input value={snapName} onChange={e => setSnapName(e.target.value)} placeholder="快照名称（字母数字）" />
          <input value={snapDesc} onChange={e => setSnapDesc(e.target.value)} placeholder="描述（可选）" />
          {type === "qemu" && (
            <label className="pve-check-label">
              <input type="checkbox" checked={vmstate} onChange={e => setVmstate(e.target.checked)} />
              包含内存状态
            </label>
          )}
          <div className="pve-snap-form-actions">
            <button className="action-button compact" onClick={() => setCreating(false)}>取消</button>
            <button
              className="action-button primary compact"
              disabled={!snapName.trim() || createMut.isPending}
              onClick={() => createMut.mutate({ snapname: snapName.trim(), description: snapDesc || undefined, vmstate })}
            >
              {createMut.isPending ? <Loader2 size={13} className="spin" /> : <Check size={13} />} 创建
            </button>
          </div>
          <MutationError mutation={createMut} />
        </div>
      )}

      {snapsQuery.isLoading && <div className="pve-loading-small"><Loader2 size={14} className="spin" /></div>}
      {snapsQuery.error && <InlineNotice tone="danger" text={String(snapsQuery.error)} />}

      {snaps.length === 0 && !snapsQuery.isLoading && (
        <div className="pve-empty-small">暂无快照</div>
      )}

      {snaps.map((snap: PveSnapshot) => (
        <div className="pve-snap-row" key={snap.name}>
          <div className="pve-snap-info">
            <span className="pve-snap-name">{snap.name}</span>
            {snap.description && <span className="pve-snap-desc">{snap.description}</span>}
            {snap.snaptime && <span className="pve-snap-time">{fmtTimestamp(snap.snaptime)}</span>}
          </div>
          <div className="pve-snap-btns">
            <button
              className="action-button compact"
              title="回滚到此快照"
              disabled={rollbackMut.isPending}
              onClick={() => { if (confirm(`确定回滚到快照 "${snap.name}"？`)) rollbackMut.mutate(snap.name); }}
            >
              <RotateCcw size={12} />
            </button>
            <button
              className="action-button compact danger"
              title="删除快照"
              disabled={deleteMut.isPending}
              onClick={() => { if (confirm(`确定删除快照 "${snap.name}"？`)) deleteMut.mutate(snap.name); }}
            >
              <Trash2 size={12} />
            </button>
          </div>
        </div>
      ))}
    </div>
  );
}

// ─── 迁移面板 ─────────────────────────────────────────────────────────────────

function MigratePanel({
  hostId,
  node,
  vmid,
  nodes,
  onClose,
}: {
  hostId: string;
  node: string;
  vmid: number;
  nodes: PveNodeInfo[];
  onClose: () => void;
}) {
  const [target, setTarget] = useState("");
  const [online, setOnline] = useState(true);
  const [withDisks, setWithDisks] = useState(false);

  const migrateMut = useMutation({
    mutationFn: (req: PveMigrateRequest) => api.pveVmMigrate(hostId, node, vmid, req),
    onSuccess: onClose,
  });

  const otherNodes = nodes.filter(n => n.node !== node);

  return (
    <div className="pve-migrate-panel">
      <div className="pve-migrate-title">迁移 VM {vmid}</div>
      <label className="inline-field">
        <span>目标节点</span>
        <select value={target} onChange={e => setTarget(e.target.value)}>
          <option value="">选择节点…</option>
          {otherNodes.map(n => (
            <option key={n.node} value={n.node}>{n.node}</option>
          ))}
        </select>
      </label>
      <label className="pve-check-label">
        <input type="checkbox" checked={online} onChange={e => setOnline(e.target.checked)} />
        在线迁移（VM 运行时迁移）
      </label>
      <label className="pve-check-label">
        <input type="checkbox" checked={withDisks} onChange={e => setWithDisks(e.target.checked)} />
        迁移本地磁盘
      </label>
      <div className="pve-migrate-actions">
        <button className="action-button" onClick={onClose}>取消</button>
        <button
          className="action-button primary"
          disabled={!target || migrateMut.isPending}
          onClick={() => migrateMut.mutate({ target, online, with_local_disks: withDisks })}
        >
          {migrateMut.isPending ? <Loader2 size={14} className="spin" /> : <SkipForward size={14} />}
          迁移
        </button>
      </div>
      <MutationError mutation={migrateMut} />
    </div>
  );
}

// ─── VM/CT 配置查看器 ─────────────────────────────────────────────────────────

function ConfigViewer({
  hostId,
  node,
  vmid,
  type,
}: {
  hostId: string;
  node: string;
  vmid: number;
  type: "qemu" | "lxc";
}) {
  const configQuery = useQuery({
    queryKey: queryKeys.pve.config(hostId, node, vmid, type),
    queryFn: () =>
      type === "qemu"
        ? api.pveVmConfig(hostId, node, vmid)
        : api.pveCtConfig(hostId, node, vmid),
  });

  if (configQuery.isLoading) return <div className="pve-loading-small"><Loader2 size={14} className="spin" /></div>;
  if (configQuery.error) return <InlineNotice tone="danger" text={String(configQuery.error)} />;
  const config = configQuery.data ?? {};

  return (
    <div className="pve-config-table">
      {Object.entries(config).map(([k, v]) => (
        <div className="pve-config-row" key={k}>
          <span className="pve-config-key">{k}</span>
          <span className="pve-config-val">{String(v)}</span>
        </div>
      ))}
    </div>
  );
}

// ─── 资源行（VM/CT 一行） ─────────────────────────────────────────────────────

function ResourceRow({
  res,
  hostId,
  nodes,
  consoleBaseUrl,
}: {
  res: PveResource;
  hostId: string;
  nodes: PveNodeInfo[];
  consoleBaseUrl: string;
}) {
  const [expanded, setExpanded] = useState(false);
  const [panel, setPanel] = useState<"snapshots" | "config" | "migrate" | null>(null);
  const qc = useQueryClient();

  const isVm = res.type === "qemu";
  const kind = isVm ? "qemu" : "lxc";
  const node = res.node ?? "";
  const vmid = res.vmid ?? 0;
  const status = res.status ?? "unknown";

  const resourcesKey = queryKeys.pve.resources(hostId);

  const mutOpts = {
    onSuccess: () => void qc.invalidateQueries({ queryKey: resourcesKey }),
  };

  const startMut = useMutation({
    mutationFn: () => isVm ? api.pveVmStart(hostId, node, vmid) : api.pveCtStart(hostId, node, vmid),
    ...mutOpts,
  });
  const stopMut = useMutation({
    mutationFn: () => isVm ? api.pveVmStop(hostId, node, vmid) : api.pveCtStop(hostId, node, vmid),
    ...mutOpts,
  });
  const shutdownMut = useMutation({
    mutationFn: () => isVm ? api.pveVmShutdown(hostId, node, vmid) : api.pveCtShutdown(hostId, node, vmid),
    ...mutOpts,
  });
  const rebootMut = useMutation({
    mutationFn: () => isVm ? api.pveVmReboot(hostId, node, vmid) : api.pveCtReboot(hostId, node, vmid),
    ...mutOpts,
  });
  const suspendMut = useMutation({
    mutationFn: () => isVm ? api.pveVmSuspend(hostId, node, vmid) : Promise.resolve(null),
    ...mutOpts,
  });
  const resumeMut = useMutation({
    mutationFn: () => isVm ? api.pveVmResume(hostId, node, vmid) : Promise.resolve(null),
    ...mutOpts,
  });
  const resetMut = useMutation({
    mutationFn: () => isVm ? api.pveVmReset(hostId, node, vmid) : Promise.resolve(null),
    ...mutOpts,
  });
  const deleteMut = useMutation({
    mutationFn: () => isVm ? api.pveVmDelete(hostId, node, vmid) : api.pveCtDelete(hostId, node, vmid),
    ...mutOpts,
  });

  const isRunning = status === "running";
  const isStopped = status === "stopped";
  const isSuspended = status === "suspended" || status === "paused";
  const anyPending = [startMut, stopMut, shutdownMut, rebootMut, suspendMut, resumeMut, resetMut, deleteMut]
    .some(m => m.isPending);

  const consoleUrl = isVm
    ? `${consoleBaseUrl}/?console=kvm&novnc=1&node=${node}&vmid=${vmid}&resize=scale`
    : `${consoleBaseUrl}/?console=lxc&novnc=1&node=${node}&vmid=${vmid}&resize=scale`;

  const togglePanel = (p: typeof panel) => {
    setExpanded(true);
    setPanel(prev => (prev === p ? null : p));
  };

  return (
    <>
      <div className={`pve-resource-row ${expanded ? "expanded" : ""}`}>
        <div className="pve-res-expand" onClick={() => setExpanded(e => !e)}>
          {expanded ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
        </div>
        <div className="pve-res-type-badge">
          {isVm ? <Monitor size={13} /> : <Server size={13} />}
          <span>{isVm ? "VM" : "CT"}</span>
        </div>
        <div className="pve-res-vmid">{vmid}</div>
        <div className="pve-res-name">{res.name ?? "—"}</div>
        <div className="pve-res-node">{node}</div>
        <div className={`pve-status-badge ${statusClass(status)}`}>{statusLabel(status)}</div>
        <div className="pve-res-cpu">{fmtCpu(res.cpu)}</div>
        <div className="pve-res-mem">
          {fmtBytes(res.mem)} / {fmtBytes(res.maxmem)}
        </div>
        <div className="pve-res-uptime">{isRunning ? fmtUptime(res.uptime) : "—"}</div>
        <div className="pve-res-actions">
          {isStopped && (
            <button className="pve-btn green" title="启动" disabled={anyPending} onClick={() => startMut.mutate()}>
              {startMut.isPending ? <Loader2 size={13} className="spin" /> : <Power size={13} />}
            </button>
          )}
          {isRunning && (
            <>
              <button className="pve-btn yellow" title="ACPI 关机" disabled={anyPending} onClick={() => shutdownMut.mutate()}>
                {shutdownMut.isPending ? <Loader2 size={13} className="spin" /> : <Power size={13} />}
              </button>
              <button className="pve-btn blue" title="重启" disabled={anyPending} onClick={() => rebootMut.mutate()}>
                {rebootMut.isPending ? <Loader2 size={13} className="spin" /> : <RefreshCw size={13} />}
              </button>
              <button className="pve-btn red" title="强制停止" disabled={anyPending} onClick={() => { if (confirm("强制停止 VM？")) stopMut.mutate(); }}>
                {stopMut.isPending ? <Loader2 size={13} className="spin" /> : <Square size={13} />}
              </button>
              {isVm && (
                <button className="pve-btn gray" title="挂起" disabled={anyPending} onClick={() => suspendMut.mutate()}>
                  {suspendMut.isPending ? <Loader2 size={13} className="spin" /> : <Zap size={13} />}
                </button>
              )}
              {isVm && (
                <button className="pve-btn gray" title="重置" disabled={anyPending} onClick={() => { if (confirm("重置 VM？")) resetMut.mutate(); }}>
                  {resetMut.isPending ? <Loader2 size={13} className="spin" /> : <RotateCcw size={13} />}
                </button>
              )}
            </>
          )}
          {isSuspended && (
            <>
              <button className="pve-btn green" title="恢复" disabled={anyPending} onClick={() => resumeMut.mutate()}>
                {resumeMut.isPending ? <Loader2 size={13} className="spin" /> : <Power size={13} />}
              </button>
              <button className="pve-btn red" title="强制停止" disabled={anyPending} onClick={() => stopMut.mutate()}>
                {stopMut.isPending ? <Loader2 size={13} className="spin" /> : <Square size={13} />}
              </button>
            </>
          )}
          <a className="pve-btn blue" href={consoleUrl} target="_blank" rel="noreferrer" title="打开控制台">
            <Monitor size={13} />
          </a>
        </div>
        <div className="pve-res-detail-btns">
          <button className="pve-detail-btn" onClick={() => togglePanel("snapshots")} title="快照">
            <Copy size={12} />
          </button>
          <button className="pve-detail-btn" onClick={() => togglePanel("config")} title="配置">
            <Database size={12} />
          </button>
          {isVm && (
            <button className="pve-detail-btn" onClick={() => togglePanel("migrate")} title="迁移">
              <SkipForward size={12} />
            </button>
          )}
          <button
            className="pve-detail-btn danger"
            onClick={() => { if (confirm(`确定删除 ${isVm ? "VM" : "CT"} ${vmid}？`)) deleteMut.mutate(); }}
            title="删除"
          >
            <Trash2 size={12} />
          </button>
        </div>
      </div>

      {expanded && panel && (
        <div className="pve-resource-detail">
          {panel === "snapshots" && (
            <SnapshotPanel hostId={hostId} node={node} vmid={vmid} type={kind as "qemu" | "lxc"} />
          )}
          {panel === "config" && (
            <ConfigViewer hostId={hostId} node={node} vmid={vmid} type={kind as "qemu" | "lxc"} />
          )}
          {panel === "migrate" && isVm && (
            <MigratePanel
              hostId={hostId}
              node={node}
              vmid={vmid}
              nodes={nodes}
              onClose={() => setPanel(null)}
            />
          )}
        </div>
      )}
    </>
  );
}

// ─── 资源总览 tab ─────────────────────────────────────────────────────────────

function ResourcesTab({ hostId, webUrl }: { hostId: string; webUrl: string }) {
  const [nodeFilter, setNodeFilter] = useState("");
  const [statusFilter, setStatusFilter] = useState<"all" | "running" | "stopped">("all");
  const [typeFilter, setTypeFilter] = useState<"all" | "qemu" | "lxc">("all");

  const resourcesQuery = useQuery({
    queryKey: queryKeys.pve.resources(hostId),
    queryFn: () => api.pveResources(hostId),
    refetchInterval: 15_000,
  });

  const nodesQuery = useQuery({
    queryKey: queryKeys.pve.nodes(hostId),
    queryFn: () => api.pveNodes(hostId),
    refetchInterval: 30_000,
  });

  const nodes: PveNodeInfo[] = nodesQuery.data ?? [];
  const resources: PveResource[] = (resourcesQuery.data ?? []);

  const nodeItems = resources.filter(r => r.type === "node");
  const vmItems = resources.filter(r =>
    (r.type === "qemu" || r.type === "lxc") &&
    (typeFilter === "all" || r.type === typeFilter) &&
    (statusFilter === "all" || r.status === statusFilter) &&
    (nodeFilter === "" || r.node === nodeFilter)
  );

  const totalVms = resources.filter(r => r.type === "qemu").length;
  const totalCts = resources.filter(r => r.type === "lxc").length;
  const runningVms = resources.filter(r => r.type === "qemu" && r.status === "running").length;
  const runningCts = resources.filter(r => r.type === "lxc" && r.status === "running").length;

  return (
    <div className="pve-tab-content">
      {/* 节点摘要行 */}
      <div className="pve-nodes-strip">
        {nodeItems.map(n => (
          <div
            key={n.node}
            className={`pve-node-chip ${nodeFilter === n.node ? "active" : ""}`}
            onClick={() => setNodeFilter(f => f === n.node ? "" : n.node ?? "")}
          >
            <span className={`pve-status-dot ${n.status === "online" ? "pve-status-running" : "pve-status-stopped"}`} />
            <strong>{n.node}</strong>
            <span className="pve-node-cpu">CPU {fmtCpu(n.cpu)}</span>
            <span>{fmtBytes(n.mem)}/{fmtBytes(n.maxmem)}</span>
          </div>
        ))}
        <div className="pve-summary-chip">
          <Monitor size={13} /> {runningVms}/{totalVms} VM
        </div>
        <div className="pve-summary-chip">
          <Server size={13} /> {runningCts}/{totalCts} CT
        </div>
      </div>

      {/* 过滤条 */}
      <div className="pve-filter-bar">
        <div className="pve-filter-group">
          <button className={`pve-filter-btn ${typeFilter === "all" ? "active" : ""}`} onClick={() => setTypeFilter("all")}>全部</button>
          <button className={`pve-filter-btn ${typeFilter === "qemu" ? "active" : ""}`} onClick={() => setTypeFilter("qemu")}>VM</button>
          <button className={`pve-filter-btn ${typeFilter === "lxc" ? "active" : ""}`} onClick={() => setTypeFilter("lxc")}>CT</button>
        </div>
        <div className="pve-filter-group">
          <button className={`pve-filter-btn ${statusFilter === "all" ? "active" : ""}`} onClick={() => setStatusFilter("all")}>全部状态</button>
          <button className={`pve-filter-btn ${statusFilter === "running" ? "active" : ""}`} onClick={() => setStatusFilter("running")}>运行中</button>
          <button className={`pve-filter-btn ${statusFilter === "stopped" ? "active" : ""}`} onClick={() => setStatusFilter("stopped")}>已停止</button>
        </div>
        {nodeFilter && (
          <button className="pve-filter-clear" onClick={() => setNodeFilter("")}>
            <X size={12} /> 清除节点过滤
          </button>
        )}
      </div>

      {resourcesQuery.isLoading && (
        <div className="pve-loading"><Loader2 size={20} className="spin" /> 加载资源…</div>
      )}
      {resourcesQuery.error && (
        <InlineNotice tone="danger" text={`加载失败: ${String(resourcesQuery.error)}`} />
      )}

      {vmItems.length > 0 && (
        <div className="pve-resource-table">
          <div className="pve-resource-head">
            <div />
            <div>类型</div>
            <div>ID</div>
            <div>名称</div>
            <div>节点</div>
            <div>状态</div>
            <div>CPU</div>
            <div>内存</div>
            <div>运行时长</div>
            <div>操作</div>
            <div />
          </div>
          {vmItems.map(res => (
            <ResourceRow
              key={res.id}
              res={res}
              hostId={hostId}
              nodes={nodes}
              consoleBaseUrl={webUrl}
            />
          ))}
        </div>
      )}

      {!resourcesQuery.isLoading && vmItems.length === 0 && (
        <div className="pve-empty">当前过滤条件下没有 VM/CT</div>
      )}
    </div>
  );
}

// ─── 存储 tab ─────────────────────────────────────────────────────────────────

function StorageTab({ hostId }: { hostId: string }) {
  const [selectedNode, setSelectedNode] = useState<string | null>(null);
  const [selectedStorage, setSelectedStorage] = useState<string | null>(null);

  const nodesQuery = useQuery({
    queryKey: queryKeys.pve.nodes(hostId),
    queryFn: () => api.pveNodes(hostId),
  });

  const storageQuery = useQuery({
    queryKey: queryKeys.pve.storage(hostId, selectedNode),
    queryFn: () => api.pveNodeStorage(hostId, selectedNode!),
    enabled: !!selectedNode,
  });

  const contentQuery = useQuery({
    queryKey: queryKeys.pve.content(hostId, selectedNode, selectedStorage),
    queryFn: () => api.pveStorageContent(hostId, selectedNode!, selectedStorage!),
    enabled: !!selectedNode && !!selectedStorage,
  });

  const nodes: PveNodeInfo[] = nodesQuery.data ?? [];
  const storages: PveStorageInfo[] = storageQuery.data ?? [];
  const contents: PveContentItem[] = contentQuery.data ?? [];

  return (
    <div className="pve-tab-content pve-storage-layout">
      {/* 节点选择 */}
      <div className="pve-storage-nodes">
        <div className="pve-section-title">节点</div>
        {nodes.map(n => (
          <button
            key={n.node}
            className={`pve-storage-node-btn ${selectedNode === n.node ? "active" : ""}`}
            onClick={() => { setSelectedNode(n.node); setSelectedStorage(null); }}
          >
            <span className={`pve-status-dot ${n.status === "online" ? "pve-status-running" : "pve-status-stopped"}`} />
            {n.node}
          </button>
        ))}
      </div>

      {/* 存储列表 */}
      <div className="pve-storage-list">
        <div className="pve-section-title">存储池</div>
        {!selectedNode && <div className="pve-empty-small">请选择节点</div>}
        {storageQuery.isLoading && <Loader2 size={14} className="spin" />}
        {storages.map(s => (
          <button
            key={s.storage}
            className={`pve-storage-item ${selectedStorage === s.storage ? "active" : ""}`}
            onClick={() => setSelectedStorage(s.storage)}
          >
            <HardDrive size={14} />
            <span>{s.storage}</span>
            <span className="pve-storage-type">{s.type}</span>
            {s.total ? (
              <span className="pve-storage-usage">
                {fmtBytes(s.used ?? 0)} / {fmtBytes(s.total)}
              </span>
            ) : null}
          </button>
        ))}
      </div>

      {/* 内容 */}
      <div className="pve-storage-content">
        <div className="pve-section-title">内容</div>
        {!selectedStorage && <div className="pve-empty-small">请选择存储池</div>}
        {contentQuery.isLoading && <Loader2 size={14} className="spin" />}
        {contentQuery.error && <InlineNotice tone="danger" text={String(contentQuery.error)} />}
        {contents.length > 0 && (
          <div className="pve-content-table">
            <div className="pve-content-head">
              <div>文件</div>
              <div>类型</div>
              <div>格式</div>
              <div>大小</div>
              <div>VM ID</div>
            </div>
            {contents.map(c => (
              <div className="pve-content-row" key={c.volid}>
                <div className="pve-content-volid">{c.volid.split("/").pop()}</div>
                <div>{c.content}</div>
                <div>{c.format ?? "—"}</div>
                <div>{fmtBytes(c.size)}</div>
                <div>{c.vmid ?? "—"}</div>
              </div>
            ))}
          </div>
        )}
        {!contentQuery.isLoading && selectedStorage && contents.length === 0 && (
          <div className="pve-empty-small">存储池为空</div>
        )}
      </div>
    </div>
  );
}

// ─── 任务 tab ─────────────────────────────────────────────────────────────────

function TasksTab({ hostId }: { hostId: string }) {
  const tasksQuery = useQuery({
    queryKey: queryKeys.pve.tasks(hostId),
    queryFn: () => api.pveTasks(hostId),
    refetchInterval: 10_000,
  });

  const tasks: PveTaskInfo[] = tasksQuery.data ?? [];

  return (
    <div className="pve-tab-content">
      {tasksQuery.isLoading && <div className="pve-loading"><Loader2 size={16} className="spin" /></div>}
      {tasksQuery.error && <InlineNotice tone="danger" text={String(tasksQuery.error)} />}

      <div className="pve-task-list">
        {tasks.map(t => (
          <div className="pve-task-row" key={t.upid}>
            <div className={`pve-task-status ${t.status === "OK" ? "ok" : t.status ? "fail" : "running"}`}>
              {t.status === "OK" ? <Check size={13} /> : t.status ? <AlertTriangle size={13} /> : <Loader2 size={13} className="spin" />}
            </div>
            <div className="pve-task-type">{t.type}</div>
            <div className="pve-task-node">{t.node}</div>
            <div className="pve-task-user">{t.user}</div>
            <div className="pve-task-id">{t.id ?? "—"}</div>
            <div className="pve-task-time">{fmtTimestamp(t.starttime)}</div>
            {t.status && t.status !== "OK" && (
              <div className="pve-task-err">{t.status}</div>
            )}
          </div>
        ))}
        {!tasksQuery.isLoading && tasks.length === 0 && (
          <div className="pve-empty">暂无任务记录</div>
        )}
      </div>
    </div>
  );
}

// ─── 主机详情面板 ─────────────────────────────────────────────────────────────

type HostTab = "resources" | "storage" | "tasks";

function HostPanel({ host, onClose }: { host: PveHostInfo; onClose: () => void }) {
  const [tab, setTab] = useState<HostTab>("resources");

  return (
    <div className="pve-host-panel">
      <div className="pve-panel-header">
        <div className="pve-panel-title">
          <strong>{host.name}</strong>
          <span className="pve-panel-addr">{host.host}:{host.port}</span>
          <a href={host.web_url} target="_blank" rel="noreferrer" className="icon-button" title="打开 PVE Web UI">
            <ExternalLink size={14} />
          </a>
        </div>
        <button className="icon-button" onClick={onClose}><X size={16} /></button>
      </div>

      <div className="pve-tab-bar">
        <button className={tab === "resources" ? "active" : ""} onClick={() => setTab("resources")}>
          <Monitor size={14} /> 资源
        </button>
        <button className={tab === "storage" ? "active" : ""} onClick={() => setTab("storage")}>
          <HardDrive size={14} /> 存储
        </button>
        <button className={tab === "tasks" ? "active" : ""} onClick={() => setTab("tasks")}>
          <ListTodo size={14} /> 任务
        </button>
      </div>

      {tab === "resources" && <ResourcesTab hostId={host.id} webUrl={host.web_url} />}
      {tab === "storage" && <StorageTab hostId={host.id} />}
      {tab === "tasks" && <TasksTab hostId={host.id} />}
    </div>
  );
}

// ─── ProxmoxView 根组件 ───────────────────────────────────────────────────────

export function ProxmoxView({ addTrigger = 0 }: { addTrigger?: number }) {
  const qc = useQueryClient();
  const [selectedHostId, setSelectedHostId] = useState<string | null>(null);
  const [panelPos, setPanelPos] = useState<AdjacentPanelPosition | null>(null);

  const outerRef = useRef<HTMLDivElement>(null);
  const gridRef  = useRef<HTMLDivElement>(null);
  const handledAddTriggerRef = useRef(addTrigger);

  const hostsQuery = useQuery({
    queryKey: queryKeys.pve.hosts,
    queryFn: () => api.pveHosts(),
    refetchInterval: 30_000,
  });

  const createMut = useMutation({
    mutationFn: (req: PveHostSaveRequest) => api.pveCreateHost(req),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: queryKeys.pve.hosts });
      setSelectedHostId(null);
    },
  });

  const updateMut = useMutation({
    mutationFn: ({ id, req }: { id: string; req: PveHostSaveRequest }) => api.pveUpdateHost(id, req),
    onSuccess: (_, { id }) => {
      void qc.invalidateQueries({ queryKey: queryKeys.pve.hosts });
      setSelectedHostId(id);
    },
  });

  const deleteMut = useMutation({
    mutationFn: (id: string) => api.pveDeleteHost(id),
    onSuccess: (_, id) => {
      void qc.invalidateQueries({ queryKey: queryKeys.pve.hosts });
      if (selectedHostId === id) { setSelectedHostId(null); setPanelPos(null); }
    },
  });

  const hosts: PveHostInfo[] = hostsQuery.data ?? [];
  const selectedHost = hosts.find(h => h.id === selectedHostId) ?? null;
  const panelOpen = selectedHostId !== null;

  const hostPatchRequest = (host: PveHostInfo, patch: { name?: string; host?: string; port?: number; token_id?: string; token_secret?: string; verify_tls?: boolean }): PveHostSaveRequest => ({
    name: patch.name ?? host.name,
    host: patch.host ?? host.host,
    port: patch.port ?? host.port,
    token_id: patch.token_id ?? host.token_id,
    token_secret: patch.token_secret ?? null,
    verify_tls: patch.verify_tls ?? host.verify_tls,
  });

  // 导航栏"+"触发：用有效默认值直接创建一个新内容块。
  useEffect(() => {
    if (addTrigger <= handledAddTriggerRef.current) return;
    handledAddTriggerRef.current = addTrigger;
    createDefaultHost();
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [addTrigger]);

  function createDefaultHost() {
    const usedNames = new Set(hosts.map(host => host.name));
    let index = hosts.length + 1;
    while (usedNames.has(`Proxmox ${index}`)) index += 1;
    createMut.mutate({
      name: `Proxmox ${index}`,
      host: "192.168.1.1",
      port: 8006,
      token_id: "root@pam!token",
      token_secret: null,
      verify_tls: true,
    });
    setSelectedHostId(null);
    setPanelPos(null);
  }

  function posFromCard(el: HTMLElement): AdjacentPanelPosition | null {
    return outerRef.current && gridRef.current
      ? calculateAdjacentPanelPosition(el, outerRef.current, gridRef.current)
      : null;
  }

  function handleSelectHost(id: string, el: HTMLElement) {
    if (selectedHostId === id) {
      setSelectedHostId(null); setPanelPos(null);
    } else {
      setSelectedHostId(id);
      setPanelPos(posFromCard(el));
    }
  }

  const { width: defW, height: defH } = defaultAdjacentPanelSize(gridRef.current);
  const panelStyle: React.CSSProperties = panelPos
    ? panelPos.side === "right"
      ? { top: panelPos.top, left: panelPos.left, width: panelPos.width, height: panelPos.height }
      : { top: panelPos.top, right: panelPos.right, width: panelPos.width, height: panelPos.height }
    : { top: 0, right: 0, width: defW, height: defH };

  return (
    <div className="view-root pve-view">
      <div className="adjacent-cards-outer pve-cards-outer" ref={outerRef}>
        {hostsQuery.isLoading && <div className="pve-loading"><Loader2 size={16} className="spin" /></div>}
        {hostsQuery.error && <InlineNotice tone="danger" text={String(hostsQuery.error)} />}
        <MutationError mutation={createMut} />
        <MutationError mutation={updateMut} />
        <MutationError mutation={deleteMut} />

        <div className="instance-list-title"><ContentTitle icon={Boxes} title="实例" /></div>
        <div className="content-grid pve-host-grid" ref={gridRef}>
          {hosts.map(host => (
            <PveHostCard
              key={host.id}
              host={host}
              selected={selectedHostId === host.id}
              onSelect={el => handleSelectHost(host.id, el)}
              validateHost={isValidHost}
              onInlineUpdate={patch => updateMut.mutateAsync({ id: host.id, req: hostPatchRequest(host, patch) }).then(() => undefined)}
              onDelete={() => { if (confirm(`删除 "${host.name}"？`)) deleteMut.mutate(host.id); }}
            />
          ))}
        </div>

        {!hostsQuery.isLoading && hosts.length === 0 && (
          <div className="pve-empty">
            <Server size={36} style={{ opacity: 0.2 }} />
            <p>暂无 PVE 主机，点击导航栏 + 新建</p>
          </div>
        )}

        {panelOpen && (
          <div className="adjacent-panel pve-adj-panel" style={panelStyle}>
            {selectedHost ? (
              <HostPanel
                host={selectedHost}
                onClose={() => { setSelectedHostId(null); setPanelPos(null); }}
              />
            ) : null}
          </div>
        )}
      </div>
    </div>
  );
}
