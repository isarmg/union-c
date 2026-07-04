import { useEffect, useState } from "react";
import { Check, Edit2, KeyRound, Loader2, Save, Settings, X } from "lucide-react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "../api";
import { queryKeys } from "../query-keys";
import { CardActions, CardInner, CardRow, InlineNotice, MutationError, SectionHeader, SettingItem, TruncatedText } from "../components/ui";

// ─── 修改密码侧面板 ───────────────────────────────────────────────────────────

function ChangePasswordPanel({ onClose }: { onClose: () => void }) {
  const [currentPw, setCurrentPw] = useState("");
  const [newPw, setNewPw] = useState("");
  const [confirmPw, setConfirmPw] = useState("");

  const changeMutation = useMutation({
    mutationFn: () => api.changePassword(currentPw, newPw),
    onSuccess: async () => {
      setCurrentPw("");
      setNewPw("");
      setConfirmPw("");
      try {
        await api.logout();
      } catch {
        // ignore
      }
      window.dispatchEvent(new Event("union:auth-expired"));
    }
  });

  const passwordMismatch = confirmPw.length > 0 && newPw !== confirmPw;
  const canSubmit =
    currentPw.length > 0 && newPw.length >= 12 && newPw === confirmPw && !changeMutation.isPending;

  return (
    <div className="settings-side-panel">
      <div className="settings-panel-header">
        <strong>修改密码</strong>
        <button className="icon-button" type="button" onClick={onClose}><X size={16} /></button>
      </div>
      <form
        className="account-form"
        onSubmit={(e) => { e.preventDefault(); if (newPw !== confirmPw) return; changeMutation.mutate(); }}
      >
        <label className="inline-field">
          <span>当前密码</span>
          <input type="password" value={currentPw} onChange={(e) => setCurrentPw(e.target.value)}
            autoComplete="current-password" placeholder="输入当前密码" autoFocus />
        </label>
        <label className="inline-field">
          <span>新密码</span>
          <input type="password" value={newPw} onChange={(e) => setNewPw(e.target.value)}
            autoComplete="new-password" placeholder="至少 12 个字符" />
        </label>
        <label className="inline-field">
          <span>确认新密码</span>
          <input type="password" value={confirmPw} onChange={(e) => setConfirmPw(e.target.value)}
            autoComplete="new-password" placeholder="再次输入新密码"
            className={passwordMismatch ? "input-error" : ""} />
        </label>
        {passwordMismatch && <InlineNotice tone="warn" text="两次输入的新密码不一致" />}
        <MutationError mutation={changeMutation} />
        {changeMutation.isSuccess && (
          <p className="account-success"><Check size={15} /> 密码已修改成功</p>
        )}
        <div className="blog-panel-actions">
          <button type="submit" className="action-button primary" disabled={!canSubmit}>
            {changeMutation.isPending ? <Loader2 size={16} className="spin" /> : <Save size={16} />}
            <span>修改密码</span>
          </button>
        </div>
      </form>
    </div>
  );
}

// ─── 账号管理区域 ─────────────────────────────────────────────────────────────

function AccountSection() {
  const meQuery = useQuery({ queryKey: queryKeys.auth.me, queryFn: api.me });
  const [panelOpen, setPanelOpen] = useState(false);

  return (
    <section className="section-band">
      <SectionHeader icon={KeyRound} title="账号管理" />
      <div className="content-grid settings-grid">
        <div className="content-card setting-item">
          <CardInner>
            <CardRow label="用户">
              <TruncatedText>{meQuery.data?.username ?? "—"}</TruncatedText>
            </CardRow>
            <CardActions>
              <button
                type="button"
                className="card-action-button primary"
                onClick={() => setPanelOpen(v => !v)}
              >
                <Edit2 size={12} /><span>修改密码</span>
              </button>
            </CardActions>
          </CardInner>
        </div>
      </div>
      {panelOpen && <ChangePasswordPanel onClose={() => setPanelOpen(false)} />}
    </section>
  );
}

type DatabaseFields = {
  scheme: "postgres" | "postgresql";
  host: string;
  port: string;
  username: string;
  password: string;
  database: string;
  search: string;
  hash: string;
};

const emptyDatabaseFields: DatabaseFields = {
  scheme: "postgresql",
  host: "",
  port: "5432",
  username: "",
  password: "",
  database: "",
  search: "",
  hash: ""
};

function decodeUrlPart(value: string) {
  try {
    return decodeURIComponent(value);
  } catch {
    return value;
  }
}

function parseDatabaseUrl(value: string): DatabaseFields {
  if (!value.trim()) return { ...emptyDatabaseFields };
  try {
    const parsed = new URL(value);
    return {
      scheme: parsed.protocol === "postgres:" ? "postgres" : "postgresql",
      host: parsed.hostname,
      port: parsed.port || "5432",
      username: decodeUrlPart(parsed.username),
      password: decodeUrlPart(parsed.password),
      database: decodeUrlPart(parsed.pathname.replace(/^\/+/, "")),
      search: parsed.search,
      hash: parsed.hash
    };
  } catch {
    return { ...emptyDatabaseFields };
  }
}

function buildDatabaseUrl(fields: DatabaseFields) {
  const host = fields.host.trim();
  const port = Number(fields.port);
  if (!host || /[\s/@?#]/.test(host) || !/^\d+$/.test(fields.port) || port < 1 || port > 65535) return "";
  if (host.includes(":") && !(host.startsWith("[") && host.endsWith("]"))) return "";

  try {
    const hostProbe = new URL(`http://${host}`);
    if (hostProbe.port || hostProbe.pathname !== "/" || hostProbe.search || hostProbe.hash) return "";

    const parsed = new URL(`${fields.scheme}://localhost`);
    parsed.hostname = hostProbe.hostname;
    parsed.port = fields.port;
    parsed.username = fields.username;
    parsed.password = fields.password;
    parsed.pathname = fields.database ? `/${encodeURIComponent(fields.database)}` : "";
    parsed.search = fields.search;
    parsed.hash = fields.hash;
    return parsed.toString();
  } catch {
    return "";
  }
}

function DatabaseSection() {
  const queryClient = useQueryClient();
  const databaseQuery = useQuery({ queryKey: queryKeys.settings.database, queryFn: api.databaseConfig });
  const [fields, setFields] = useState<DatabaseFields>(emptyDatabaseFields);
  const databaseUrl = buildDatabaseUrl(fields);

  useEffect(() => {
    if (databaseQuery.data) setFields(parseDatabaseUrl(databaseQuery.data.database_url));
  }, [databaseQuery.data]);

  const saveMutation = useMutation({
    mutationFn: () => api.saveDatabaseConfig(databaseUrl),
    onSuccess: (data) => {
      setFields(parseDatabaseUrl(data.database_url));
      queryClient.setQueryData(queryKeys.settings.database, data);
    }
  });
  const canSave = Boolean(databaseUrl && fields.password !== "********" && !saveMutation.isPending);
  const updateField = (field: keyof Pick<DatabaseFields, "host" | "port" | "username" | "password" | "database">) =>
    (event: React.ChangeEvent<HTMLInputElement>) => setFields(current => ({ ...current, [field]: event.target.value }));

  return (
    <section className="section-band">
      <SectionHeader icon={Settings} title="数据库连接" description="连接测试和初始化成功后写入本地私有配置，并立即切换当前连接。" />
      <div className="content-grid settings-grid">
        <form
          className="content-card database-card"
          aria-label="数据库连接"
          onSubmit={(event) => {
            event.preventDefault();
            if (canSave) saveMutation.mutate();
          }}
        >
          <CardInner>
            <CardRow label="主机">
              <input
                type="text"
                autoComplete="off"
                value={fields.host}
                className="database-field-input"
                aria-label="主机"
                onChange={updateField("host")}
                placeholder="127.0.0.1"
                spellCheck={false}
                required
              />
            </CardRow>
            <CardRow label="端口">
              <input
                type="number"
                min="1"
                max="65535"
                value={fields.port}
                className="database-field-input"
                aria-label="端口"
                onChange={updateField("port")}
                placeholder="5432"
                required
              />
            </CardRow>
            <CardRow label="用户名">
              <input
                type="text"
                autoComplete="username"
                value={fields.username}
                className="database-field-input"
                aria-label="用户名"
                onChange={updateField("username")}
              />
            </CardRow>
            <CardRow label="密码">
              <input
                type="password"
                autoComplete="new-password"
                value={fields.password}
                className="database-field-input"
                aria-label="密码"
                onChange={updateField("password")}
              />
            </CardRow>
            <CardRow label="数据库名">
              <input
                type="text"
                autoComplete="off"
                value={fields.database}
                className="database-field-input"
                aria-label="数据库名"
                onChange={updateField("database")}
              />
            </CardRow>
            <CardActions>
              <button
                type="submit"
                className="card-action-button primary"
                disabled={!canSave}
              >
                {saveMutation.isPending ? <Loader2 size={12} className="spin" /> : <Save size={12} />}
                <span>测试并保存</span>
              </button>
            </CardActions>
          </CardInner>
        </form>
      </div>
      <div className="database-notices">
        <MutationError mutation={saveMutation} />
        {databaseQuery.isError && <InlineNotice tone="danger" text={databaseQuery.error.message} />}
        {databaseQuery.data && <InlineNotice tone="warn"
          text={databaseQuery.data.connected ? "数据库已连接" : databaseQuery.data.configured ? "配置已保存，当前进程尚未连接" : "尚未配置数据库"} />}
        {saveMutation.isSuccess && <InlineNotice tone="warn" text="连接测试通过，配置已保存并立即生效。" />}
      </div>
    </section>
  );
}

export function SettingsView() {
  return (
    <section className="view-stack">
      <AccountSection />
      <DatabaseSection />
      <section className="section-band">
        <SectionHeader
          icon={Settings}
          title="运行配置"
          description="运行路径由后端配置决定。"
        />
        <div className="content-grid settings-grid">
          <SettingItem label="配置" value="data/union-config.json" />
          <SettingItem label="文件" value="data/ram/files" />
          <SettingItem label="内容" value="data/blog/content" />
          <SettingItem label="资源" value="data/blog/files" />
          <SettingItem label="日光" value="data/sunshine" />
          <SettingItem label="月光" value="data/moonlight" />
        </div>
      </section>
    </section>
  );
}
