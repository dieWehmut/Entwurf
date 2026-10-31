import { Icons, type IconComponent } from "./Icons";
import type { PageId, Theme } from "../lib/types";

interface SidebarProps {
  page: PageId;
  theme: Theme;
  projectName: string;
  taskCount: number;
  onNavigate: (page: PageId) => void;
  onToggleTheme: () => void;
  onOpenSettings: () => void;
}

const navigation: { id: PageId; label: string; icon: IconComponent; hint: string }[] = [
  { id: "project", label: "项目", icon: Icons.folder, hint: "概览与最近任务" },
  { id: "sources", label: "资料与风格", icon: Icons.layers, hint: "导入、锚点与视觉参考" },
  { id: "tasks", label: "任务", icon: Icons.bot, hint: "GPT 计划与任务版本" },
  { id: "result", label: "结果", icon: Icons.grid, hint: "审阅、校验与导出" },
];

export function Sidebar({
  page,
  theme,
  projectName,
  taskCount,
  onNavigate,
  onToggleTheme,
  onOpenSettings,
}: SidebarProps) {
  return (
    <aside className="sidebar" aria-label="主导航">
      <div className="brand">
        <span className="brand-mark" aria-hidden="true">E</span>
        <div>
          <strong>Entwurf</strong>
          <span>可验证知识图</span>
        </div>
      </div>

      <div className="project-chip">
        <span className="eyebrow">当前项目</span>
        <span className="project-chip-name">{projectName}</span>
        <span className="project-chip-meta">{taskCount} 个任务</span>
      </div>

      <nav className="nav-list">
        {navigation.map((item) => {
          const Icon = item.icon;
          const active = page === item.id;
          return (
            <button
              key={item.id}
              className={`nav-item ${active ? "is-active" : ""}`}
              onClick={() => onNavigate(item.id)}
              aria-current={active ? "page" : undefined}
              title={item.hint}
            >
              <Icon size={17} strokeWidth={1.7} />
              <span>{item.label}</span>
              {active && <span className="nav-dot" aria-hidden="true" />}
            </button>
          );
        })}
      </nav>

      <div className="sidebar-spacer" />

      <div className="sidebar-note">
        <Icons.info size={14} />
        <span>核心功能离线可用。GPT 自动运行默认关闭。</span>
      </div>

      <div className="sidebar-footer">
        <button className="icon-button" onClick={onToggleTheme} aria-label={theme === "dark" ? "切换到浅色主题" : "切换到深色主题"} title={theme === "dark" ? "浅色主题" : "深色主题"}>
          {theme === "dark" ? <Icons.sun size={16} /> : <Icons.moon size={16} />}
        </button>
        <button className="icon-button" onClick={onOpenSettings} aria-label="打开设置" title="设置">
          <Icons.settings size={16} />
        </button>
        <div className="build-tag">v0.1 · local</div>
      </div>
    </aside>
  );
}
