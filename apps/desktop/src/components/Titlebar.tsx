import { useEffect, useRef, useState } from "react";
import { Icons } from "./Icons";
import type { PageId } from "../lib/types";

interface Props {
  native: boolean; projectName: string; sidebarOpen: boolean; busy: boolean; locked: boolean;
  hasDraft: boolean; canSave: boolean; canCreateTask: boolean; canBack: boolean; canForward: boolean;
  onToggleSidebar: () => void; onBack: () => void; onForward: () => void;
  onNewProject: () => void; onOpenProject: () => void; onNewTask: () => void;
  onSave: () => void; onDiscard: () => void; onNavigate: (page: PageId) => void;
  onOpenCommand: () => void; onError: (message: string) => void;
}
type Menu = "文件" | "编辑" | "视图" | "帮助";
export function Titlebar(props: Props) {
  const [menu, setMenu] = useState<Menu | null>(null);
  const [help, setHelp] = useState(false);
  const root = useRef<HTMLDivElement>(null);
  const helpReturn = useRef<HTMLElement | null>(null);
  const trigger = useRef<HTMLButtonElement | null>(null);
  const latest = useRef(props); latest.current = props;
  const closeMenu = () => { setMenu(null); trigger.current?.focus(); };
  const closeHelp = () => { setHelp(false); requestAnimationFrame(() => helpReturn.current?.focus()); };
  useEffect(() => {
    if (!menu) return;
    root.current?.querySelector<HTMLButtonElement>('[role="menuitem"]:not(:disabled)')?.focus();
    const outside = (event: PointerEvent) => { if (!root.current?.contains(event.target as Node)) setMenu(null); };
    document.addEventListener("pointerdown", outside);
    return () => document.removeEventListener("pointerdown", outside);
  }, [menu]);
  useEffect(() => {
    if (!props.native) return;
    let gone = false; let unlisten: (() => void) | undefined;
    void import("@tauri-apps/api/window").then(({ getCurrentWindow }) => getCurrentWindow().onCloseRequested(event => {
      if (latest.current.busy || latest.current.hasDraft) {
        event.preventDefault(); latest.current.onError("请先完成当前操作，并保存或放弃节点草稿后关闭窗口。");
      }
    })).then(stop => { if (gone) stop(); else unlisten = stop; }).catch(error => latest.current.onError(String(error)));
    return () => { gone = true; unlisten?.(); };
  }, [props.native]);
  async function windowAction(action: "minimize" | "toggleMaximize" | "close") {
    if (!props.native || (action === "close" && (props.busy || props.hasDraft))) return;
    try { const { getCurrentWindow } = await import("@tauri-apps/api/window"); await getCurrentWindow()[action](); }
    catch (error) { props.onError(String(error)); }
  }
  const entries: Record<Menu, { label: string; action: () => void; disabled?: boolean }[]> = {
    文件: [
      { label: "新建项目", action: props.onNewProject, disabled: props.locked },
      { label: "打开项目", action: props.onOpenProject, disabled: props.locked },
      { label: "新建任务", action: props.onNewTask, disabled: props.locked || !props.canCreateTask },
    ],
    编辑: [
      { label: "保存节点修改", action: props.onSave, disabled: !props.canSave },
      { label: "放弃节点草稿", action: props.onDiscard, disabled: props.busy || !props.hasDraft },
    ],
    视图: [
      { label: props.sidebarOpen ? "隐藏任务侧栏" : "显示任务侧栏", action: props.onToggleSidebar, disabled: props.busy },
      { label: "快速跳转", action: props.onOpenCommand, disabled: props.busy },
      { label: "项目", action: () => props.onNavigate("project"), disabled: props.busy },
      { label: "资料与风格", action: () => props.onNavigate("sources"), disabled: props.busy },
      { label: "任务", action: () => props.onNavigate("tasks"), disabled: props.busy },
      { label: "结果", action: () => props.onNavigate("result"), disabled: props.busy },
    ],
    帮助: [
      { label: "使用说明", action: () => { helpReturn.current = document.activeElement as HTMLElement; setHelp(true); } },
      { label: "应用设置", action: () => props.onNavigate("settings"), disabled: props.busy },
    ],
  };
  return <><div className="titlebar" ref={root} onKeyDown={event => {
    if (event.key === "Escape") { event.preventDefault(); closeMenu(); }
    if (menu && ["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key)) {
      const items = [...(root.current?.querySelectorAll<HTMLButtonElement>('[role="menuitem"]:not(:disabled)') ?? [])];
      const index = items.indexOf(document.activeElement as HTMLButtonElement);
      const next = event.key === "Home" ? 0 : event.key === "End" ? items.length - 1 : (index + (event.key === "ArrowDown" ? 1 : -1) + items.length) % items.length;
      event.preventDefault(); items[next]?.focus();
    }
  }}>
    <button className="titlebar-button" aria-label="切换任务侧栏" aria-expanded={props.sidebarOpen} title="切换任务侧栏" disabled={props.busy} onClick={props.onToggleSidebar}><Icons.panel size={17} /></button>
    <button className="titlebar-button" aria-label="后退" disabled={props.busy || !props.canBack} onClick={props.onBack}><Icons.back size={17} /></button>
    <button className="titlebar-button" aria-label="前进" disabled={props.busy || !props.canForward} onClick={props.onForward}><Icons.forward size={17} /></button>
    <div className="titlebar-menus">{(Object.keys(entries) as Menu[]).map(name => <div className="titlebar-menu" key={name}>
      <button className="titlebar-menu-trigger" aria-haspopup="menu" aria-expanded={menu === name} onClick={event => { trigger.current = event.currentTarget; setMenu(menu === name ? null : name); }}>{name}</button>
      {menu === name && <div className="titlebar-popover" role="menu" aria-label={`${name}菜单`}>{entries[name].map(entry => <button role="menuitem" key={entry.label} disabled={entry.disabled} onClick={() => { closeMenu(); entry.action(); }}>{entry.label}</button>)}</div>}
    </div>)}</div>
    <div className="titlebar-drag" data-tauri-drag-region={props.native ? "" : undefined}><span data-tauri-drag-region={props.native ? "" : undefined}>{props.projectName}</span></div>
    {props.native && <div className="window-controls"><button aria-label="最小化窗口" title="最小化" onClick={() => { void windowAction("minimize"); }}><Icons.minimize size={18} /></button><button aria-label="切换窗口最大化" title="最大化 / 还原" onClick={() => { void windowAction("toggleMaximize"); }}><Icons.maximize size={13} /></button><button className="window-close" aria-label="关闭窗口" title="关闭" disabled={props.busy || props.hasDraft} onClick={() => { void windowAction("close"); }}><Icons.x size={17} /></button></div>}
  </div>{help && <div className="palette-backdrop" onMouseDown={closeHelp}><section className="shell-help panel" role="dialog" aria-modal="true" aria-labelledby="shell-help-title" onMouseDown={event => event.stopPropagation()} onKeyDown={event => {
    if (event.key === "Escape") { event.preventDefault(); closeHelp(); }
    if (event.key === "Tab") {
      const controls = event.currentTarget.querySelectorAll<HTMLButtonElement>("button");
      const first = controls[0]; const last = controls[controls.length - 1];
      if (event.shiftKey && document.activeElement === first) { event.preventDefault(); last?.focus(); }
      else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first?.focus(); }
    }
  }}><div className="result-head"><h2 id="shell-help-title">使用 Entwurf</h2><button autoFocus className="icon-button" aria-label="关闭使用说明" onClick={closeHelp}><Icons.x size={18} /></button></div><ol><li>在项目页新建或打开项目，再创建任务。</li><li>在资料页导入来源，设置画布与风格。</li><li>在结果页审阅知识与来源，保存修改后编译和导出。</li><li>在任务页连接 ChatGPT，明确启动一次图片生成。</li></ol><p>任务侧栏显示当前项目的真实任务。Ctrl K 可快速跳转页面。</p></section></div>}</>;
}
