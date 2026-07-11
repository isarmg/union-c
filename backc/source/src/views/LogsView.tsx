import { useQueries, useQuery } from "@tanstack/react-query";
import { api } from "../api";
import { queryKeys } from "../query-keys";
import type { LogsResponse } from "../types";
import { InlineNotice, LoadingBlock, LogViewer } from "../components/ui";

export function LogsView() {
  const hostsQuery = useQuery({
    queryKey: queryKeys.sunshine.hosts,
    queryFn: api.sunshineHosts,
    refetchInterval: 30_000,
  });
  const hosts = hostsQuery.data ?? [];
  const results = useQueries({
    queries: hosts.map((host) => ({
      queryKey: queryKeys.logs.sunshine(host.id),
      queryFn: () => api.sunshineHostLogs(host.id),
      refetchInterval: 15_000,
    })),
  });

  if (hostsQuery.isLoading) return <LoadingBlock label="读取主机列表" />;
  if (!hosts.length) return <InlineNotice tone="warn" text="暂无已配置的 Sunshine 主机" />;

  const merged: LogsResponse = {
    path: "sunshine (all hosts)",
    lines: results.flatMap((result, index) => {
      const lines = result.data?.lines ?? [];
      return hosts.length === 1 ? lines : [`▶ ${hosts[index].name}`, ...lines, ""];
    }),
  };
  return <LogViewer logs={merged} loading={results.some((result) => result.isLoading)} />;
}
