import { useEffect, useMemo, useRef, useState } from "react";
import { Icons } from "./Icons";
import type { PageId } from "../lib/types";

interface CommandPaletteProps {
  open: boolean;
  onClose: () => void;
  onNavigate: (page: PageId) => void;
}

const items: { page: PageId; label: string; detail: string }[] = [
  { page: "project", label: "项目概览", detail: "最近任务与知识图预览" },
  { page: "sources", label: "资料与风格", detail: "导入来源、检查锚点" },
  { page: "tasks", label: "任务", detail: "查看版本与自动运行闸门" },
  { page: "result", label: "结果与导出", detail: "审阅、校验、导出" },
  { page: "settings", label: "设置", detail: "主题、能力报告、隐私" },
];

export function CommandPalette({ open, onClose, onNavigate }: CommandPaletteProps) {
  const [query, setQuery] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);
  const dialogRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (open) {
      const previous = document.activeElement as HTMLElement | null;
      setQuery("");
      const frame = requestAnimationFrame(() => inputRef.current?.focus());
      return () => { cancelAnimationFrame(frame); previous?.focus(); };
    }
  }, [open]);
  useEffect(() => {
    function onKey(event: KeyboardEvent) {
      if (event.key === "Escape") onClose();
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);
  const filtered = useMemo(() => items.filter((item) => `${item.label}${item.detail}`.toLowerCase().includes(query.toLowerCase())), [query]);
  if (!open) return null;
  return (
    <div className="palette-backdrop" onMouseDown={onClose}>
      <div ref={dialogRef} onKeyDown={event => {
        if (event.key === "Tab") {
          const controls = dialogRef.current?.querySelectorAll<HTMLElement>("input, button");
          if (!controls?.length) return;
          const first = controls[0]; const last = controls[controls.length - 1];
          if (event.shiftKey && document.activeElement === first) { event.preventDefault(); last.focus(); }
          else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first.focus(); }
        }
        if (event.key === "Enter" && event.target === inputRef.current && filtered[0]) { onNavigate(filtered[0].page); onClose(); }
      }} className="palette" onMouseDown={(event) => event.stopPropagation()} role="dialog" aria-modal="true" aria-label="命令面板">
        <div className="palette-input-row"><Icons.search size={17} /><input aria-label="搜索页面" ref={inputRef} value={query} onChange={(event) => setQuery(event.target.value)} placeholder="跳转到页面或功能…" /></div>
        <div className="palette-list">
          {filtered.map((item, index) => (
            <button key={item.page} className={index === 0 ? "is-highlighted" : ""} onClick={() => { onNavigate(item.page); onClose(); }}>
              <Icons.chevron size={13} /><span><strong>{item.label}</strong><small>{item.detail}</small></span>
            </button>
          ))}
          {filtered.length === 0 && <div className="empty-note">没有匹配项</div>}
        </div>
        <div className="palette-foot"><span><kbd>↵</kbd> 打开</span><span><kbd>esc</kbd> 关闭</span><span>本地搜索 · 不联网</span></div>
      </div>
    </div>
  );
}
