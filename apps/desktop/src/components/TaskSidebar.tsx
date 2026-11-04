import { Icons } from "./Icons";
import type { WorkspaceSnapshot } from "../lib/workspace";

export function TaskSidebar({ snapshot, locked, onSelect, onCreate, onOpenProject, onNewProject, onClose }: {
  snapshot: WorkspaceSnapshot | null; locked: boolean; onSelect: (id: string) => void;
  onCreate: () => void; onOpenProject: () => void; onNewProject: () => void; onClose: () => void;
}) {
  const groups = new Map<string, NonNullable<WorkspaceSnapshot['project']>['tasks']>();
  for (const task of snapshot?.project.tasks ?? []) {
    const chapter = task.chapter || "任务";
    if (!groups.has(chapter)) groups.set(chapter, []);
    groups.get(chapter)!.push(task);
  }
  return <aside className="task-sidebar" aria-label="任务侧栏">
    <div className="task-sidebar-heading"><strong title={snapshot?.project.name}>{snapshot?.project.name ?? "工作区"}</strong><button className="icon-button task-sidebar-close" onClick={onClose} aria-label="关闭任务侧栏"><Icons.x size={16} /></button></div>
    <button className="task-sidebar-action" disabled={locked || !snapshot} onClick={onCreate}><Icons.plus size={17} />新建任务</button>
    {!snapshot && <button className="task-sidebar-action" disabled={locked} onClick={onNewProject}><Icons.folder size={17} />创建项目</button>}
    <button className="task-sidebar-action muted" disabled={locked} onClick={onOpenProject}><Icons.folder size={17} />切换项目</button>
    <div className="task-sidebar-list">{[...groups].map(([chapter, tasks]) => <section key={chapter} aria-label={chapter}><h2>{chapter}</h2>{tasks.map(task => <button key={task.id} disabled={locked} className={`task-sidebar-item ${snapshot?.selected_task_id === task.id ? "is-active" : ""}`} aria-label={`切换到任务 ${task.title}`} aria-current={snapshot?.selected_task_id === task.id ? "true" : undefined} title={task.title} onClick={() => onSelect(task.id)}><Icons.file size={16} /><span>{task.title}</span></button>)}</section>)}</div>
    {!snapshot && <p className="task-sidebar-empty">打开或新建项目后，任务会显示在这里。</p>}
    {snapshot && !snapshot.project.tasks.length && <p className="task-sidebar-empty">此项目还没有任务。</p>}
  </aside>;
}
