// 共享 UI 基础组件。
//
// 所有视图页面都可能用到的通用展示和交互元素集中放在这里，
// 避免在每个 View 文件里重复写相同的结构和 CSS 类。

import { useEffect, useRef, useState } from "react";
import { BellDot, Loader2 } from "lucide-react";
import type { LogsResponse, ServiceStatus } from "../types";
import { serviceLabel } from "../utils";

// ─── 跑马灯文字 ───────────────────────────────────────────────────────────────
//
// 文字宽度超出容器时自动切换为水平循环滚动；未超出则保持静态单行显示。
// 通过隐藏的量测节点检测溢出，避免直接测量动画元素宽度造成误判。

export function TickerText({ children }: { children: string }) {
  const outerRef = useRef<HTMLSpanElement>(null);
  const measureRef = useRef<HTMLSpanElement>(null);
  const [isOverflow, setIsOverflow] = useState(false);

  useEffect(() => {
    const outer = outerRef.current;
    const measure = measureRef.current;
    if (!outer || !measure) return;

    const check = () => {
      setIsOverflow(measure.scrollWidth > outer.clientWidth + 1);
    };

    const raf = requestAnimationFrame(check);
    const observer = new ResizeObserver(check);
    observer.observe(outer);
    return () => { cancelAnimationFrame(raf); observer.disconnect(); };
  }, [children]);

  return (
    <span ref={outerRef} className="ticker-outer">
      {/* 隐藏量测节点，始终渲染单份文字用于宽度检测 */}
      <span ref={measureRef} className="ticker-measure" aria-hidden="true">{children}</span>
      {isOverflow ? (
        <span className="ticker-animate">
          <span className="ticker-unit">{children}</span>
          <span className="ticker-unit" aria-hidden="true">{children}</span>
        </span>
      ) : (
        <span className="ticker-static">{children}</span>
      )}
    </span>
  );
}

// ─── 内容块通用布局原语 ───────────────────────────────────────────────────────

/** 等边距内容框：使用内容块短边计算四边间距，内部等分六行 */
export function CardInner({ children }: { children: React.ReactNode }) {
  return <div className="card-inner">{children}</div>;
}

/** 标准内容行：左列标题 + 2ch 间距 + 右列内容 */
export function CardRow({
  label,
  children,
  span,
  row,
  chart
}: {
  label: React.ReactNode;
  children?: React.ReactNode;
  span?: number;
  row?: number;
  chart?: boolean;
}) {
  const gridRow = row ? String(row) : span ? `span ${span}` : undefined;
  return (
    <div
      className={`card-row${chart ? " card-row-chart" : ""}`}
      style={gridRow ? { gridRow } : undefined}
    >
      <span className="card-row-label">{label}</span>
      <div className="card-row-content">{children}</div>
    </div>
  );
}

/** 内容块中的单行截断文字。 */
export function TruncatedText({
  children,
  muted = false,
  grow = false,
  className = "",
  ...spanProps
}: React.HTMLAttributes<HTMLSpanElement> & {
  muted?: boolean;
  grow?: boolean;
}) {
  const classes = [
    "truncate-text",
    muted ? "muted-text" : "",
    grow ? "grow" : "",
    className
  ].filter(Boolean).join(" ");
  return <span {...spanProps} className={classes}>{children}</span>;
}

/** 固定在内容块第六行的通用操作区。 */
export function CardActions({
  children,
  label = "操作",
  className = "",
  onClick
}: {
  children: React.ReactNode;
  label?: React.ReactNode;
  className?: string;
  onClick?: React.MouseEventHandler<HTMLDivElement>;
}) {
  return (
    <CardRow label={label} row={6}>
      <div className={`card-actions${className ? ` ${className}` : ""}`} onClick={onClick}>{children}</div>
    </CardRow>
  );
}

// ─── 服务卡片 ─────────────────────────────────────────────────────────────────

function serviceledTone(service: ServiceStatus): "good" | "warn" | "danger" {
  if (service.healthy) return "good";
  // stopped / not-configured → 红色（错误）；其他（unknown / checking）→ 黄色（繁忙）
  const stopped = ["stopped", "not-configured"].includes(service.runtime_state);
  return stopped ? "danger" : "warn";
}

export function ServiceCard({
  service,
  compact = false
}: {
  service: ServiceStatus;
  compact?: boolean;
}) {
  return (
    <article className={compact ? "content-card service-card compact" : "content-card service-card"}>
      <CardInner>
        <CardRow label="名称">
          <TruncatedText grow>
            <TickerText>{serviceLabel(service.name)}</TickerText>
          </TruncatedText>
          <StatusLed tone={serviceledTone(service)} />
        </CardRow>
        <CardRow label="状态">
          <TickerText>{service.runtime_state}</TickerText>
        </CardRow>
        <CardRow label="PID">{service.pid ?? "-"}</CardRow>
        <CardRow label="地址">
          {service.address ? (
            <TruncatedText>
              <TickerText>{service.address}</TickerText>
            </TruncatedText>
          ) : "-"}
        </CardRow>
        <CardRow label="消息">
          {!compact && service.message ? (
            <TruncatedText muted>{service.message}</TruncatedText>
          ) : null}
        </CardRow>
      </CardInner>
    </article>
  );
}

// ─── 指标卡片 ─────────────────────────────────────────────────────────────────

export function Sparkline({
  data,
  color = "var(--primary)",
  maxValue
}: {
  data: number[];
  color?: string;
  /** 指定 Y 轴最大值以固定纵坐标范围（如 CPU/内存传 100）；
   *  不传时自适应到数据最大值，适合网络等量纲不固定的指标。 */
  maxValue?: number;
}) {
  if (data.length < 2) return null;
  const W = 200;
  const H = 56;
  const verticalPad = 2;
  const max = Math.max(maxValue ?? Math.max(...data), 0.001);
  // 横向端点贴齐 SVG 边界；纵向仍留出空间，避免峰值线被裁切。
  const tx = (i: number) => (i / (data.length - 1)) * W;
  const ty = (v: number) => H - verticalPad - (v / max) * (H - verticalPad * 2);

  let path = `M ${tx(0)} ${ty(data[0])}`;
  for (let i = 1; i < data.length; i++) {
    const cx = (tx(i - 1) + tx(i)) / 2;
    path += ` C ${cx} ${ty(data[i - 1])} ${cx} ${ty(data[i])} ${tx(i)} ${ty(data[i])}`;
  }
  const fillPath = `${path} L ${tx(data.length - 1)} ${H} L ${tx(0)} ${H} Z`;

  return (
    <svg
      viewBox={`0 0 ${W} ${H}`}
      preserveAspectRatio="none"
      width="100%"
      height="100%"
      style={{ display: "block", position: "absolute", inset: 0 }}
      aria-hidden="true"
    >
      <path d={fillPath} style={{ fill: color, fillOpacity: 0.12 }} />
      <path
        d={path}
        style={{ fill: "none", stroke: color, strokeWidth: 2 }}
        vectorEffect="non-scaling-stroke"
      />
    </svg>
  );
}

export function Metric({
  label,
  value,
  detail,
  tone,
  sparkData,
  sparkColor,
  sparkMax
}: {
  label: string;
  value: string;
  detail?: string;
  tone: "good" | "warn" | "neutral";
  sparkData?: number[];
  sparkColor?: string;
  sparkMax?: number;
}) {
  const hasChart = sparkData && sparkData.length >= 2;
  return (
    <article className={`content-card metric ${tone}`}>
      <CardInner>
        <CardRow label={label}>
          <strong className="metric-row-value">{value}</strong>
        </CardRow>
        <CardRow label="详情">
          {detail ? <span className="metric-row-detail">{detail}</span> : null}
        </CardRow>
        {hasChart ? (
          <div className="card-spark-row metric-chart-slot">
            <Sparkline data={sparkData} color={sparkColor ?? "var(--primary)"} maxValue={sparkMax} />
          </div>
        ) : null}
      </CardInner>
    </article>
  );
}

// ─── 通用按钮 ─────────────────────────────────────────────────────────────────

export function ActionButton({
  icon: Icon,
  label,
  busy,
  disabled,
  tone = "primary",
  onClick
}: {
  icon: React.ComponentType<{ size?: number }>;
  label: string;
  busy?: boolean;
  disabled?: boolean;
  tone?: "primary" | "danger";
  onClick: () => void;
}) {
  return (
    <button
      className={`action-button ${tone}`}
      type="button"
      onClick={onClick}
      disabled={busy || disabled}
      title={label}
    >
      {busy ? <Loader2 className="spin" size={16} /> : <Icon size={16} />}
      <span>{label}</span>
    </button>
  );
}

// ─── 区域标题 ─────────────────────────────────────────────────────────────────

export function SectionHeader({
  icon: Icon,
  title,
  description,
  actions
}: {
  icon: React.ComponentType<{ size?: number }>;
  title: string;
  description?: string;
  actions?: React.ReactNode;
}) {
  return (
    <div className="section-header">
      <ContentTitle icon={Icon} title={title} description={description} />
      {actions ? <div className="section-actions">{actions}</div> : null}
    </div>
  );
}

/** 内容块网格统一使用“图标 + 名称”，图标和名称高度均为 18px。 */
export function ContentTitle({ icon: Icon, title, description }: {
  icon: React.ComponentType<{ size?: number }>;
  title: string;
  description?: string;
}) {
  return (
    <div className="section-title">
      <Icon size={18} />
      <div>
        <h2>{title}</h2>
        {description ? <p>{description}</p> : null}
      </div>
    </div>
  );
}

// ─── 状态标记 ─────────────────────────────────────────────────────────────────

/** 圆形 LED 状态指示灯：green=正常，yellow=繁忙/检测中，red=错误/离线 */
export function StatusLed({ tone }: { tone: "good" | "warn" | "danger" }) {
  return <span className={`status-led ${tone}`} />;
}

export function StatusBadge({
  tone,
  icon: Icon,
  label
}: {
  tone: "good" | "warn";
  icon: React.ComponentType<{ size?: number }>;
  label: string;
}) {
  return (
    <div className={`status-badge ${tone}`}>
      <Icon size={16} />
      <span>{label}</span>
    </div>
  );
}

// ─── 通知与错误 ───────────────────────────────────────────────────────────────

export function InlineNotice({
  tone,
  text
}: {
  tone: "warn" | "danger";
  text: string;
}) {
  return (
    <div className={`inline-notice ${tone}`}>
      <BellDot size={16} />
      <span>{text}</span>
    </div>
  );
}

export function MutationError({
  mutation
}: {
  mutation: { error: Error | null; isError: boolean };
}) {
  if (!mutation.isError || !mutation.error) {
    return null;
  }
  return <InlineNotice tone="danger" text={mutation.error.message} />;
}

// ─── 进度条 ───────────────────────────────────────────────────────────────────

export function ProgressBar({ value }: { value: number }) {
  return (
    <div className="progress" aria-label={`使用率 ${value.toFixed(0)}%`}>
      <span style={{ width: `${Math.max(4, Math.min(value, 100))}%` }} />
    </div>
  );
}

// ─── 加载占位 ─────────────────────────────────────────────────────────────────

export function LoadingBlock({ label }: { label: string }) {
  return (
    <div className="loading-block">
      <Loader2 className="spin" size={18} />
      <span>{label}</span>
    </div>
  );
}

// ─── 分段控制器 ───────────────────────────────────────────────────────────────

export function SegmentedControl({
  value,
  options,
  onChange
}: {
  value: string;
  options: Array<{ value: string; label: string }>;
  onChange: (value: string) => void;
}) {
  return (
    <div className="segmented-control">
      {options.map((option) => (
        <button
          key={option.value}
          className={value === option.value ? "active" : ""}
          type="button"
          onClick={() => onChange(option.value)}
        >
          {option.label}
        </button>
      ))}
    </div>
  );
}

// ─── 日志查看器 ───────────────────────────────────────────────────────────────

export function LogViewer({
  logs,
  loading
}: {
  logs: LogsResponse | undefined;
  loading: boolean;
}) {
  return (
    <div className="log-viewer">
      <div className="log-toolbar">
        <span>{logs?.path ?? "等待日志文件"}</span>
        <span>{logs?.lines.length ?? 0} 行</span>
      </div>
      <pre>
        {loading
          ? "loading..."
          : logs?.lines.length
            ? logs.lines.join("\n")
            : "暂无日志"}
      </pre>
    </div>
  );
}

// ─── 设置项只读行 ─────────────────────────────────────────────────────────────

export function SettingItem({ label, value }: { label: string; value: string }) {
  return (
    <div className="content-card setting-item">
      <CardInner>
        <CardRow label={label}>
          <TruncatedText>
            <TickerText>{value}</TickerText>
          </TruncatedText>
        </CardRow>
      </CardInner>
    </div>
  );
}
