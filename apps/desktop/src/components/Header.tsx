import { Icons } from "./Icons";
import type { PageId } from "../lib/types";

const titles: Record<PageId, { label: string; title: string; description: string }> = {
  project: { label: "项目", title: "你的知识工作台", description: "从一份资料开始，整理章节、审阅知识，再生成图片。" },
  sources: { label: "资料与风格", title: "资料与风格", description: "保留原始来源，为知识图设置画布与视觉风格。" },
  tasks: { label: "任务", title: "任务与图片", description: "管理任务版本，生成图片或导入结构化规划结果。" },
  result: { label: "结果", title: "审阅与导出", description: "核对内容与来源，编译并导出通过校验的知识图。" },
  settings: { label: "设置", title: "设置", description: "管理外观、OCR、连接与本地存储。" },
};
interface HeaderProps {
  page: PageId;
  projectName: string;
  onOpenCommand: () => void;
  navigationDisabled?: boolean;
}
export function Header({ page, projectName, onOpenCommand, navigationDisabled }: HeaderProps) {
  const content = titles[page];
  return <header className="page-header">
    <div className="header-topline"><div className="breadcrumb"><span>Entwurf</span><Icons.chevron size={12} /><span>{projectName}</span><Icons.chevron size={12} /><strong>{content.label}</strong></div>
      <button className="command-trigger" aria-label="快速跳转" title="快速跳转 · Ctrl K" disabled={navigationDisabled} onClick={onOpenCommand}><Icons.search size={15} /><span>快速跳转</span><kbd>Ctrl K</kbd></button>
    </div>
    <div className="header-row"><div><h1>{content.title}</h1><p>{content.description}</p></div></div>
  </header>;
}
