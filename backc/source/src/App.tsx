import { FormEvent, useEffect, useState } from "react";
import {
  Check, Gamepad2, LayoutDashboard, Lock, LogIn, MonitorCog, Moon, Plus, Power,
  RefreshCw, Settings, Sun, Terminal, User, X,
} from "lucide-react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "./api";
import { useEventStream, useMetricHistory, useServicesFromEvents } from "./hooks";
import { queryKeys } from "./query-keys";
import { OverviewView } from "./views/OverviewView";
import { SunshineView } from "./views/SunshineView";
import { LogsView } from "./views/LogsView";
import { SettingsView } from "./views/SettingsView";
import { MonitoringView } from "./views/MonitoringView";
import { CardActions, CardInner, CardRow, InlineNotice, LoadingBlock } from "./components/ui";

const navItems = [
  { key: "overview", label: "总览", icon: LayoutDashboard },
  { key: "monitoring", label: "主机", icon: MonitorCog },
  { key: "sunshine", label: "Sunshine", icon: Gamepad2 },
  { key: "logs", label: "日志", icon: Terminal },
  { key: "settings", label: "设置", icon: Settings },
] as const satisfies ReadonlyArray<{
  key: string; label: string; icon: React.ComponentType<{ size?: number }>;
}>;
type ViewKey = (typeof navItems)[number]["key"];
type Theme = "light" | "dark";
const THEME_STORAGE_KEY = "unionc-theme";

function getInitialTheme(): Theme {
  try {
    const stored = window.localStorage.getItem(THEME_STORAGE_KEY);
    if (stored === "light" || stored === "dark") return stored;
  } catch { /* local storage may be unavailable */ }
  return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
}

function AuthedApp({ onLogout }: { onLogout: () => Promise<void> }) {
  const [view, setView] = useState<ViewKey>("overview");
  const [theme, setTheme] = useState<Theme>(getInitialTheme);
  const [addTrigger, setAddTrigger] = useState(0);
  const queryClient = useQueryClient();
  const databaseQuery = useQuery({
    queryKey: queryKeys.settings.database,
    queryFn: api.databaseConfig,
    refetchInterval: 10_000,
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
    enabled: databaseConnected,
  });
  const resourcesQuery = useQuery({
    queryKey: queryKeys.systemResources,
    queryFn: api.systemResources,
    refetchInterval: 20_000,
  });
  const history = useMetricHistory(resourcesQuery.data);
  const services = useServicesFromEvents(servicesQuery.data, eventStream.lastEvent);
  const unhealthy = services.filter((service) => !service.healthy);
  const databaseRequired = view === "monitoring" || view === "sunshine" || view === "logs";

  return (
    <div className="app-shell" data-theme={theme}>
      <aside className="sidebar">
        <nav className="nav-list" aria-label="UnionC 导航">
          {navItems.map(({ key, label, icon: Icon }) => (
            <button
              key={key}
              className={view === key ? "nav-item active" : "nav-item"}
              type="button"
              onClick={() => { setView(key); setAddTrigger(0); }}
              title={label}
            >
              <Icon size={18} /><span>{label}</span>
            </button>
          ))}
        </nav>
        <div className="sidebar-footer">
          {databaseConnected && view === "sunshine" && (
            <button className="icon-button" type="button" title="新建实例" onClick={() => setAddTrigger((value) => value + 1)}>
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
          <button className="icon-button" type="button" onClick={onLogout} title="退出登录"><Power size={18} /></button>
        </div>
      </aside>
      <main className="main">
        {eventStream.error && <InlineNotice tone="warn" text={eventStream.error} />}
        {healthQuery.error && <InlineNotice tone="danger" text={healthQuery.error.message} />}
        {view === "overview" && (
          <OverviewView
            services={services}
            unhealthyCount={unhealthy.length}
            resources={resourcesQuery.data}
            history={history}
            loading={servicesQuery.isLoading || resourcesQuery.isLoading}
          />
        )}
        {view === "monitoring" && databaseConnected && <MonitoringView />}
        {databaseRequired && !databaseConnected && (
          <section className="view-stack"><InlineNotice tone="warn" text="数据库未连接，请先前往“设置”配置数据库。" /></section>
        )}
        {view === "sunshine" && databaseConnected && <SunshineView addTrigger={addTrigger} />}
        {view === "logs" && databaseConnected && <LogsView />}
        {view === "settings" && <SettingsView />}
      </main>
    </div>
  );
}

function LoginScreen({ onLogin }: { onLogin: (username: string, password: string) => Promise<void> }) {
  const [username, setUsername] = useState("admin");
  const [password, setPassword] = useState("");
  const [error, setError] = useState("");
  const [submitting, setSubmitting] = useState(false);
  const submit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault(); setError(""); setSubmitting(true);
    try { await onLogin(username.trim(), password); }
    catch (loginError) { setError(loginError instanceof Error ? loginError.message : "登录失败"); }
    finally { setSubmitting(false); }
  };
  return (
    <main className="app-shell login-screen">
      <form className="content-card login-card" onSubmit={submit} aria-label="登录 UnionC 管理中心">
        <CardInner>
          <CardRow label={<><span className="login-label-icon"><User /></span>账号</>} />
          <CardRow label=""><input className="login-input" aria-label="账号" value={username} onChange={(event) => setUsername(event.target.value)} autoComplete="username" autoFocus required /></CardRow>
          <CardRow label={<><span className="login-label-icon"><Lock /></span>密码</>} />
          <CardRow label=""><input className="login-input" aria-label="密码" type="password" value={password} onChange={(event) => setPassword(event.target.value)} autoComplete="current-password" required /></CardRow>
          <CardRow label="" row={5}>{error ? <span className="login-error" role="alert">{error}</span> : null}</CardRow>
          <CardActions label={<><span className="login-label-icon"><LogIn /></span>操作</>}>
            <button className="card-action-button primary" type="submit" disabled={submitting || !username.trim() || !password}>
              <span>{submitting ? "正在登录…" : "登录"}</span>
            </button>
          </CardActions>
        </CardInner>
      </form>
    </main>
  );
}

export function App() {
  const queryClient = useQueryClient();
  const meQuery = useQuery({ queryKey: queryKeys.auth.me, queryFn: api.authenticate, retry: false });
  useEffect(() => {
    const expire = () => { void queryClient.invalidateQueries({ queryKey: queryKeys.auth.me }); };
    window.addEventListener("unionc:auth-expired", expire);
    return () => window.removeEventListener("unionc:auth-expired", expire);
  }, [queryClient]);
  const handleLogout = async () => {
    try { await api.logout(); } catch { /* ignore */ }
    await queryClient.resetQueries({ queryKey: queryKeys.auth.me });
  };
  const handleLogin = async (username: string, password: string) => {
    const result = await api.login(username, password);
    queryClient.setQueryData(queryKeys.auth.me, { username: result.username });
  };
  if (meQuery.isPending) return <main className="app-shell login-screen"><LoadingBlock label="正在验证会话" /></main>;
  if (meQuery.isError) return <LoginScreen onLogin={handleLogin} />;
  return <AuthedApp onLogout={handleLogout} />;
}
