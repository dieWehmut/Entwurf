import { useState } from "react";
import { Icons } from "../components/Icons";
import { Panel, StatusPill } from "../components/Panel";
import type { Task, Version } from "../lib/types";

interface TasksPageProps {
  tasks: Task[];
  onStartTask: (task: Task) => void;
  onPauseTask: (task: Task) => void;
  onRollback: (task: Task, version: Version) => void;
}

export function TasksPage({ tasks, onStartTask, onPauseTask, onRollback }: TasksPageProps) {
  const [selected, setSelected] = useState(tasks[0]?.id ?? "");
  const task = tasks.find((item) => item.id === selected) ?? tasks[0];
  if (!task) return <Panel>暂无任务</Panel>;
  return (
    <div className="two-column align-start">
      <div className="side-stack">
        <Panel eyebrow="任务队列" title="章节任务" action={<button className="small-button"><Icons.command size={13} /> 新建任务</button>}>
          <div className="task-board">
            {tasks.map((item) => (
              <button key={item.id} className={`task-card ${item.id === task.id ? "is-active" : ""}`} onClick={() => setSelected(item.id)}>
                <div className="task-card-top">
                  <StatusPill tone={stateTone(item.state)}>{stateLabel(item.state)}</StatusPill>
                  <span className="task-meta">r{item.revision} · {item.updatedAt}</span>
                </div>
                <strong>{item.title}</strong>
                <div className="task-step-track">
                  {["摄取", "IR", "计划", "导入", "编译", "审阅"].map((step, index) => (
                    <span key={step} className={index < Math.ceil(item.progress / 17) ? "is-done" : ""}>{step}</span>
                  ))}
                </div>
                <div className="task-progress"><span style={{ width: `${item.progress}%` }} /></div>
              </button>
            ))}
          </div>
        </Panel>

        <Panel eyebrow="任务详情" title={task.title}>
          <div className="task-detail-head">
            <div className="state-machine">
              <span className="done">DRAFT</span><Icons.chevron size={11} /><span className="done">INGESTING</span><Icons.chevron size={11} /><span className="done">IR_READY</span><Icons.chevron size={11} /><span className="done">COMPILING</span><Icons.chevron size={11} /><span className="current">NEEDS_REVIEW</span>
            </div>
            <div className="task-controls">
              <button className="primary-button" onClick={() => onStartTask(task)}><Icons.play size={14} /> 开始 / 导入</button>
              <button className="secondary-button" onClick={() => onPauseTask(task)}><Icons.pause size={14} /> 暂停</button>
            </div>
          </div>
          <div className="automation-gate">
            <div className="gate-icon"><Icons.bot size={18} /></div>
            <div>
              <strong>自动运行保持关闭</strong>
              <p>当前 ChatGPT 桌面应用的聊天控件无法被 WebView 可访问性可靠读取。不会使用坐标盲控。可用“复制任务 / 打开 ChatGPT / 导入结果”完成一次人工闭环。</p>
            </div>
            <StatusPill tone="warn">人工接管</StatusPill>
          </div>
          <div className="handoff-actions">
            <button className="secondary-button"><Icons.command size={14} /> 复制 TaskEnvelope</button>
            <button className="secondary-button"><Icons.file size={14} /> 打开 ChatGPT</button>
            <button className="secondary-button"><Icons.upload size={14} /> 导入 ResultEnvelope</button>
          </div>
        </Panel>

        <Panel eyebrow="运行日志" title="可观测事件">
          <ol className="timeline">
            <li><span className="timeline-dot ok" /><div><strong>校验通过</strong><p>8/8 节点有来源，公式定界符平衡，版面无重叠。</p><time>17:42:08</time></div></li>
            <li><span className="timeline-dot accent" /><div><strong>编译完成</strong><p>生成 2 页 A3 SVG、300 DPI PNG 与矢量 PDF。</p><time>17:41:56</time></div></li>
            <li><span className="timeline-dot warn" /><div><strong>自动运行暂停</strong><p>ChatGPT 窗口可访问性不匹配；草稿与版本已保存。</p><time>17:24:31</time></div></li>
            <li><span className="timeline-dot ok" /><div><strong>ResultEnvelope 导入</strong><p>新增 2 个分组、1 条公式提示；未覆盖手工修改。</p><time>17:24:18</time></div></li>
          </ol>
        </Panel>
      </div>

      <Panel eyebrow="版本历史" title="不可变任务版本" action={<span className="count-tag">{task.versions.length} 个版本</span>}>
        <div className="version-list">
          {task.versions.map((version, index) => (
            <div key={version.id} className="version-row">
              <div className="version-rail"><span className={index === 0 ? "is-current" : ""} /></div>
              <div className="version-content">
                <div className="version-top">
                  <strong>r{version.revision} · {versionLabel(version.reason)}</strong>
                  <span>{version.createdAt}</span>
                </div>
                <p>{version.summary}</p>
                <div className="version-actions">
                  {version.approved ? <StatusPill tone="ok">已批准</StatusPill> : <StatusPill tone="muted">未批准</StatusPill>}
                  <button className="text-button" onClick={() => onRollback(task, version)}><Icons.revert size={12} /> 从此版本创建副本</button>
                </div>
              </div>
            </div>
          ))}
        </div>
        <div className="callout subtle">
          <Icons.info size={15} />
          <span>回滚不会删除后续版本；它会创建一个新的、可审阅的快照，便于并排比较。</span>
        </div>
      </Panel>
    </div>
  );
}

function stateLabel(state: string) {
  return ({ NEEDS_REVIEW: "待审阅", IR_READY: "IR 就绪", DRAFT: "草稿", APPROVED: "已批准", PAUSED: "已暂停", FAILED: "失败" } as Record<string, string>)[state] ?? state;
}

function stateTone(state: string): "ok" | "warn" | "muted" | "accent" {
  if (state === "APPROVED") return "ok";
  if (state === "NEEDS_REVIEW" || state === "PAUSED") return "warn";
  if (state === "IR_READY") return "accent";
  return "muted";
}

function versionLabel(reason: Version["reason"]) {
  return ({ ingested: "摄取", gpt_imported: "GPT 导入", user_edited: "手工编辑", correction_reverted: "纠错回退", manual_snapshot: "快照" })[reason];
}
