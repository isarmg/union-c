// Sunshine 多主机管理视图。
//
// 左侧：主机列表（增删改）。
// 右侧：选中主机的详细管理（6 个功能 tab）。

import { useEffect, useRef, useState } from "react";
import {
  AppWindow,
  Boxes,
  Check,
  ExternalLink,
  KeyRound,
  Plus,
  Power,
  RefreshCw,
  RotateCcw,
  Settings2,
  ToggleLeft,
  ToggleRight,
  Trash2,
  Unlink,
  Users,
  Wrench,
  X
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
  SunshineApp,
  SunshineAppsResponse,
  SunshineClient,
  SunshineClientsResponse,
  SunshineConfig,
  SunshineHostInfo,
  SunshineHostSaveRequest
} from "../types";
import {
  ActionButton,
  CardActions,
  CardInner,
  CardRow,
  ContentTitle,
  InlineNotice,
  LoadingBlock,
  MutationError,
  SectionHeader,
  StatusLed,
  TruncatedText
} from "../components/ui";

// ─── 类型 ─────────────────────────────────────────────────────────────────────

type HostSection = "apps" | "clients" | "pairing" | "config" | "system";

const HOST_SECTIONS: Array<{ key: HostSection; label: string; Icon: React.ComponentType<{ size?: number }> }> = [
  { key: "apps",    label: "应用",   Icon: AppWindow },
  { key: "clients", label: "客户端", Icon: Users     },
  { key: "pairing", label: "配对",   Icon: KeyRound  },
  { key: "config",  label: "配置",   Icon: Settings2 },
  { key: "system",  label: "系统",   Icon: Wrench    }
];

const RE_IPV4       = /^((25[0-5]|2[0-4]\d|1\d{2}|[1-9]\d|\d)\.){3}(25[0-5]|2[0-4]\d|1\d{2}|[1-9]\d|\d)$/;
const RE_IPV6       = /^\[?([0-9a-fA-F]{0,4}:){2,7}[0-9a-fA-F]{0,4}\]?$/;
const RE_DOMAIN     = /^(?!-)[A-Za-z0-9-]{1,63}(?<!-)(\.[A-Za-z0-9-]{1,63}(?<!-))*\.?$/;

function isValidHost(v: string): boolean {
  return RE_IPV4.test(v) || RE_IPV6.test(v) || RE_DOMAIN.test(v);
}

// ─── 主机卡片（article + 底部三按钮） ────────────────────────────────────────

function InlineHostField({ value, label, validate, onSave, compact = false, displayValue, inputType = "text" }: {
  value: string;
  label: string;
  validate: (value: string) => string | null;
  onSave: (value: string) => Promise<void>;
  compact?: boolean;
  displayValue?: string;
  inputType?: "text" | "password";
}) {
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(value);
  const [error, setError] = useState("");
  const committingRef = useRef(false);
  const skipBlurRef = useRef(false);

  useEffect(() => { if (!editing) setDraft(value); }, [editing, value]);

  const cancel = () => {
    skipBlurRef.current = true;
    setDraft(value);
    setError("");
    setEditing(false);
  };

  const commit = async () => {
    if (committingRef.current) return;
    const next = draft.trim();
    const validationError = validate(next);
    if (validationError) {
      setError(validationError);
      return;
    }
    if (next === value) {
      setEditing(false);
      return;
    }
    committingRef.current = true;
    try {
      await onSave(next);
      setError("");
      setEditing(false);
    } catch (saveError) {
      setError(saveError instanceof Error ? saveError.message : "保存失败");
    } finally {
      committingRef.current = false;
    }
  };

  if (editing) {
    return (
      <input
        className={`sunshine-inline-input${compact ? " compact" : ""}${error ? " input-error" : ""}`}
        value={draft}
        type={inputType}
        aria-label={label}
        title={error || undefined}
        autoFocus
        onClick={(event) => event.stopPropagation()}
        onDoubleClick={(event) => event.stopPropagation()}
        onChange={(event) => { setDraft(event.target.value); setError(""); }}
        onBlur={() => {
          if (skipBlurRef.current) {
            skipBlurRef.current = false;
            return;
          }
          void commit();
        }}
        onKeyDown={(event) => {
          if (event.key === "Enter") { event.preventDefault(); void commit(); }
          if (event.key === "Escape") { event.preventDefault(); cancel(); }
        }}
      />
    );
  }

  return (
    <TruncatedText
      className={`sunshine-inline-editable${compact ? " compact" : ""}`}
      title={`双击修改${label}`}
      onClick={(event) => event.stopPropagation()}
      onDoubleClick={(event) => {
        event.stopPropagation();
        setDraft(value);
        setEditing(true);
      }}
    >
      {displayValue ?? value}
    </TruncatedText>
  );
}

function HostCard({ host, selected, onOpen, onDelete, onInlineUpdate }: {
  host: SunshineHostInfo;
  selected: boolean;
  onOpen:   (el: HTMLElement) => void;
  onDelete: () => void;
  onInlineUpdate: (patch: { name?: string; host?: string; web_port?: number; username?: string; password?: string; verify_tls?: boolean }) => Promise<void>;
}) {
  const ref = useRef<HTMLElement>(null);
  const qc  = useQueryClient();
  const wakeM = useMutation({
    mutationFn: () => api.sunshineHostWake(host.id),
    onSuccess: async () => { await qc.invalidateQueries({ queryKey: queryKeys.sunshine.hosts }); }
  });

  const el = () => ref.current!;

  return (
    <article
      ref={ref as React.RefObject<HTMLElement>}
      className={`content-card service-card sunshine-host-card${selected ? " active" : ""}`}
      onClick={() => el() && onOpen(el())}
    >
      <CardInner>
        <CardRow label="名称">
          <InlineHostField
            label="名称"
            value={host.name}
            validate={(value) => value ? null : "名称不能为空"}
            onSave={(name) => onInlineUpdate({ name })}
          />
          <span title={host.connection_error ?? "Sunshine API 已连接"}>
            <StatusLed tone={host.connected ? "good" : "danger"} />
          </span>
        </CardRow>
        <CardRow label="地址">
          <div className="card-address-inline">
            <InlineHostField
              label="地址"
              value={host.host}
              validate={(value) => isValidHost(value) ? null : "请输入有效的 IPv4、IPv6 或域名"}
              onSave={(address) => onInlineUpdate({ host: address })}
            />
            <span className="sunshine-inline-separator">:</span>
            <InlineHostField
              label="端口"
              value={String(host.web_port)}
              compact
              validate={(value) => {
                const port = Number(value);
                return Number.isInteger(port) && port >= 1 && port <= 65535 ? null : "端口必须是 1–65535 的整数";
              }}
              onSave={(port) => onInlineUpdate({ web_port: Number(port) })}
            />
          </div>
        </CardRow>
        <CardRow label="账号">
          <InlineHostField label="账号" value={host.username} validate={value => value ? null : "账号不能为空"}
            onSave={username => onInlineUpdate({ username })} />
        </CardRow>
        <CardRow label="密码">
          <InlineHostField label="密码" value="" displayValue={host.password_set ? "已设置" : "未设置"} inputType="password"
            validate={value => value ? null : "密码不能为空"} onSave={password => onInlineUpdate({ password })} />
        </CardRow>
        <CardRow label="TLS">
          <button type="button" className="card-action-button" title="自签名证书可关闭验证；生产环境建议安装 CA"
            onClick={(event) => { event.stopPropagation(); void onInlineUpdate({ verify_tls: !host.verify_tls }); }}>
            {host.verify_tls ? "验证证书" : "允许自签名"}
          </button>
        </CardRow>
        <CardActions>
            <button type="button" className="card-action-button primary"
              disabled={wakeM.isPending}
              onClick={(e) => { e.stopPropagation(); wakeM.mutate(); }}>
              <Power size={12} /><span>唤醒</span>
            </button>
            <button type="button" className="card-action-button danger"
              onClick={(e) => { e.stopPropagation(); onDelete(); }}>
              <Trash2 size={12} /><span>删除</span>
            </button>
            <a href={host.web_url} target="_blank" rel="noopener noreferrer"
              className="card-action-button primary"
              onClick={(e) => e.stopPropagation()}>
              <ExternalLink size={12} /><span>打开</span>
            </a>
        </CardActions>
      </CardInner>
    </article>
  );
}

// ─── 已选主机的管理面板 ───────────────────────────────────────────────────────

function HostPanel({ host }: { host: SunshineHostInfo }) {
  const [section, setSection] = useState<HostSection>("apps");

  return (
    <div className="sunshine-host-panel">
      <div className="sunshine-panel-nav-row">
        <nav className="sunshine-subnav-inline">
          {HOST_SECTIONS.map(({ key, label, Icon }) => (
            <button
              key={key}
              type="button"
              className={section === key ? "sunshine-section-tab active" : "sunshine-section-tab"}
              onClick={() => setSection(key)}
            >
              <Icon size={18} /><strong>{label}</strong>
            </button>
          ))}
        </nav>
      </div>

      {section === "apps"    && <AppsSection   host={host} />}
      {section === "clients" && <ClientsSection host={host} />}
      {section === "pairing" && <PairingSection host={host} />}
      {section === "config"  && <ConfigSection  host={host} />}
      {section === "system"  && <SystemSection  host={host} />}
    </div>
  );
}

// ─── 应用 tab ─────────────────────────────────────────────────────────────────

type AppDraft = { name: string; cmd: string; "working-dir": string; "auto-detach": boolean; "wait-all": boolean; "exit-timeout": number; index: number };

function extractApps(data: SunshineAppsResponse | undefined): SunshineApp[] {
  if (!data) return [];
  const apps = Array.isArray(data.apps) ? data.apps : Array.isArray(data) ? data : [];
  // Sunshine 的 GET /api/apps 返回数组位置作为应用 ID，条目本身通常没有 index。
  return (apps as SunshineApp[]).map((app, index) => ({ ...app, index }));
}

function AppsSection({ host }: { host: SunshineHostInfo }) {
  const qc = useQueryClient();
  const qKey = queryKeys.sunshine.apps(host.id);
  const appsQuery = useQuery({
    queryKey: qKey,
    queryFn: () => api.sunshineApps(host.id),
    retry: false,
  });
  const [draft, setDraft] = useState<AppDraft | null>(null);

  const saveMutation = useMutation({
    mutationFn: (app: Partial<SunshineApp>) => api.sunshineSaveApp(host.id, app),
    onSuccess: async () => { setDraft(null); await qc.invalidateQueries({ queryKey: qKey }); }
  });
  const deleteMutation = useMutation({
    mutationFn: (idx: number) => api.sunshineDeleteApp(host.id, idx),
    onSuccess: async () => { await qc.invalidateQueries({ queryKey: qKey }); }
  });
  const closeMutation = useMutation({
    mutationFn: () => api.sunshineCloseApp(host.id),
    onSuccess: async () => { await qc.invalidateQueries({ queryKey: qKey }); }
  });

  const apps = extractApps(appsQuery.data);

  return (
    <section className="section-band">
      <SectionHeader icon={AppWindow} title="应用" actions={
        <div className="button-row">
          <ActionButton icon={X} label="结束会话" tone="danger" busy={closeMutation.isPending}
            onClick={() => window.confirm("结束当前应用会话？") && closeMutation.mutate()} />
          <ActionButton icon={Plus} label="新建" onClick={() =>
            setDraft({ name: "", cmd: "", "working-dir": "", "auto-detach": true, "wait-all": true, "exit-timeout": 5, index: -1 })} />
        </div>
      } />
      <MutationError mutation={saveMutation} />
      <MutationError mutation={deleteMutation} />
      {draft ? (
        <div className="sunshine-app-form">
          <div className="sunshine-form-header">
            <strong>{draft.index === -1 ? "新建应用" : "编辑应用"}</strong>
            <button className="icon-button" type="button" onClick={() => setDraft(null)}><X size={16} /></button>
          </div>
          <div className="sunshine-form-grid">
            <label className="inline-field wide"><span>名称 *</span>
              <input value={draft.name} onChange={e => setDraft(d => d && { ...d, name: e.target.value })} autoFocus /></label>
            <label className="inline-field wide"><span>启动命令</span>
              <input value={draft.cmd} onChange={e => setDraft(d => d && { ...d, cmd: e.target.value })} placeholder="留空=桌面串流" /></label>
            <label className="inline-field"><span>工作目录</span>
              <input value={draft["working-dir"]} onChange={e => setDraft(d => d && { ...d, "working-dir": e.target.value })} /></label>
            <label className="inline-field"><span>退出超时（秒）</span>
              <input type="number" value={draft["exit-timeout"]} onChange={e => setDraft(d => d && { ...d, "exit-timeout": Number(e.target.value) })} /></label>
          </div>
          <div className="button-row">
            <ActionButton icon={Check} label="保存" busy={saveMutation.isPending}
              onClick={() => draft && saveMutation.mutate(draft as unknown as Partial<SunshineApp>)} />
            <ActionButton icon={X} label="取消" onClick={() => setDraft(null)} />
          </div>
        </div>
      ) : null}
      {appsQuery.isLoading ? <LoadingBlock label="读取应用" /> : null}
      {appsQuery.error ? <InlineNotice tone="danger" text={appsQuery.error.message} /> : null}
      <div className="sunshine-app-list">
        {apps.map((app) => (
          <div
            className="sunshine-app-item"
            key={String(app.index)}
            onDoubleClick={() => setDraft({ name: app.name, cmd: (app.cmd as string) ?? "", "working-dir": (app["working-dir"] ?? "") as string,
              "auto-detach": (app["auto-detach"] ?? true) as boolean, "wait-all": (app["wait-all"] ?? true) as boolean,
              "exit-timeout": (app["exit-timeout"] ?? 5) as number, index: app.index })}
            title="双击编辑应用"
          >
            <div className="sunshine-app-info">
              <strong>{app.name}</strong>
              <span className="mono">{(app.cmd as string) || "（桌面串流）"}</span>
              <em>index: {app.index}</em>
            </div>
            <div className="button-row">
              <button className="icon-button danger" type="button" title="删除" disabled={deleteMutation.isPending}
                onClick={() => window.confirm(`删除应用 "${app.name}"？`) && deleteMutation.mutate(app.index)}>
                <Trash2 size={15} />
              </button>
            </div>
          </div>
        ))}
      </div>
    </section>
  );
}

// ─── 客户端 tab ───────────────────────────────────────────────────────────────

function extractClients(data: SunshineClientsResponse | undefined): SunshineClient[] {
  if (!data) return [];
  const all = [
    ...((data.named_certs ?? data.named ?? []) as SunshineClient[]),
    ...((data.unnamed_certs ?? data.unnamed ?? []) as SunshineClient[]),
    ...((data.certs ?? []) as SunshineClient[])
  ];
  const seen = new Set<string>();
  return all.filter(c => { if (seen.has(c.uuid)) return false; seen.add(c.uuid); return true; });
}

function ClientsSection({ host }: { host: SunshineHostInfo }) {
  const qc = useQueryClient();
  const qKey = queryKeys.sunshine.clients(host.id);
  const query = useQuery({ queryKey: qKey, queryFn: () => api.sunshineClients(host.id) });

  const unpairM = useMutation({ mutationFn: (uuid: string) => api.sunshineUnpairClient(host.id, uuid),
    onSuccess: async () => qc.invalidateQueries({ queryKey: qKey }) });
  const unpairAllM = useMutation({ mutationFn: () => api.sunshineUnpairAll(host.id),
    onSuccess: async () => qc.invalidateQueries({ queryKey: qKey }) });
  const updateM = useMutation({ mutationFn: ({ uuid, enabled }: { uuid: string; enabled: boolean }) =>
    api.sunshineUpdateClient(host.id, uuid, enabled), onSuccess: async () => qc.invalidateQueries({ queryKey: qKey }) });

  const clients = extractClients(query.data);

  return (
    <section className="section-band">
      <SectionHeader icon={Users} title="客户端" actions={
        <ActionButton icon={Unlink} label="取消所有配对" tone="danger" busy={unpairAllM.isPending}
          onClick={() => window.confirm("取消所有配对？") && unpairAllM.mutate()} />
      } />
      <MutationError mutation={unpairM} />
      <MutationError mutation={unpairAllM} />
      {query.isLoading ? <LoadingBlock label="读取客户端" /> : null}
      {query.error ? <InlineNotice tone="danger" text={query.error.message} /> : null}
      <div className="sunshine-client-list">
        {clients.map(c => (
          <div className="sunshine-client-item" key={c.uuid}>
            <div className="sunshine-client-info">
              <strong>{c.name ?? "未命名设备"}</strong>
              <span className="mono">{c.uuid}</span>
              <span className="sunshine-client-status">
                <StatusLed tone={c.enabled ? "good" : "warn"} />
                {c.enabled ? "已启用" : "已禁用"}
              </span>
            </div>
            <div className="button-row">
              <button className="icon-button" type="button" title={c.enabled ? "禁用" : "启用"}
                disabled={updateM.isPending} onClick={() => updateM.mutate({ uuid: c.uuid, enabled: !c.enabled })}>
                {c.enabled ? <ToggleRight size={18} /> : <ToggleLeft size={18} />}
              </button>
              <button className="icon-button danger" type="button" title="取消配对" disabled={unpairM.isPending}
                onClick={() => window.confirm(`取消设备 "${c.name ?? c.uuid}" 的配对？`) && unpairM.mutate(c.uuid)}>
                <Unlink size={15} />
              </button>
            </div>
          </div>
        ))}
        {!query.isLoading && !clients.length ? <p className="muted-inline">暂无已配对客户端。</p> : null}
      </div>
    </section>
  );
}

// ─── 配对 tab ─────────────────────────────────────────────────────────────────

function PairingSection({ host }: { host: SunshineHostInfo }) {
  const [pin, setPin] = useState("");
  const [deviceName, setDeviceName] = useState("");
  const pairM = useMutation({
    mutationFn: () => api.sunshinePin(host.id, pin.trim(), deviceName.trim() || "Moonlight Client"),
    onSuccess: () => { setPin(""); setDeviceName(""); }
  });

  return (
    <section className="section-band">
      <SectionHeader icon={KeyRound} title="PIN 配对" />
      <MutationError mutation={pairM} />
      {pairM.isSuccess ? <InlineNotice tone="warn" text="配对请求已提交。" /> : null}
      <div className="sunshine-pin-form">
        <label className="inline-field"><span>PIN 码 *</span>
          <input value={pin} onChange={e => setPin(e.target.value)} maxLength={8} placeholder="1234" autoFocus
            onKeyDown={e => e.key === "Enter" && pin.trim() && pairM.mutate()} /></label>
        <label className="inline-field"><span>设备名称</span>
          <input value={deviceName} onChange={e => setDeviceName(e.target.value)} placeholder="Moonlight Client" /></label>
        <div style={{ display: "flex" }}>
          <ActionButton icon={Check} label="提交配对" busy={pairM.isPending} disabled={!pin.trim()} onClick={() => pairM.mutate()} />
        </div>
      </div>
    </section>
  );
}

// ─── 配置 tab ─────────────────────────────────────────────────────────────────

function ConfigSection({ host }: { host: SunshineHostInfo }) {
  const qc = useQueryClient();
  const qKey = queryKeys.sunshine.config(host.id);
  const query = useQuery({ queryKey: qKey, queryFn: () => api.sunshineConfig(host.id) });
  const [editMode, setEditMode] = useState(false);
  const [draft, setDraft] = useState<SunshineConfig>({});

  useEffect(() => { if (query.data) setDraft(query.data); }, [query.data]);

  const saveM = useMutation({
    mutationFn: () => api.sunshineSaveConfig(host.id, draft),
    onSuccess: async () => { setEditMode(false); await qc.invalidateQueries({ queryKey: qKey }); }
  });

  const entries = Object.entries(query.data ?? {});

  return (
    <section className="section-band">
      <SectionHeader icon={Settings2} title="配置" actions={
        editMode ? (
          <div className="button-row">
            <ActionButton icon={Check} label="保存" busy={saveM.isPending} onClick={() => saveM.mutate()} />
            <ActionButton icon={X} label="取消" onClick={() => setEditMode(false)} />
          </div>
        ) : null
      } />
      {query.isLoading ? <LoadingBlock label="读取配置" /> : null}
      {query.error ? <InlineNotice tone="danger" text={query.error.message} /> : null}
      <MutationError mutation={saveM} />
      {!editMode ? (
        <div className="sunshine-config-table" onDoubleClick={() => setEditMode(true)} title="双击编辑配置">
          {entries.map(([k, v]) => (
            <div className="sunshine-config-row" key={k}>
              <span className="mono">{k}</span>
              <span className="mono sunshine-config-value">{v === null ? "null" : String(v)}</span>
            </div>
          ))}
        </div>
      ) : (
        <div className="sunshine-config-edit">
          {entries.map(([k, v]) => (
            <label className="inline-field" key={k}>
              <span className="mono">{k}</span>
              <input value={draft[k] === null ? "" : String(draft[k] ?? "")}
                onChange={e => setDraft(d => ({ ...d, [k]: e.target.value }))} />
            </label>
          ))}
        </div>
      )}
    </section>
  );
}

// ─── 系统 tab ─────────────────────────────────────────────────────────────────

function SystemSection({ host }: { host: SunshineHostInfo }) {
  const restartM = useMutation({ mutationFn: () => api.sunshineRestart(host.id) });
  const resetM = useMutation({ mutationFn: () => api.sunshineResetDisplay(host.id) });

  return (
    <section className="view-stack">
      <section className="section-band">
        <SectionHeader icon={Wrench} title="系统操作" />
        <MutationError mutation={restartM} />
        <MutationError mutation={resetM} />
        {restartM.isSuccess ? <InlineNotice tone="warn" text="重启命令已发送。" /> : null}
        {resetM.isSuccess ? <InlineNotice tone="warn" text="显示设备配置已重置。" /> : null}
        <div className="sunshine-system-actions">
          <div className="sunshine-system-card">
            <RefreshCw size={24} />
            <div><strong>重启 Sunshine</strong><p>重新加载配置，当前串流会话将中断。</p></div>
            <ActionButton icon={RefreshCw} label="立即重启" tone="danger" busy={restartM.isPending}
              onClick={() => window.confirm("确定重启 Sunshine？当前会话将中断。") && restartM.mutate()} />
          </div>
          <div className="sunshine-system-card">
            <RotateCcw size={24} />
            <div><strong>重置显示设备</strong><p>清除 Sunshine 保存的显示设备持久化配置。</p></div>
            <ActionButton icon={RotateCcw} label="重置显示" busy={resetM.isPending}
              onClick={() => window.confirm("确定重置显示设备配置？") && resetM.mutate()} />
          </div>
        </div>
      </section>
    </section>
  );
}

// ─── SunshineView 根组件 ──────────────────────────────────────────────────────

export function SunshineView({ addTrigger = 0 }: { addTrigger?: number }) {
  const qc = useQueryClient();
  const hostsQuery = useQuery({ queryKey: queryKeys.sunshine.hosts, queryFn: api.sunshineHosts, refetchInterval: 30_000 });
  const hosts = hostsQuery.data ?? [];

  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [panelPos, setPanelPos] = useState<AdjacentPanelPosition | null>(null);

  const outerRef = useRef<HTMLDivElement>(null);
  const gridRef  = useRef<HTMLDivElement>(null);
  const handledAddTriggerRef = useRef(addTrigger);

  const panelOpen = selectedId !== null;

  // 响应导航栏"+"按钮触发信号
  useEffect(() => {
    if (addTrigger <= handledAddTriggerRef.current) return;
    handledAddTriggerRef.current = addTrigger;
    createDefaultHost();
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [addTrigger]);

  const createM = useMutation({
    mutationFn: (req: SunshineHostSaveRequest) => api.sunshineCreateHost(req),
    onSuccess: async () => { await qc.invalidateQueries({ queryKey: queryKeys.sunshine.hosts }); }
  });
  const updateM = useMutation({
    mutationFn: ({ id, req }: { id: string; req: SunshineHostSaveRequest }) => api.sunshineUpdateHost(id, req),
    onSuccess: async () => { await qc.invalidateQueries({ queryKey: queryKeys.sunshine.hosts }); }
  });
  const deleteM = useMutation({
    mutationFn: (id: string) => api.sunshineDeleteHost(id),
    onSuccess: async (_, id) => {
      if (selectedId === id) { setSelectedId(null); setPanelPos(null); }
      await qc.invalidateQueries({ queryKey: queryKeys.sunshine.hosts });
    }
  });

  const hostPatchRequest = (
    host: SunshineHostInfo,
    patch: { name?: string; host?: string; web_port?: number; username?: string; password?: string; verify_tls?: boolean },
  ): SunshineHostSaveRequest => ({
    name: patch.name ?? host.name,
    host: patch.host ?? host.host,
    web_port: patch.web_port ?? host.web_port,
    broadcast_addr: host.broadcast_addr,
    username: patch.username ?? host.username,
    password: patch.password ?? null,
    verify_tls: patch.verify_tls ?? host.verify_tls,
  });

  const selectedHost = hosts.find(h => h.id === selectedId) ?? null;

  function posFromEl(el: HTMLElement): AdjacentPanelPosition | null {
    return outerRef.current && gridRef.current
      ? calculateAdjacentPanelPosition(el, outerRef.current, gridRef.current)
      : null;
  }

  // 点击卡片信息区：切换面板，同时黑色边框标记选中
  function handleHostOpen(id: string, el: HTMLElement) {
    if (selectedId === id) {
      setSelectedId(null);
      setPanelPos(null);
    } else {
      setSelectedId(id);
      setPanelPos(posFromEl(el));
    }
  }

  function createDefaultHost() {
    const usedNames = new Set(hosts.map(host => host.name));
    let index = hosts.length + 1;
    while (usedNames.has(`Sunshine ${index}`)) index += 1;
    createM.mutate({
      name: `Sunshine ${index}`,
      host: "192.168.1.2",
      web_port: 47990,
      mac_address: null,
      broadcast_addr: "255.255.255.255:9",
      username: "admin",
      password: null,
      verify_tls: true,
    });
    setSelectedId(null);
    setPanelPos(null);
  }

  // 面板的 inline style（位置 + 精确尺寸）
  const { width: defW, height: defH } = defaultAdjacentPanelSize(gridRef.current);
  const panelStyle: React.CSSProperties = panelPos
    ? panelPos.side === "right"
      ? { top: panelPos.top, left: panelPos.left, width: panelPos.width, height: panelPos.height }
      : { top: panelPos.top, right: panelPos.right, width: panelPos.width, height: panelPos.height }
    : { top: 0, right: 0, width: defW, height: defH };

  return (
    <section className="view-stack">
      <section className="section-band sunshine-new-section">
        {/* mutation 错误提示 */}
        <MutationError mutation={createM} />
        <MutationError mutation={updateM} />
        <MutationError mutation={deleteM} />

        {hostsQuery.error ? <InlineNotice tone="danger" text={hostsQuery.error.message} /> : null}
        {hostsQuery.isLoading ? <LoadingBlock label="读取主机" /> : null}
        {!hostsQuery.isLoading && !hosts.length ? <p className="muted-inline">暂无主机，点击 + 新建</p> : null}

        {/* 主机卡网格 + 动态定位的相邻覆盖面板 */}
        <div className="instance-list-title"><ContentTitle icon={Boxes} title="实例" /></div>
        <div className="adjacent-cards-outer sunshine-cards-outer" ref={outerRef}>
          <div className="content-grid sunshine-host-grid" ref={gridRef}>
            {hosts.map(h => (
              <HostCard
                key={h.id}
                host={h}
                selected={selectedId === h.id}
                onOpen={(el) => handleHostOpen(h.id, el)}
                onInlineUpdate={(patch) => updateM.mutateAsync({ id: h.id, req: hostPatchRequest(h, patch) }).then(() => undefined)}
                onDelete={() => {
                  if (window.confirm(`确定删除主机 "${h.name}"？`)) deleteM.mutate(h.id);
                }}
              />
            ))}
          </div>

          {panelOpen && (
            <div className="adjacent-panel sunshine-adj-panel" style={panelStyle}>
              {selectedHost ? (
                <HostPanel host={selectedHost} />
              ) : null}
            </div>
          )}
        </div>
      </section>
    </section>
  );
}
