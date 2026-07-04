// 管理后台根组件。
//
// 这个文件只负责全局框架：登录、导航栏、主题切换、实时事件流。
// 各功能页面在 src/views/ 目录下独立维护，共享组件在 src/components/ 下。

import { FormEvent, useEffect, useState } from "react";
import {
  BookOpenText,
  Check,
  Cpu,
  Gamepad2,
  LayoutDashboard,
  Moon,
  Plus,
  Power,
  RefreshCw,
  Settings,
  Sun,
  Terminal,
  UploadCloud,
  X
} from "lucide-react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "./api";
import {
  useEventStream,
  useMetricHistory,
  useServicesFromEvents,
} from "./hooks";
import { queryKeys } from "./query-keys";
import { OverviewView } from "./views/OverviewView";
import { RamView } from "./views/RamView";
import { ProxmoxView } from "./views/ProxmoxView";
import { SunshineView } from "./views/SunshineView";
import { LogsView } from "./views/LogsView";
import { BlogView } from "./views/BlogView";
import { SettingsView } from "./views/SettingsView";
import { InlineNotice, LoadingBlock } from "./components/ui";

// ─── 导航配置 ─────────────────────────────────────────────────────────────────

const navItems = [
  { key: "overview",  label: "总览",      icon: LayoutDashboard },
  { key: "ram",      label: "ram",      icon: UploadCloud },
  { key: "proxmox",  label: "Proxmox",   icon: Cpu },
  { key: "sunshine",  label: "Sunshine",  icon: Gamepad2 },
  { key: "logs",      label: "日志",      icon: Terminal },
  { key: "blog",      label: "blog",    icon: BookOpenText },
  { key: "settings",  label: "设置",      icon: Settings }
] as const satisfies ReadonlyArray<{
  key: string;
  label: string;
  icon: React.ComponentType<{ size?: number }>;
}>;

type ViewKey = (typeof navItems)[number]["key"];

// ─── 主题 ─────────────────────────────────────────────────────────────────────

type Theme = "light" | "dark";
const THEME_STORAGE_KEY = "union-theme";

function getInitialTheme(): Theme {
  try {
    const stored = window.localStorage.getItem(THEME_STORAGE_KEY);
    if (stored === "light" || stored === "dark") return stored;
  } catch { /* ignore */ }
  return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
}

// ─── 主界面 ───────────────────────────────────────────────────────────────────

function AuthedApp({ onLogout }: { onLogout: () => Promise<void> }) {
  const [view, setView] = useState<ViewKey>("overview");
  const [theme, setTheme] = useState<Theme>(getInitialTheme);
  const [addTrigger, setAddTrigger] = useState(0);
  const queryClient = useQueryClient();
  const databaseQuery = useQuery({
    queryKey: queryKeys.settings.database,
    queryFn: api.databaseConfig,
    refetchInterval: 10_000
  });
  const databaseConnected = databaseQuery.data?.connected === true;
  const eventStream = useEventStream(databaseConnected);

  useEffect(() => {
    try { window.localStorage.setItem(THEME_STORAGE_KEY, theme); } catch { /* ignore */ }
    document.documentElement.style.colorScheme = theme;
  }, [theme]);

  const healthQuery = useQuery({ queryKey: queryKeys.health, queryFn: api.health, refetchInterval: 15_000 });
  const servicesQuery = useQuery({
    queryKey: queryKeys.services,
    queryFn: api.services,
    refetchInterval: 10_000,
    enabled: databaseConnected
  });
  const resourcesQuery = useQuery({ queryKey: queryKeys.systemResources, queryFn: api.systemResources, refetchInterval: 20_000 });
  const metricHistory = useMetricHistory(resourcesQuery.data);

  const services = useServicesFromEvents(servicesQuery.data, eventStream.lastEvent);
  const unhealthy = services.filter((s) => !s.healthy);
  const ramService = services.find((s) => s.name === "ram");
  const databaseRequired = view === "ram" || view === "proxmox" || view === "sunshine" || view === "logs" || view === "blog";

  return (
    <div className="app-shell" data-theme={theme}>
      <aside className="sidebar">
        <nav className="nav-list" aria-label="管理台导航">
          {navItems.map(({ key, label, icon: Icon }) => (
            <button
              key={key}
              className={view === key ? "nav-item active" : "nav-item"}
              type="button"
              onClick={() => { setView(key); setAddTrigger(0); }}
              title={label}
            >
              <Icon size={18} />
              <span>{label}</span>
            </button>
          ))}
        </nav>

        <div className="sidebar-footer">
          {databaseConnected && (view === "ram" || view === "sunshine" || view === "proxmox") && (
            <button className="icon-button" type="button" title={view === "ram" ? "新建账号" : "新建实例"}
              onClick={() => setAddTrigger(n => n + 1)}>
              <Plus size={18} />
            </button>
          )}
          <button className="icon-button" type="button" onClick={() => { void queryClient.invalidateQueries(); }} title="刷新全部数据">
            <RefreshCw size={18} />
          </button>
          <div className="connection-pill">
            {eventStream.connected
              ? <Check size={14} className="conn-icon connected" />
              : <X size={14} className="conn-icon disconnected" />}
          </div>
          <button className="icon-button" type="button" onClick={() => setTheme(theme === "light" ? "dark" : "light")} title="切换主题">
            {theme === "light" ? <Moon size={18} /> : <Sun size={18} />}
          </button>
          <button className="icon-button" type="button" onClick={onLogout} title="退出登录">
            <Power size={18} />
          </button>
        </div>
      </aside>

      <main className="main">
        {eventStream.error ? <InlineNotice tone="warn" text={eventStream.error} /> : null}
        {healthQuery.error ? <InlineNotice tone="danger" text={healthQuery.error.message} /> : null}

        {view === "overview"  && <OverviewView services={services} unhealthyCount={unhealthy.length} resources={resourcesQuery.data} history={metricHistory} loading={servicesQuery.isLoading || resourcesQuery.isLoading} />}
        {databaseRequired && !databaseConnected && (
          <section className="view-stack">
            <InlineNotice tone="warn" text="数据库未连接，请先前往“设置”配置数据库。" />
          </section>
        )}
        {view === "ram" && databaseConnected && <RamView service={ramService} addTrigger={addTrigger} />}
        {view === "proxmox" && databaseConnected && <ProxmoxView addTrigger={addTrigger} />}
        {view === "sunshine" && databaseConnected && <SunshineView addTrigger={addTrigger} />}
        {view === "logs" && databaseConnected && <LogsView />}
        {view === "blog" && databaseConnected && <BlogView />}
        {view === "settings"  && <SettingsView />}
      </main>
    </div>
  );
}

// ─── 根组件 ───────────────────────────────────────────────────────────────────

function LoginScreen({ onLogin }: { onLogin: (username: string, password: string) => Promise<void> }) {
  const [username, setUsername] = useState("admin");
  const [password, setPassword] = useState("");
  const [error, setError] = useState("");
  const [submitting, setSubmitting] = useState(false);

  const submit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setError("");
    setSubmitting(true);
    try {
      await onLogin(username.trim(), password);
    } catch (loginError) {
      setError(loginError instanceof Error ? loginError.message : "登录失败");
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <main className="app-shell login-screen">
      <form className="login-card" onSubmit={submit}>
        <div><h1>Union</h1><p>登录管理中心</p></div>
        <label>
          <span>用户名</span>
          <input value={username} onChange={(event) => setUsername(event.target.value)} autoComplete="username" autoFocus required />
        </label>
        <label>
          <span>密码</span>
          <input type="password" value={password} onChange={(event) => setPassword(event.target.value)} autoComplete="current-password" required />
        </label>
        {error ? <InlineNotice tone="danger" text={error} /> : null}
        <button className="primary-button" type="submit" disabled={submitting || !username.trim() || !password}>
          {submitting ? "正在登录…" : "登录"}
        </button>
      </form>
    </main>
  );
}

export function App() {
  const queryClient = useQueryClient();
  const meQuery = useQuery({
    queryKey: queryKeys.auth.me,
    queryFn: api.authenticate,
    retry: false
  });

  useEffect(() => {
    const expire = () => { void queryClient.invalidateQueries({ queryKey: queryKeys.auth.me }); };
    window.addEventListener("union:auth-expired", expire);
    return () => window.removeEventListener("union:auth-expired", expire);
  }, [queryClient]);

  const handleLogout = async () => {
    try { await api.logout(); } catch { /* ignore */ }
    await queryClient.resetQueries({ queryKey: queryKeys.auth.me });
  };

  const handleLogin = async (username: string, password: string) => {
    const result = await api.login(username, password);
    queryClient.setQueryData(queryKeys.auth.me, { username: result.username });
  };

  if (meQuery.isPending) {
    return <main className="app-shell login-screen"><LoadingBlock label="正在验证会话" /></main>;
  }
  if (meQuery.isError) {
    return <LoginScreen onLogin={handleLogin} />;
  }
  return <AuthedApp onLogout={handleLogout} />;
}
