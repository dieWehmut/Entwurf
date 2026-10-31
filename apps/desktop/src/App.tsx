import { useEffect, useMemo, useState } from "react";
import { CommandPalette } from "./components/CommandPalette";
import { Header } from "./components/Header";
import { Sidebar } from "./components/Sidebar";
import { capabilities, demoFindings, demoProject, demoTasks } from "./lib/demo-data";
import type { KnowledgeNode, PageId, ProjectData, SourceFile, Task, Theme, Version } from "./lib/types";
import { ProjectPage } from "./pages/ProjectPage";
import { ResultPage } from "./pages/ResultPage";
import { SettingsPage } from "./pages/SettingsPage";
import { SourcesPage } from "./pages/SourcesPage";
import { TasksPage } from "./pages/TasksPage";

const themeKey = "entwurf.theme";
const pageKey = "entwurf.page";

function readTheme(): Theme {
  const stored = localStorage.getItem(themeKey);
  if (stored === "light" || stored === "dark") return stored;
  return window.matchMedia("(prefers-color-scheme: light)").matches ? "light" : "dark";
}

function readPage(): PageId {
  const stored = localStorage.getItem(pageKey);
  return (["project", "sources", "tasks", "result", "settings"] as PageId[]).includes(stored as PageId) ? (stored as PageId) : "project";
}

export default function App() {
  const [theme, setTheme] = useState<Theme>(readTheme);
  const [page, setPage] = useState<PageId>(readPage);
  const [project, setProject] = useState<ProjectData>(() => ({ ...demoProject, tasks: demoTasks }));
  const [tasks, setTasks] = useState<Task[]>(demoTasks);
  const [activeNodeId, setActiveNodeId] = useState<string | null>("n-cond");
  const [commandOpen, setCommandOpen] = useState(false);
  const [automationEnabled, setAutomationEnabled] = useState(false);
  const [ocrLanguages, setOcrLanguages] = useState("chi_sim+eng");
  const [exportNotice, setExportNotice] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const versions = useMemo<Version[]>(() => tasks.flatMap((task) => task.versions).slice(0, 4), [tasks]);

  useEffect(() => {
    document.documentElement.dataset.theme = theme;
    localStorage.setItem(themeKey, theme);
  }, [theme]);

  useEffect(() => {
    localStorage.setItem(pageKey, page);
  }, [page]);

  useEffect(() => {
    function onKey(event: KeyboardEvent) {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "k") {
        event.preventDefault();
        setCommandOpen((open) => !open);
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  function navigate(next: PageId) {
    setPage(next);
    window.scrollTo({ top: 0, behavior: "smooth" });
  }

  function toggleTheme() {
    setTheme((current) => (current === "dark" ? "light" : "dark"));
  }

  function editNode(node: KnowledgeNode) {
    setProject((current) => ({ ...current, nodes: current.nodes.map((item) => (item.id === node.id ? { ...node, origin: "user_edit" } : item)) }));
    setNotice("已更新节点草稿；点击导出前会重新校验内容门禁。");
  }

  function addSource(source: SourceFile) {
    setProject((current) => ({ ...current, sources: [...current.sources, source] }));
    setNotice(`已加入 ${source.name}；等待本地解析与哈希确认。`);
  }

  function startTask(task: Task) {
    setTasks((current) => current.map((item) => (item.id === task.id ? { ...item, state: "PAUSED", automation: "paused", updatedAt: "刚刚" } : item)));
    setNotice("自动运行未通过可访问性门禁，任务已安全暂停并保留草稿。");
  }

  function pauseTask(task: Task) {
    setTasks((current) => current.map((item) => (item.id === task.id ? { ...item, state: "PAUSED", automation: "manual", updatedAt: "刚刚" } : item)));
    setNotice("任务已暂停，未保存的输入不会丢失。");
  }

  function rollback(task: Task, version: Version) {
    setTasks((current) => current.map((item) => {
      if (item.id !== task.id) return item;
      const revision = item.revision + 1;
      return {
        ...item,
        revision,
        versions: [{ id: `v-${revision}`, revision, reason: "manual_snapshot", createdAt: "刚刚", approved: false, summary: `从 r${version.revision} 创建可审阅快照` }, ...item.versions],
      };
    }));
    setNotice(`已从 r${version.revision} 创建新快照；旧版本保持不变。`);
  }

  function exportArtifacts() {
    setExportNotice("已准备 ArtifactSet：2 个 SVG、2 个 300 DPI PNG、1 个 A3 PDF、manifest.json 与可重载 IR。");
    window.setTimeout(() => setExportNotice(null), 5000);
  }

  function selectTask(task: Task) {
    setTasks((current) => current.map((item) => (item.id === task.id ? item : item)));
    navigate("tasks");
  }

  return (
    <div className="app-shell">
      <Sidebar
        page={page}
        theme={theme}
        projectName={project.name}
        taskCount={tasks.length}
        onNavigate={navigate}
        onToggleTheme={toggleTheme}
        onOpenSettings={() => navigate("settings")}
      />
      <main className="main-column">
        <Header page={page} projectName={project.name} theme={theme} onToggleTheme={toggleTheme} onOpenCommand={() => setCommandOpen(true)} />
        {notice && <div className="toast page-toast" role="status"><span>{notice}</span><button className="icon-button" onClick={() => setNotice(null)} aria-label="关闭提示">×</button></div>}
        {page === "project" && <ProjectPage project={project} tasks={tasks} activeNodeId={activeNodeId} onSelectNode={setActiveNodeId} onNavigate={navigate} onSelectTask={selectTask} />}
        {page === "sources" && <SourcesPage project={project} activeNodeId={activeNodeId} onSelectNode={setActiveNodeId} onAddSource={addSource} />}
        {page === "tasks" && <TasksPage tasks={tasks} onStartTask={startTask} onPauseTask={pauseTask} onRollback={rollback} />}
        {page === "result" && <ResultPage project={project} findings={demoFindings} activeNodeId={activeNodeId} onSelectNode={setActiveNodeId} onEditNode={editNode} versions={versions} onExport={exportArtifacts} exportNotice={exportNotice} />}
        {page === "settings" && <SettingsPage theme={theme} capabilities={capabilities} automationEnabled={automationEnabled} ocrLanguages={ocrLanguages} onToggleAutomation={setAutomationEnabled} onSetOcrLanguages={setOcrLanguages} onToggleTheme={toggleTheme} />}
        <footer className="app-footer"><span>Entwurf v0.1 · 本地优先</span><span>无遥测 · 无账号 · 源资料可追溯</span><span>Apache-2.0</span></footer>
      </main>
      <CommandPalette open={commandOpen} onClose={() => setCommandOpen(false)} onNavigate={navigate} />
    </div>
  );
}
