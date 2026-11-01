import { Icons, type IconComponent } from "./Icons";
import type { PageId } from "../lib/types";

interface SidebarProps {
  page: PageId;
  onNavigate: (page: PageId) => void;
  onOpenSettings: () => void;
  navigationDisabled?: boolean;
}
const navigation: { id: PageId; label: string; shortLabel: string; icon: IconComponent; hint: string }[] = [
  { id: "project", label: "项目", shortLabel: "项目", icon: Icons.folder, hint: "概览与最近任务" },
  { id: "sources", label: "资料与风格", shortLabel: "资料", icon: Icons.layers, hint: "导入、锚点与视觉参考" },
  { id: "tasks", label: "任务", shortLabel: "任务", icon: Icons.bot, hint: "任务版本、图片生成与结果导入" },
  { id: "result", label: "结果", shortLabel: "结果", icon: Icons.grid, hint: "审阅、校验与导出" },
];
export function Sidebar({ page, onNavigate, onOpenSettings, navigationDisabled }: SidebarProps) {
  return <aside className="sidebar" aria-label="主导航">
    <div className="brand" title="Entwurf · 可验证知识图">
      <span className="brand-mark"><img src={`${import.meta.env.BASE_URL}icon.png`} alt="Entwurf" /></span>
    </div>
    <nav className="nav-list" aria-label="工作区导航">
      {navigation.map(item => {
        const Icon = item.icon;
        return <button key={item.id} disabled={navigationDisabled} className={`nav-item ${page === item.id ? "is-active" : ""}`}
          onClick={() => onNavigate(item.id)} aria-label={item.label} aria-current={page === item.id ? "page" : undefined} title={`${item.label} · ${item.hint}`}>
          <Icon size={22} strokeWidth={1.6} /><span className="rail-label" aria-hidden="true">{item.shortLabel}</span>
        </button>;
      })}
    </nav>
    <div className="sidebar-footer">
      <button className={`nav-item ${page === "settings" ? "is-active" : ""}`} onClick={onOpenSettings} disabled={navigationDisabled}
        aria-label="打开设置" aria-current={page === "settings" ? "page" : undefined} title="设置">
        <Icons.settings size={22} strokeWidth={1.6} /><span className="rail-label" aria-hidden="true">设置</span>
      </button>
    </div>
  </aside>;
}
