/*
 * React 自定义 Hook。
 *
 * Hook 是 React 里复用状态逻辑的函数。这个文件把“实时事件流”“从事件流合并服务状态”
 * 和“输入防抖”抽出来，页面组件就不用重复写这些细节。
 */
import { useEffect, useMemo, useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { api } from "./api";
import { queryKeys } from "./query-keys";
import type { EventPayload, ServiceStatus, SystemResources } from "./types";

export interface MetricHistory {
  cpu: number[];
  memory: number[];
  network: number[];
  disk: number[];
}

export function useMetricHistory(
  data: SystemResources | undefined,
  maxPoints = 180
): MetricHistory {
  const [history, setHistory] = useState<MetricHistory>({
    cpu: [],
    memory: [],
    network: [],
    disk: [],
  });

  useEffect(() => {
    if (!data) return;
    const memPercent =
      data.memory_total_kib > 0
        ? (data.memory_used_kib / data.memory_total_kib) * 100
        : 0;
    setHistory((prev) => ({
      cpu: [...prev.cpu.slice(-(maxPoints - 1)), data.cpu_usage_percent],
      memory: [...prev.memory.slice(-(maxPoints - 1)), memPercent],
      network: [
        ...prev.network.slice(-(maxPoints - 1)),
        data.network.total_bytes_per_second,
      ],
      disk: [
        ...prev.disk.slice(-(maxPoints - 1)),
        data.disk_throughput?.total_bytes_per_second ?? 0,
      ],
    }));
  }, [data, maxPoints]);

  return history;
}

/**
 * 订阅后端的 Server-Sent Events 实时事件。
 *
 * SSE 可以理解成“后端主动向浏览器推消息”的轻量连接：
 * - 浏览器用 EventSource 连接 /api/events。
 * - 后端有服务状态变化时推送 status 事件。
 * - 页面收到事件后更新本地状态，用户就能看到更及时的运行状态。
 */
export function useEventStream(enabled = true) {
  const [lastEvent, setLastEvent] = useState<EventPayload | null>(null);
  const [connected, setConnected] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!enabled) {
      setLastEvent(null);
      setConnected(false);
      setError(null);
      return;
    }
    // EventSource 不支持自定义请求头，因此先向后端申请一个 60 秒有效的短效
    // ticket，再把 ticket 放入 URL 参数。这样服务器日志里不会出现长效 session
    // token，泄露风险大幅降低。
    // 断线后手动重连（每 5 秒一次），重连时重新申请 ticket。
    let source: EventSource | null = null;
    let reconnectTimer: ReturnType<typeof setTimeout> | null = null;
    let cancelled = false;

    function connect() {
      api.issueSseTicket()
        .then(({ ticket }) => {
          if (cancelled) return;
          source = new EventSource(`/api/events?ticket=${encodeURIComponent(ticket)}`);

          source.addEventListener("open", () => {
            setConnected(true);
            setError(null);
          });

          source.addEventListener("status", (event) => {
            try {
              setLastEvent(JSON.parse(event.data) as EventPayload);
            } catch {
              setError("实时状态解析失败");
            }
          });

          source.addEventListener("error", () => {
            // SSE 断开时页面不会崩溃；组件仍可使用 React Query 的普通轮询数据作为兜底。
            setConnected(false);
            source?.close();
            source = null;
            if (!cancelled) {
              reconnectTimer = setTimeout(connect, 5000);
            }
          });
        })
        .catch(() => {
          if (!cancelled) {
            setError("无法建立实时连接");
            reconnectTimer = setTimeout(connect, 5000);
          }
        });
    }

    connect();

    return () => {
      cancelled = true;
      source?.close();
      if (reconnectTimer !== null) clearTimeout(reconnectTimer);
    };
  }, [enabled]);

  return { connected, error, lastEvent };
}

/**
 * 在“事件流状态”和“普通接口状态”之间做选择。
 *
 * 如果 SSE 已经推来了服务列表，优先展示最新事件；如果还没推来，就使用 /api/services
 * 轮询得到的 fallback。useMemo 可以避免每次渲染都重新计算数组。
 */
export function useServicesFromEvents(
  fallback: ServiceStatus[] | undefined,
  eventPayload: EventPayload | null
) {
  return useMemo(() => {
    if (eventPayload?.services?.length) {
      return eventPayload.services;
    }
    return fallback ?? [];
  }, [eventPayload, fallback]);
}

/**
 * 控制类动作的 mutation 封装。
 *
 * 启动、停止、构建等动作成功后统一刷新 services 查询。
 * 把这个逻辑集中在这里，各个 View 不需要重复写 onSettled。
 */
export function useActionMutation(
  mutationFn: () => Promise<unknown>
) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn,
    onSettled: async () => {
      await queryClient.invalidateQueries({ queryKey: queryKeys.services });
    }
  });
}

/**
 * 输入防抖。
 *
 * 例如用户在搜索框里连续输入 5 个字，如果每按一次键都请求后端，会产生很多无意义请求。
 * 防抖的做法是：等用户停下来 delayMs 毫秒后，才把最新值交给真正的查询逻辑。
 */
export function useDebouncedValue<T>(value: T, delayMs: number) {
  const [debouncedValue, setDebouncedValue] = useState(value);

  useEffect(() => {
    const timeoutId = window.setTimeout(() => {
      setDebouncedValue(value);
    }, delayMs);

    return () => {
      window.clearTimeout(timeoutId);
    };
  }, [delayMs, value]);

  return debouncedValue;
}
