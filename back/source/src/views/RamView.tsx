import { useEffect, useRef, useState } from "react";
import { Boxes, ExternalLink, KeyRound, Lock, Trash2 } from "lucide-react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "../api";
import { queryKeys } from "../query-keys";
import type { RamAuthPath, RamAuthResponse, RamAuthRuleInput, RamAuthUpdateRequest, ServiceStatus } from "../types";
import { CardActions, CardInner, CardRow, ContentTitle, InlineNotice, LoadingBlock, MutationError, StatusLed, TruncatedText } from "../components/ui";

// ─── 类型 ────────────────────────────────────────────────────────────────────

type AuthDraftRule = {
  id: string;
  username: string;
  password: string;
  passwordSet: boolean;
  permission: "ro" | "rw";
  pathsText: string;
};

type AuthDraftState = {
  managementUsername: string;
  managementPassword: string;
  managementConfigured: boolean;
  rules: AuthDraftRule[];
};

// ─── 辅助函数 ─────────────────────────────────────────────────────────────────

function toDraftRule(rule: RamAuthResponse["rules"][number]): AuthDraftRule {
  const firstWritable = rule.paths.find((p) => p.permission === "rw");
  return {
    id: crypto.randomUUID(),
    username: rule.username ?? "",
    password: "",
    passwordSet: rule.password_set,
    permission: firstWritable ? "rw" : "ro",
    pathsText: formatPathsText(rule.paths)
  };
}

function emptyDraftRule(): AuthDraftRule {
  return { id: crypto.randomUUID(), username: "", password: "", passwordSet: false, permission: "ro", pathsText: "/public" };
}

function toAuthInput(rule: AuthDraftRule): RamAuthRuleInput {
  return {
    username: rule.username.trim() || null,
    password: rule.password,
    paths: parseDraftPaths(rule.pathsText, rule.permission)
  };
}

function toAuthUpdateRequest(draft: AuthDraftState): RamAuthUpdateRequest {
  return {
    management_username: draft.managementUsername.trim() || null,
    management_password: draft.managementPassword,
    rules: draft.rules.map(toAuthInput),
  };
}

function formatPathsText(paths: RamAuthPath[]): string {
  return paths.map((p) => (p.permission === "rw" ? `${p.path}:rw` : p.path)).join("\n");
}

function parseDraftPaths(text: string, fallback: "ro" | "rw"): RamAuthPath[] {
  return text
    .split(/[\n,]+/)
    .map((item) => item.trim())
    .filter(Boolean)
    .map((item) => {
      const match = item.match(/^(.*):(ro|rw)$/i);
      const path = match ? match[1] : item;
      const permission = (match ? match[2].toLowerCase() : fallback) as "ro" | "rw";
      return { path: path.startsWith("/") ? path : `/${path}`, permission };
    });
}

// ─── 卡片字段原位编辑 ─────────────────────────────────────────────────────────

type EditableFieldKind = "text" | "password" | "select";

function InlineEditableValue({
  label,
  value,
  displayValue,
  onCommit,
  kind = "text",
  placeholder,
  className = "",
  options = [],
}: {
  label: string;
  value: string;
  displayValue: string;
  onCommit: (value: string) => void;
  kind?: EditableFieldKind;
  placeholder?: string;
  className?: string;
  options?: Array<{ value: string; label: string }>;
}) {
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(value);

  const startEditing = () => {
    setDraft(value);
    setEditing(true);
  };

  const commit = () => {
    if (draft !== value) onCommit(draft);
    setEditing(false);
  };

  const cancel = () => {
    setDraft(value);
    setEditing(false);
  };

  const handleKeyDown = (
    event: React.KeyboardEvent<HTMLInputElement | HTMLSelectElement>
  ) => {
    if (event.key === "Escape") {
      event.preventDefault();
      cancel();
      return;
    }
    if (event.key === "Enter") {
      event.preventDefault();
      commit();
    }
  };

  if (editing) {
    if (kind === "select") {
      return (
        <select
          className="ram-inline-editor select"
          value={draft}
          aria-label={label}
          autoFocus
          onClick={(event) => event.stopPropagation()}
          onChange={(event) => setDraft(event.target.value)}
          onBlur={commit}
          onKeyDown={handleKeyDown}
        >
          {options.map((option) => (
            <option key={option.value} value={option.value}>{option.label}</option>
          ))}
        </select>
      );
    }

    return (
      <form className="ram-inline-editor-form" onClick={(event) => event.stopPropagation()} onSubmit={(event) => { event.preventDefault(); commit(); }}>
        <input
          className="ram-inline-editor"
          type={kind}
          value={draft}
          placeholder={placeholder}
          aria-label={label}
          autoFocus
          onChange={(event) => setDraft(event.target.value)}
          onBlur={commit}
          onKeyDown={handleKeyDown}
        />
      </form>
    );
  }

  return (
    <span
      className={`ram-editable-value${className ? ` ${className}` : ""}`}
      role="button"
      tabIndex={0}
      title={`双击修改${label}`}
      aria-label={`${label}：${displayValue}，双击修改`}
      onClick={(event) => event.stopPropagation()}
      onDoubleClick={startEditing}
      onKeyDown={(event) => {
        if (event.key === "Enter" || event.key === " ") {
          event.preventDefault();
          startEditing();
        }
      }}
    >
      {displayValue}
    </span>
  );
}

// ─── 管理员账号卡片 ───────────────────────────────────────────────────────────

function RamMgmtCard({
  username,
  password,
  configured,
  onUpdate,
}: {
  username: string;
  password: string;
  configured: boolean;
  onUpdate: (username: string, password: string) => void;
}) {
  return (
    <article className="content-card service-card ram-account-card">
      <CardInner>
        <CardRow label="账号">
          <InlineEditableValue
            label="账号名称"
            value={username}
            displayValue={username || "admin"}
            placeholder="admin"
            className="grow"
            onCommit={(value) => onUpdate(value, password)}
          />
          <span className="ram-perm-badge mgmt">管理员</span>
        </CardRow>
        <CardRow label="密码">
          <InlineEditableValue
            label="账号密码"
            value={password}
            displayValue={password ? "待保存" : configured ? "已设置" : "未设置"}
            kind="password"
            placeholder="留空保留旧密码"
            className="muted"
            onCommit={(value) => onUpdate(username, value)}
          />
        </CardRow>
        <CardRow label="权限">
          <TruncatedText muted>管理界面</TruncatedText>
        </CardRow>
        <CardActions>
          <button type="button" className="card-action-button" disabled title="管理账号不可删除">
            <Lock size={12} /><span>不可删除</span>
          </button>
        </CardActions>
      </CardInner>
    </article>
  );
}

// ─── 普通账号卡片 ─────────────────────────────────────────────────────────────

function RamAccountCard({
  rule,
  onUpdate,
  onDelete,
}: {
  rule: AuthDraftRule;
  onUpdate: (patch: Partial<AuthDraftRule>) => void;
  onDelete: () => void;
}) {
  const singleLinePaths = rule.pathsText.replace(/\n+/g, ", ");

  return (
    <article className="content-card service-card ram-account-card">
      <CardInner>
        <CardRow label="账号">
          <InlineEditableValue
            label="账号名称"
            value={rule.username}
            displayValue={rule.username || "（未命名）"}
            placeholder="alice"
            className="grow"
            onCommit={(username) => onUpdate({ username })}
          />
        </CardRow>
        <CardRow label="密码">
          <InlineEditableValue
            label="账号密码"
            value={rule.password}
            displayValue={rule.password ? "待保存" : rule.passwordSet ? "已设置" : "未设置"}
            kind="password"
            placeholder="留空保留旧密码"
            className="muted"
            onCommit={(password) => onUpdate({ password })}
          />
        </CardRow>
        <CardRow label="权限">
          <InlineEditableValue
            label="访问权限"
            value={rule.permission}
            displayValue={rule.permission === "rw" ? "读写" : "只读"}
            kind="select"
            className="muted"
            options={[{ value: "ro", label: "只读" }, { value: "rw", label: "读写" }]}
            onCommit={(permission) => onUpdate({ permission: permission as "ro" | "rw" })}
          />
        </CardRow>
        <CardRow label="路径">
          <InlineEditableValue
            label="工作目录"
            value={singleLinePaths}
            displayValue={singleLinePaths || "—"}
            placeholder="/public, /media:rw"
            className="ram-paths-preview"
            onCommit={(pathsText) => onUpdate({ pathsText })}
          />
        </CardRow>
        <CardActions>
          <button type="button" className="card-action-button danger" onClick={onDelete}>
            <Trash2 size={12} /><span>删除</span>
          </button>
        </CardActions>
      </CardInner>
    </article>
  );
}

// ─── ram 权限编辑器 ──────────────────────────────────────────────────────────

function RamAuthManager({ addTrigger, instanceId }: { addTrigger: number; instanceId: string }) {
  const queryClient = useQueryClient();
  const authQuery = useQuery({ queryKey: queryKeys.ram.instanceAuth(instanceId), queryFn: () => api.ramInstanceAuth(instanceId) });
  const [draft, setDraft] = useState<AuthDraftState>({
    managementUsername: "",
    managementPassword: "",
    managementConfigured: false,
    rules: [],
  });
  const draftRef = useRef(draft);
  const saveRevisionRef = useRef(0);
  const hasLocalChangesRef = useRef(false);
  const [saveNotice, setSaveNotice] = useState<{
    tone: "warn" | "danger";
    text: string;
  } | null>(null);
  // 组件挂载时把当前计数作为基线，避免刚选中实例就把之前用于创建实例的“+”误判为新增用户。
  const handledAddTriggerRef = useRef(addTrigger);

  useEffect(() => {
    if (!authQuery.data || hasLocalChangesRef.current) return;
    const nextDraft: AuthDraftState = {
      managementUsername: authQuery.data.management_username ?? "admin",
      managementPassword: "",
      managementConfigured: authQuery.data.management_auth_configured,
      rules: authQuery.data.rules.filter(r => !r.anonymous).map(toDraftRule),
    };
    draftRef.current = nextDraft;
    setDraft(nextDraft);
  }, [authQuery.data]);

  const saveMutation = useMutation({
    scope: { id: "ram-auth-autosave" },
    mutationFn: ({ request }: { revision: number; request: RamAuthUpdateRequest }) =>
      api.updateRamInstanceAuth(instanceId, request),
    onSuccess: async (data, { revision }) => {
      // 同一账号区的保存串行执行；只有最后一次保存完成后才刷新草稿，
      // 避免较早请求的响应覆盖用户随后完成的修改。
      if (revision !== saveRevisionRef.current) return;
      const savedDraft: AuthDraftState = {
        managementUsername: data.management_username ?? "admin",
        managementPassword: "",
        managementConfigured: data.management_auth_configured,
        rules: data.rules.filter(rule => !rule.anonymous).map(toDraftRule),
      };
      draftRef.current = savedDraft;
      setDraft(savedDraft);
      hasLocalChangesRef.current = false;
      setSaveNotice({
        tone: data.message.includes("失败") || data.message.includes("未能") ? "danger" : "warn",
        text: data.message,
      });
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: queryKeys.ram.instanceAuth(instanceId) }),
      ]);
    },
  });

  const updateDraft = (
    updater: (current: AuthDraftState) => AuthDraftState,
    persist = true,
  ) => {
    const nextDraft = updater(draftRef.current);
    draftRef.current = nextDraft;
    setDraft(nextDraft);
    hasLocalChangesRef.current = true;
    setSaveNotice(null);
    const revision = ++saveRevisionRef.current;

    if (persist && authQuery.data) {
      saveMutation.mutate({
        revision,
        request: toAuthUpdateRequest(nextDraft),
      });
    }
  };

  const addAccount = () => {
    updateDraft(
      (current) => ({ ...current, rules: [...current.rules, emptyDraftRule()] }),
      false,
    );
  };

  useEffect(() => {
    if (!authQuery.data || addTrigger <= handledAddTriggerRef.current) return;
    handledAddTriggerRef.current = addTrigger;
    addAccount();
  // addAccount 只依赖当前草稿 ref；触发源仅为顶部导航按钮计数。
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [addTrigger, authQuery.data]);

  return (
    <div className="auth-manager">
      <div className="subsection-head">
        <ContentTitle icon={KeyRound} title="账号与权限" />
      </div>

      {authQuery.isLoading ? <LoadingBlock label="正在读取 ram 账号" /> : null}
      {authQuery.error ? <InlineNotice tone="danger" text={authQuery.error.message} /> : null}
      <MutationError mutation={saveMutation} />
      {saveNotice ? <InlineNotice tone={saveNotice.tone} text={saveNotice.text} /> : null}

      <div className="content-grid ram-account-grid">
        <RamMgmtCard
          username={draft.managementUsername}
          password={draft.managementPassword}
          configured={draft.managementConfigured}
          onUpdate={(managementUsername, managementPassword) => {
            updateDraft((current) => ({
              ...current,
              managementUsername: managementUsername.trim() || "admin",
              managementPassword,
            }));
          }}
        />
        {draft.rules.map(rule => (
          <RamAccountCard
            key={rule.id}
            rule={rule}
            onUpdate={(patch) => {
              updateDraft((current) => ({
                ...current,
                rules: current.rules.map((item) =>
                  item.id === rule.id ? { ...item, ...patch } : item
                ),
              }));
            }}
            onDelete={() => {
              updateDraft((current) => ({
                ...current,
                rules: current.rules.filter((item) => item.id !== rule.id),
              }));
            }}
          />
        ))}
      </div>
    </div>
  );
}

// ─── RamView ─────────────────────────────────────────────────────────────────

export function RamView({
  addTrigger = 0,
}: {
  service?: ServiceStatus;
  addTrigger?: number;
}) {
  const queryClient = useQueryClient();
  const instancesQuery = useQuery({ queryKey: queryKeys.ram.instances, queryFn: api.ramInstances, refetchInterval: 5000 });
  const instances = instancesQuery.data ?? [];
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const handledTrigger = useRef(0);
  const selected = instances.find(instance => instance.id === selectedId) ?? null;
  const createMutation = useMutation({
    mutationFn: api.createRamInstance,
    onSuccess: async () => { await queryClient.invalidateQueries({ queryKey: queryKeys.ram.instances }); }
  });
  const updateMutation = useMutation({
    mutationFn: ({ instance, patch }: {
      instance: (typeof instances)[number];
      patch: Partial<Pick<(typeof instances)[number], "name" | "host" | "port" | "use_tls" | "verify_tls">>;
    }) => api.updateRamInstance(instance.id, {
      name: patch.name ?? instance.name,
      host: patch.host ?? instance.host,
      port: patch.port ?? instance.port,
      use_tls: patch.use_tls ?? instance.use_tls,
      verify_tls: patch.verify_tls ?? instance.verify_tls,
    }),
    onSuccess: async () => { await queryClient.invalidateQueries({ queryKey: queryKeys.ram.instances }); }
  });
  const deleteMutation = useMutation({ mutationFn: api.deleteRamInstance, onSuccess: async () => { setSelectedId(null); await queryClient.invalidateQueries({ queryKey: queryKeys.ram.instances }); } });

  useEffect(() => {
    if (addTrigger <= handledTrigger.current) return;
    handledTrigger.current = addTrigger;
    if (selectedId) return;
    createMutation.mutate({ name: `RAM ${instances.length + 1}`, host: "192.168.1.2", port: 5000, use_tls: false, verify_tls: true });
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [addTrigger]);

  return (
    <section className="view-stack">
      <section className="section-band">
        {instancesQuery.isLoading ? <LoadingBlock label="正在读取 RAM 实例" /> : null}
        {instancesQuery.error ? <InlineNotice tone="danger" text={instancesQuery.error.message} /> : null}
        <MutationError mutation={createMutation} />
        <MutationError mutation={updateMutation} />
        <MutationError mutation={deleteMutation} />
        <div className="instance-list-title"><ContentTitle icon={Boxes} title="实例" /></div>
        <div className="content-grid ram-instance-grid">
          {instances.map(instance => <article key={instance.id} className={`content-card service-card ram-status-card${selectedId === instance.id ? " active" : ""}`} onClick={() => setSelectedId(selectedId === instance.id ? null : instance.id)}>
            <CardInner>
              <CardRow label="服务">
                <InlineEditableValue
                  label="实例名称"
                  value={instance.name}
                  displayValue={instance.name}
                  className="grow"
                  onCommit={(name) => updateMutation.mutate({ instance, patch: { name } })}
                />
                <StatusLed tone={instance.reachable ? "good" : "danger"} />
              </CardRow>
              <CardRow label="账号"><TruncatedText muted>{instance.management_username ?? "未配置"}</TruncatedText></CardRow>
              <CardRow label="密码"><TruncatedText muted>{instance.management_password_set ? "已设置" : "未设置"}</TruncatedText></CardRow>
              <CardRow label="地址">
                <div className="card-address-inline">
                  <InlineEditableValue
                    label="远程主机"
                    value={instance.host}
                    displayValue={instance.host}
                    className="muted"
                    onCommit={(host) => updateMutation.mutate({ instance, patch: { host } })}
                  />
                  <span>:</span>
                  <InlineEditableValue
                    label="端口"
                    value={String(instance.port)}
                    displayValue={String(instance.port)}
                    className="muted"
                    onCommit={(value) => {
                      const port = Number(value);
                      if (Number.isInteger(port) && port >= 1 && port <= 65535) {
                        updateMutation.mutate({ instance, patch: { port } });
                      }
                    }}
                  />
                </div>
              </CardRow>
              <CardRow label="协议">
                <InlineEditableValue
                  label="访问协议"
                  value={instance.use_tls ? "https" : "http"}
                  displayValue={instance.use_tls ? "HTTPS" : "HTTP"}
                  kind="select"
                  options={[{ value: "http", label: "HTTP" }, { value: "https", label: "HTTPS" }]}
                  onCommit={(value) => updateMutation.mutate({ instance, patch: { use_tls: value === "https" } })}
                />
              </CardRow>
              {instance.use_tls && <CardRow label="证书">
                <InlineEditableValue
                  label="TLS 证书校验"
                  value={instance.verify_tls ? "verify" : "skip"}
                  displayValue={instance.verify_tls ? "校验" : "忽略"}
                  kind="select"
                  options={[{ value: "verify", label: "校验" }, { value: "skip", label: "忽略（不安全）" }]}
                  onCommit={(value) => updateMutation.mutate({ instance, patch: { verify_tls: value === "verify" } })}
                />
              </CardRow>}
              <CardActions>
                  <a href={instance.url} target="_blank" rel="noopener noreferrer" className="card-action-button" title="使用卡片中的管理员账号密码登录远程 RAM" onClick={(e) => e.stopPropagation()}>
                    <ExternalLink size={12} /><span>管理员登录</span>
                  </a>
                  <button type="button" className="card-action-button danger" onClick={(event) => { event.stopPropagation(); if(confirm(`删除 ${instance.name}？`)) deleteMutation.mutate(instance.id); }}><Trash2 size={12}/><span>删除</span></button>
              </CardActions>
            </CardInner>
          </article>)}
        </div>
      </section>

      {selected ? <section className="section-band ram-auth-section"><RamAuthManager instanceId={selected.id} addTrigger={addTrigger} /></section> : null}
    </section>
  );
}
