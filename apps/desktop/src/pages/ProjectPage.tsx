import { Icons } from "../components/Icons";
import { KnowledgePreview } from "../components/KnowledgePreview";
import { Panel, StatusPill } from "../components/Panel";
import type { PageId, ProjectData, Task } from "../lib/types";

interface ProjectPageProps {
  project: ProjectData;
  tasks: Task[];
  activeNodeId: string | null;
  onSelectNode: (id: string) => void;
  onNavigate: (page: PageId) => void;
  onSelectTask: (task: Task) => void;
}

export function ProjectPage({ project, tasks, activeNodeId, onSelectNode, onNavigate, onSelectTask }: ProjectPageProps) {
  const activeTask = tasks[0];
  const critical = project.nodes.filter((node) => node.importance === "critical").length;
  const anchored = project.nodes.filter((node) => node.anchors.length > 0).length;
  return (
    <div className="page-stack">
      <section className="stat-strip" aria-label="项目统计">
        <div><span className="stat-label">来源</span><strong>{project.sources.length}</strong><span className="stat-note">全部已哈希</span></div>
        <div><span className="stat-label">知识点</span><strong>{project.nodes.length}</strong><span className="stat-note">{critical} 个关键</span></div>
        <div><span className="stat-label">可追溯</span><strong>{Math.round((anchored / project.nodes.length) * 100)}%</strong><span className="stat-note">{anchored}/{project.nodes.length} 带锚点</span></div>
        <div><span className="stat-label">当前版本</span><strong>r{activeTask?.revision ?? 0}</strong><span className="stat-note">{activeTask?.state ?? "DRAFT"}</span></div>
      </section>

      <div className="two-column">
        <Panel
          eyebrow="当前画面"
          title="知识图预览"
          action={<button className="text-button" onClick={() => onNavigate("result")}>打开结果 <Icons.chevron size={13} /></button>}
          className="preview-panel"
        >
          <KnowledgePreview nodes={project.nodes} edges={project.edges} activeNodeId={activeNodeId} onSelectNode={onSelectNode} />
          <div className="preview-foot">
            <StatusPill tone="ok"><Icons.ok size={12} /> 内容门禁通过</StatusPill>
            <span>2 页 · A3 横向 · 8pt 最小字号</span>
            <span>最近编译 17:42</span>
          </div>
        </Panel>

        <div className="side-stack">
          <Panel eyebrow="最近任务" title="章节进度" action={<button className="text-button" onClick={() => onNavigate("tasks")}>全部任务 <Icons.chevron size={13} /></button>}>
            <div className="task-list compact-list">
              {tasks.map((task) => (
                <button className="task-row" key={task.id} onClick={() => onSelectTask(task)}>
                  <span className={`task-state state-${task.state.toLowerCase().replace("_", "-")}`} />
                  <span className="task-main">
                    <strong>{task.title}</strong>
                    <span>{task.state} · r{task.revision} · {task.updatedAt}</span>
                  </span>
                  <span className="task-progress" aria-label={`进度 ${task.progress}%`}><span style={{ width: `${task.progress}%` }} /></span>
                </button>
              ))}
            </div>
          </Panel>
          <Panel eyebrow="下一步" title="审阅与导出">
            <div className="next-step">
              <div className="next-step-icon"><Icons.sparkles size={18} /></div>
              <div>
                <strong>第一章已可人工审阅</strong>
                <p>公式与来源链已通过，剩余 1 条建议级提示。确认版本后可导出 SVG、300 DPI PNG 与 A3 PDF。</p>
              </div>
            </div>
            <button className="primary-button full" onClick={() => onNavigate("result")}><Icons.grid size={15} /> 进入审阅</button>
          </Panel>
        </div>
      </div>
    </div>
  );
}
