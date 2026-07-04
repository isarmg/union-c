import { useEffect, useRef, useState } from "react";
import type { PveHostInfo } from "../types";
import { CardActions, CardInner, CardRow, StatusLed, TruncatedText } from "./ui";

type HostPatch = { name?: string; host?: string; port?: number; token_id?: string; token_secret?: string; verify_tls?: boolean };

function InlinePveField({ value, label, display, compact, secret, validate, onSave }: {
  value: string; label: string; display?: string; compact?: boolean; secret?: boolean;
  validate: (value: string) => string | null; onSave: (value: string) => Promise<void>;
}) {
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(value);
  const [error, setError] = useState("");
  const saving = useRef(false);
  useEffect(() => { if (!editing) setDraft(value); }, [editing, value]);
  const commit = async () => {
    if (saving.current) return;
    const next = draft.trim(); const problem = validate(next);
    if (problem) { setError(problem); return; }
    if (next === value && !secret) { setEditing(false); return; }
    saving.current = true;
    try { await onSave(next); setEditing(false); setError(""); }
    catch (err) { setError(err instanceof Error ? err.message : "保存失败"); }
    finally { saving.current = false; }
  };
  if (editing) return <input autoFocus type={secret ? "password" : "text"} value={draft}
    className={`pve-inline-input${compact ? " compact" : ""}${error ? " input-error" : ""}`}
    title={error || undefined} onClick={e => e.stopPropagation()} onDoubleClick={e => e.stopPropagation()}
    onChange={e => { setDraft(e.target.value); setError(""); }} onBlur={() => void commit()}
    onKeyDown={e => { if (e.key === "Enter") void commit(); if (e.key === "Escape") setEditing(false); }} />;
  return <TruncatedText className={`pve-inline-editable${compact ? " compact" : ""}`}
    title={`双击修改${label}`} onClick={e => e.stopPropagation()}
    onDoubleClick={e => { e.stopPropagation(); setDraft(value); setEditing(true); }}>{display ?? value}</TruncatedText>;
}

export function PveHostCard({ host, selected, onSelect, onDelete, onInlineUpdate, validateHost }: {
  host: PveHostInfo; selected: boolean; onSelect: (element: HTMLElement) => void; onDelete: () => void;
  onInlineUpdate: (patch: HostPatch) => Promise<void>; validateHost: (value: string) => boolean;
}) {
  return <article className={`content-card pve-host-card${selected ? " active" : ""}`} onClick={event => onSelect(event.currentTarget)}>
    <CardInner>
      <CardRow label="名称"><InlinePveField value={host.name} label="名称" validate={v => v ? null : "名称不能为空"} onSave={name => onInlineUpdate({name})}/><span title={host.connection_error ?? "PVE API 已连接"}><StatusLed tone={host.connected ? "good" : "danger"}/></span></CardRow>
      <CardRow label="地址"><div className="card-address-inline"><InlinePveField value={host.host} label="地址" validate={v => validateHost(v) ? null : "请输入 IPv4、IPv6 或域名"} onSave={value => onInlineUpdate({host:value})}/><span>:</span><InlinePveField compact value={String(host.port)} label="端口" validate={v => Number.isInteger(+v) && +v > 0 && +v <= 65535 ? null : "端口无效"} onSave={value => onInlineUpdate({port:+value})}/></div></CardRow>
      <CardRow label="令牌"><InlinePveField value={host.token_id} label="API Token ID" validate={v => v ? null : "API Token ID 不能为空"} onSave={token_id => onInlineUpdate({token_id})}/></CardRow>
      <CardRow label="密钥"><InlinePveField secret value="" display={host.token_secret_set ? "已设置" : "未设置"} label="API Token Secret" validate={v => v ? null : "API Token Secret 不能为空"} onSave={token_secret => onInlineUpdate({token_secret})}/></CardRow>
      <CardRow label="TLS">
        <button type="button" className="card-action-button" title="PVE 自签名证书可关闭验证；生产环境建议安装 PVE CA"
          onClick={event => { event.stopPropagation(); void onInlineUpdate({verify_tls: !host.verify_tls}); }}>
          {host.verify_tls ? "验证证书" : "允许自签名"}
        </button>
      </CardRow>
      <CardActions className="pve-host-actions" onClick={event => event.stopPropagation()}>
        <a href={host.web_url} target="_blank" rel="noreferrer" className="card-action-button" title="打开 PVE Web UI">打开</a>
        <button type="button" className="card-action-button danger" onClick={onDelete}>删除</button>
      </CardActions>
    </CardInner>
  </article>;
}
