import { Icons } from "./Icons";
import type { PageId } from "../lib/types";

const titles: Record<PageId, { eyebrow: string; title: string; description: string }> = {
  project: {
    eyebrow: "工作台 / 项目",
    title: "把教材编译成可追溯的知识图。",
    description: "资料为真值，GPT 只做结构化规划。任何公式改动都保留原文、理由与回滚能力。",
  },
  sources: {
    eyebrow: "工作台 / 资料与风格",
    title: "先确认来源，再决定构图。",
    description: "复制源文件到本地项目并按内容哈希去重；每个知识点都要能回到具体页、行或引用。",
  },
  tasks: {
    eyebrow: "工作台 / 任务",
    title: "管理任务，生成图片与导入结果。",
    description: "图片绑定当前知识版本，完成后保存到项目；结构化规划结果仍可手动导入。",
  },
  result: {
    eyebrow: "工作台 / 结果",
    title: "内容通过后才允许导出。",
    description: "来源一致性、公式与版面是三道独立门禁；视觉分数不能抵消内容错误。",
  },
  settings: {
    eyebrow: "工作台 / 设置",
    title: "本地、可观测、可回退。",
    description: "管理项目目录、OCR、自动化兼容性与导出默认值。设置不会暗中开启后台采集。",
  },
};

interface HeaderProps {
  page: PageId;
  projectName: string;
  theme: "dark" | "light";
  onToggleTheme: () => void;
  onOpenCommand: () => void;
  navigationDisabled?: boolean;
}

export function Header({ page, projectName, theme, onToggleTheme, onOpenCommand, navigationDisabled }: HeaderProps) {
  const content = titles[page];
  return (
    <header className="page-header">
      <div className="breadcrumb">
        <span>Entwurf</span><Icons.chevron size={12} /><span>{projectName}</span><Icons.chevron size={12} /><strong>{content.eyebrow.split(" / ")[1]}</strong>
      </div>
      <div className="header-row">
        <div>
          <span className="eyebrow">{content.eyebrow}</span>
          <h1>{content.title}</h1>
          <p>{content.description}</p>
        </div>
        <div className="header-actions">
          <button className="command-trigger" disabled={navigationDisabled} onClick={onOpenCommand}>
            <Icons.search size={15} /><span>快速跳转</span><kbd>Ctrl K</kbd>
          </button>
          <button className="icon-button bordered" onClick={onToggleTheme} aria-label={theme === "dark" ? "切换到浅色主题" : "切换到深色主题"}>
            {theme === "dark" ? <Icons.sun size={16} /> : <Icons.moon size={16} />}
          </button>
        </div>
      </div>
    </header>
  );
}
