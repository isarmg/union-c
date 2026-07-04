import { useState } from "react";
import { BookOpenText, Gamepad2, Terminal } from "lucide-react";
import { useQueries, useQuery } from "@tanstack/react-query";
import { api } from "../api";
import { queryKeys } from "../query-keys";
import type { LogsResponse } from "../types";
import { InlineNotice, LoadingBlock, LogViewer } from "../components/ui";

type LogTab = "ram" | "sunshine" | "blog";

const TAB_ITEMS: Array<{ key: LogTab; label: string; Icon: React.ComponentType<{ size?: number }> }> = [
  { key: "ram",     label: "ram",     Icon: Terminal     },
  { key: "sunshine", label: "Sunshine", Icon: Gamepad2     },
  { key: "blog",     label: "blog",   Icon: BookOpenText },
];

export function LogsView() {
  const [tab, setTab] = useState<LogTab>("ram");

  const hostsQuery = useQuery({
    queryKey: queryKeys.sunshine.hosts,
    queryFn: api.sunshineHosts,
    refetchInterval: 30_000
  });
  const hosts = hostsQuery.data ?? [];

  return (
    <section className="view-stack logs-view-stack">
      <section className="section-band logs-band">
        <div className="logs-tab-bar">
          <span className="logs-tab-title">日志</span>
          <div className="logs-tab-buttons">
            {TAB_ITEMS.map(({ key, label, Icon }) => (
              <button
                key={key}
                type="button"
                className={`logs-tab-btn${tab === key ? " active" : ""}`}
                onClick={() => setTab(key)}
              >
                <Icon size={14} />
                <span>{label}</span>
              </button>
            ))}
          </div>
        </div>

        {tab === "ram"     && <RamLogPanel />}
        {tab === "sunshine" && <SunshineLogPanel hosts={hosts} hostsLoading={hostsQuery.isLoading} />}
        {tab === "blog"     && <BlogLogPanel />}
      </section>
    </section>
  );
}

function RamLogPanel() {
  const q = useQuery({
    queryKey: queryKeys.logs.ram,
    queryFn: () => api.ramLogs(),
    refetchInterval: 10_000
  });
  return <LogViewer logs={q.data} loading={q.isLoading} />;
}

function SunshineLogPanel({
  hosts,
  hostsLoading
}: {
  hosts: { id: string; name: string }[];
  hostsLoading: boolean;
}) {
  const results = useQueries({
    queries: hosts.map(h => ({
      queryKey: queryKeys.logs.sunshine(h.id),
      queryFn: () => api.sunshineHostLogs(h.id),
      refetchInterval: 15_000
    }))
  });

  if (hostsLoading) return <LoadingBlock label="读取主机列表" />;
  if (!hosts.length) return <InlineNotice tone="warn" text="暂无已配置的 Sunshine 主机" />;

  const loading = results.some(r => r.isLoading);

  // 合并所有主机的日志行；多主机时在每段前插入分隔标题
  const merged: LogsResponse = {
    path: "sunshine (all hosts)",
    lines: results.flatMap((r, i) => {
      const lines = r.data?.lines ?? [];
      if (hosts.length === 1) return lines;
      return [`▶ ${hosts[i].name}`, ...lines, ""];
    })
  };

  return <LogViewer logs={merged} loading={loading} />;
}

function BlogLogPanel() {
  const q = useQuery({
    queryKey: queryKeys.logs.blog,
    queryFn: () => api.blogLogs(),
    refetchInterval: 10_000
  });
  return <LogViewer logs={q.data} loading={q.isLoading} />;
}
