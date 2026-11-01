import { useEffect, useRef, useState } from "react";
import { CommandPalette } from "./components/CommandPalette";
import { Header } from "./components/Header";
import { Sidebar } from "./components/Sidebar";
import { Titlebar } from "./components/Titlebar";
import { TaskSidebar } from "./components/TaskSidebar";
import { ResultPage, type Draft } from "./pages/ResultPage";
import { SourcesPage, SettingsPage, TasksPage } from "./pages/WorkspacePages";
import type { PageId, Theme } from "./lib/types";
import { errorMessage, nativeClient, nodeChanges, type Capabilities, type WorkspaceClient, type WorkspaceSnapshot } from "./lib/workspace";

export type Run = (label: string, action: () => Promise<WorkspaceSnapshot | null | void>, success?: () => void, checkOutcome?: (snapshot: WorkspaceSnapshot | null) => string | null) => Promise<boolean>;
function readTheme(): Theme {
  const stored = localStorage.getItem("entwurf.theme");
  return stored === "light" || stored === "dark" ? stored : "dark";
}
export default function App({ client = nativeClient }: { client?: WorkspaceClient }) {
  const [theme, setTheme] = useState<Theme>(readTheme);
  const [history, setHistory] = useState<{ pages: PageId[]; index: number }>({ pages: ["project"], index: 0 });
  const page = history.pages[history.index];
  const [sidebarOpen, setSidebarOpen] = useState(() => {
    const saved = localStorage.getItem("entwurf.task-sidebar.v1");
    return saved === null ? window.innerWidth > 760 : saved === "open";
  });
  const [snapshot, setSnapshot] = useState<WorkspaceSnapshot | null>(null);
  const [capabilities, setCapabilities] = useState<Capabilities | null>(null);
  const [busy, setBusy] = useState<string | null>(client.available ? "连接本地工作区…" : null);
  const working = useRef(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [commandOpen, setCommandOpen] = useState(false);
  const [projectName, setProjectName] = useState("");
  const [taskTitle, setTaskTitle] = useState("");
  const [chapter, setChapter] = useState("");
  const [projects, setProjects] = useState<{ root: string; name: string; id: string }[] | null>(null);
  const [draft, setDraft] = useState<Draft | null>(null);
  const [activeNodeId, setActiveNodeId] = useState<string | null>(null);
  const task = snapshot?.project.tasks.find(t => t.id === snapshot.selected_task_id);
  const version = snapshot?.current_version;

  useEffect(() => {
    if (!client.available) return;
    let alive = true;
    Promise.allSettled([client.call("bootstrap", undefined), client.call("workspace_snapshot", {})]).then(([caps, workspace]) => {
      if (!alive) return;
      if (caps.status === "fulfilled") setCapabilities(caps.value);
      else setError(errorMessage(caps.reason));
      if (workspace.status === "fulfilled") setSnapshot(workspace.value);
      else if (!/no.*workspace|no project is open|workspace.*not.*open|尚未|未打开/i.test(errorMessage(workspace.reason))) setError(errorMessage(workspace.reason));
      setBusy(null);
    });
    return () => { alive = false; };
  }, [client]);
  useEffect(() => { document.documentElement.dataset.theme = theme; localStorage.setItem("entwurf.theme", theme); }, [theme]);
  useEffect(() => { localStorage.setItem("entwurf.task-sidebar.v1", sidebarOpen ? "open" : "closed"); }, [sidebarOpen]);
  useEffect(() => { document.scrollingElement?.scrollTo({ top: 0, left: 0 }); }, [page]);
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => { if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "k") { event.preventDefault(); if (!working.current) setCommandOpen(o => !o); } };
    window.addEventListener("keydown", onKey); return () => window.removeEventListener("keydown", onKey);
  }, []);
  useEffect(() => {
    if (!draft) return;
    const warn = (e: BeforeUnloadEvent) => { e.preventDefault(); e.returnValue = ""; };
    window.addEventListener("beforeunload", warn); return () => window.removeEventListener("beforeunload", warn);
  }, [draft]);
  const navigate = (next: PageId) => {
    if (working.current || busy) return;
    setHistory(current => {
      if (current.pages[current.index] === next) return current;
      const pages = [...current.pages.slice(0, current.index + 1), next].slice(-64);
      return { pages, index: pages.length - 1 };
    });
    setNotice(null);
  };
  const travel = (offset: number) => {
    if (working.current || busy) return;
    setHistory(current => ({ ...current, index: Math.max(0, Math.min(current.pages.length - 1, current.index + offset)) }));
    setNotice(null);
  };
  const toggleTheme = () => setTheme(t => t === "dark" ? "light" : "dark");
  const run: Run = async (label, action, success, checkOutcome) => {
    if (working.current || !client.available) return false;
    working.current = true; setBusy(label); setError(null); setNotice(null);
    try {
      const next = await action();
      if (next === undefined) return false;
      if (next) setSnapshot(next);
      const outcomeError = checkOutcome?.(next);
      if (outcomeError) { setError(outcomeError); return false; }
      success?.(); setNotice(`${label}完成`); return true;
    } catch (cause) {
      setError(errorMessage(cause));
      // Failed import/compile may still persist a rejection counter or validation report.
      if (snapshot) {
        try { setSnapshot(await client.call("workspace_snapshot", { taskId: snapshot.selected_task_id ?? undefined })); }
        catch { /* Keep last snapshot and unsaved edits available; original error stays visible. */ }
      }
      return false;
    } finally { working.current = false; setBusy(null); }
  };
  const unavailable = !client.available || !!busy;
  const locked = unavailable || !!draft;
  async function saveDraft() {
    if (!draft || !task) return;
    await run("保存节点", async () => {
      const nodes = draft.nodes.map(node => {
        const anchors: unknown = JSON.parse(draft.anchorText[node.id] ?? JSON.stringify(node.anchors));
        if (!Array.isArray(anchors)) throw new Error("来源锚点必须是 JSON 数组；原字段将完整交给本地校验。");
        return { ...node, anchors } as typeof node;
      });
      return client.call("update_nodes", { taskId: task.id, baseVersionId: draft.baseVersionId, edits: nodeChanges(draft.baseNodes, nodes), nodeOrder: nodes.map(n => n.id) });
    }, () => setDraft(null));
  }
  function openProject(create: boolean) {
    if (!create && client.projects) {
      void run("读取项目列表", async () => { setProjects(await client.projects!()); return null; });
      return;
    }
    void run(create ? "创建项目" : "打开项目", async () => {
      const root = await client.call("choose_directory", client.runtime && client.runtime !== "native" ? { purpose: create ? "create" : "open", name: projectName.trim() } : undefined);
      if (!root) return;
      return create ? client.call("create_project", { root, name: projectName.trim() }) : client.call("load_existing_project", { root });
    }, () => setActiveNodeId(null));
  }
  function focusProjectField(id: string) {
    if (locked) return;
    navigate("project");
    if (window.innerWidth <= 760) setSidebarOpen(false);
    requestAnimationFrame(() => document.getElementById(id)?.focus());
  }
  function chooseProject() { if (!locked) { navigate("project"); openProject(false); } }
  async function selectSidebarTask(taskId: string) {
    if (locked) return;
    if (await run("切换任务", () => client.call("workspace_snapshot", { taskId }), () => setActiveNodeId(null))) {
      navigate("tasks");
      if (window.innerWidth <= 760) setSidebarOpen(false);
    }
  }
  const sidebarVisible = sidebarOpen && page !== "settings";
  function toggleSidebar() {
    if (working.current || busy) return;
    if (page === "settings") { navigate("project"); setSidebarOpen(true); }
    else setSidebarOpen(open => !open);
  }
  return <div className={`app-shell${sidebarVisible ? " has-task-sidebar" : ""}`}>
    <Titlebar native={client.runtime === "native"} projectName={snapshot?.project.name ?? "Entwurf"} sidebarOpen={sidebarVisible} busy={!!busy} locked={locked}
      hasDraft={!!draft} canSave={!!draft && !unavailable && version?.id === draft.baseVersionId} canCreateTask={!!snapshot} canBack={history.index > 0} canForward={history.index < history.pages.length - 1}
      onToggleSidebar={toggleSidebar} onBack={() => travel(-1)} onForward={() => travel(1)}
      onNewProject={() => focusProjectField("project-name")} onOpenProject={chooseProject} onNewTask={() => focusProjectField("task-title")}
      onSave={() => { void saveDraft(); }} onDiscard={() => setDraft(null)} onNavigate={navigate} onOpenCommand={() => setCommandOpen(true)} onError={setError} />
    <Sidebar page={page} onNavigate={navigate} onOpenSettings={() => navigate("settings")} navigationDisabled={!!busy} theme={theme} onToggleTheme={toggleTheme} />
    {sidebarVisible && <TaskSidebar snapshot={snapshot} locked={locked} onSelect={id => { void selectSidebarTask(id); }} onCreate={() => focusProjectField("task-title")} onOpenProject={chooseProject} onClose={() => setSidebarOpen(false)} />}
    <main className={`main-column page-${page}${snapshot ? "" : " is-empty"}`} aria-busy={!!busy}>
      {page !== "settings" && <Header page={page} />}
      {!client.available && <div className="callout warn" role="status">浏览器预览：本地工作区不可用。请启动 Windows 桌面应用以创建项目、导入和导出文件。</div>}
      {busy && <div className="toast page-toast" role="status">{busy}</div>}
      {error && <div className="callout warn" role="alert"><span>{error}</span><button onClick={() => setError(null)} aria-label="关闭错误">×</button></div>}
      {notice && <div className="toast page-toast" role="status">{notice}</div>}
      {draft && <div className="callout warn">节点有未保存修改。保存或放弃后才能切换任务、修改来源、导出或回滚。{version?.id !== draft.baseVersionId && <strong>当前版本已改变，草稿已过期；保留内容供复制，放弃草稿后重新编辑。</strong>}<button className="text-button" disabled={unavailable} onClick={() => navigate("result")}>查看草稿</button></div>}
      {snapshot && page !== "settings" && <div className="workspace-toolbar current-workspace">
        <label>当前任务<select aria-label="当前任务" disabled={locked} value={snapshot.selected_task_id ?? ""} onChange={e => { void run("切换任务", () => client.call("workspace_snapshot", { taskId: e.target.value }), () => setActiveNodeId(null)); }}>
          <option value="" disabled>请选择任务</option>{snapshot.project.tasks.map(t => <option key={t.id} value={t.id}>{t.chapter ? `${t.chapter} / ` : ""}{t.title}</option>)}
        </select></label><span>{task?.state ?? "没有任务"} · {version ? `r${version.revision}` : "尚无知识版本"}</span>
        <button className="secondary-button" disabled={unavailable} onClick={() => { void run("刷新工作区", () => client.call("workspace_snapshot", { taskId: task?.id })); }}>刷新工作区</button>
      </div>}
      {page === "project" && <div className="workspace-stack project-workspace">
        <section className="panel"><h2>{client.runtime === "browser" ? "浏览器项目" : "本地项目"}</h2><p className="muted path-text">{snapshot?.root ?? (client.runtime === "browser" ? "项目保存在此浏览器中，刷新或重新打开页面后可继续编辑。" : client.runtime === "local-http" ? "项目保存在本机服务的数据库中。" : "选择本机目录，新建项目或打开已有项目。")}</p>
          <div className="workspace-toolbar"><label className="field">项目名称<input id="project-name" disabled={locked} value={projectName} onChange={e => setProjectName(e.target.value)} placeholder="输入项目名称" /></label><button className="primary-button" disabled={locked || !projectName.trim()} onClick={() => openProject(true)}>新建项目</button><button className="secondary-button" disabled={locked} onClick={() => openProject(false)}>打开项目</button></div>
          {projects && <section aria-label="选择项目"><h3>已保存项目</h3>{projects.length ? projects.map(project => <button className="secondary-button" disabled={locked} key={project.id} onClick={() => { void run("打开项目", () => client.call("load_existing_project", { root: project.root }), () => { setProjects(null); setActiveNodeId(null); }); }}>{project.name}</button>) : <p className="empty-note">此存储中还没有项目。</p>}<button className="text-button" onClick={() => setProjects(null)}>关闭项目列表</button></section>}
          {client.runtime === "browser" && <p className="muted">清除网站数据会删除浏览器项目。请定期导出结果；不同浏览器与设备的数据分别保存。</p>}
        </section>
        {snapshot && <><section className="panel"><h2>章节与任务</h2><div className="workspace-toolbar"><label className="field">任务标题<input id="task-title" disabled={locked} value={taskTitle} onChange={e => setTaskTitle(e.target.value)} /></label><label className="field">章节（可选）<input disabled={locked} value={chapter} onChange={e => setChapter(e.target.value)} /></label><button className="primary-button" disabled={locked || !taskTitle.trim()} onClick={() => { void run("创建任务", () => client.call("create_task", { title: taskTitle.trim(), chapter: chapter.trim() || undefined }), () => setTaskTitle("")).then(saved => { if (saved) navigate("sources"); }); }}>创建任务</button></div>
          <div className="task-board">{snapshot.project.tasks.map(t => <button key={t.id} className={`task-card ${task?.id === t.id ? "is-active" : ""}`} disabled={locked} onClick={() => { void selectSidebarTask(t.id); }}><strong>{t.title}</strong><span>{t.chapter} · {t.state} · {t.versions.length} 个版本</span></button>)}</div>{!snapshot.project.tasks.length && <p className="empty-note">创建第一个任务，然后导入知识资料。</p>}
        </section><section className="panel"><h2>当前任务概况</h2><div className="stat-strip"><div className="stat-item"><strong>{version?.ir.sources.length ?? 0}</strong><span>来源</span></div><div className="stat-item"><strong>{version?.ir.nodes.length ?? 0}</strong><span>知识节点</span></div><div className="stat-item"><strong>{version?.ir.unverified.length ?? 0}</strong><span>待确认</span></div><div className="stat-item"><strong>{version?.layout?.pages.length ?? 0}</strong><span>已编译页面</span></div></div><button className="secondary-button" disabled={unavailable} onClick={() => navigate("result")}>审阅结果</button></section></>}
      </div>}
      {page === "sources" && (snapshot ? <SourcesPage key={snapshot.project.id} onCreateTask={() => navigate("project")} snapshot={snapshot} client={client} run={run} locked={locked} /> : <p className="empty-note">先在项目页打开项目。</p>)}
      {page === "tasks" && (snapshot && task ? <TasksPage key={task.id} snapshot={snapshot} client={client} run={run} locked={locked} /> : <p className="empty-note">先创建或选择一个任务。</p>)}
      {page === "result" && (snapshot && version ? <ResultPage key={task?.id} snapshot={snapshot} client={client} run={run} locked={locked} busy={unavailable} draft={draft} setDraft={setDraft} activeNodeId={activeNodeId} setActiveNodeId={setActiveNodeId} saveDraft={saveDraft} /> : <p className="empty-note">当前任务没有知识版本。导入资料后可编辑、校验和预览。</p>)}
      {page === "settings" && <SettingsPage snapshot={snapshot} capabilities={capabilities} client={client} run={run} locked={locked} theme={theme} toggleTheme={toggleTheme} onOpenTasks={() => navigate("tasks")} refreshCapabilities={() => { void run("检测能力", async () => { setCapabilities(await client.call("bootstrap", undefined)); return null; }); }} />}
      {page !== "settings" && <footer className="app-footer"><span>Entwurf v0.1 · 本地优先</span><span>无遥测 · 源资料可追溯</span><span>Apache-2.0</span></footer>}
    </main><CommandPalette open={commandOpen} onClose={() => setCommandOpen(false)} onNavigate={navigate} />
  </div>;
}
