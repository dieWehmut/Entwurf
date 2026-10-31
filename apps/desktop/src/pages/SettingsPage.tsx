import { useState } from "react";
import { Icons } from "../components/Icons";
import { Panel, StatusPill } from "../components/Panel";
import type { Capability, Theme } from "../lib/types";

interface SettingsPageProps {
  theme: Theme;
  capabilities: Capability[];
  automationEnabled: boolean;
  ocrLanguages: string;
  onToggleAutomation: (enabled: boolean) => void;
  onSetOcrLanguages: (value: string) => void;
  onToggleTheme: () => void;
}

export function SettingsPage({ theme, capabilities, automationEnabled, ocrLanguages, onToggleAutomation, onSetOcrLanguages, onToggleTheme }: SettingsPageProps) {
  const [notice, setNotice] = useState<string | null>(null);
  return (
    <div className="settings-layout">
      <div className="settings-nav" role="tablist" aria-label="设置分类">
        <button className="is-active">外观</button><button>项目与存储</button><button>自动化</button><button>导出</button><button>诊断</button>
      </div>
      <div className="side-stack">
        <Panel eyebrow="外观" title="主题与阅读">
          <div className="setting-row">
            <div><strong>主题</strong><p>深色与浅色均保留相同的信息层级和强调色语义。</p></div>
            <div className="theme-picker">
              <button className={theme === "dark" ? "is-active" : ""} onClick={() => theme !== "dark" && onToggleTheme()}><span className="theme-swatch dark" />深色</button>
              <button className={theme === "light" ? "is-active" : ""} onClick={() => theme !== "light" && onToggleTheme()}><span className="theme-swatch light" />浅色</button>
            </div>
          </div>
          <div className="setting-row"><div><strong>界面密度</strong><p>Classic 间距与细网格；打印布局始终使用 A3。</p></div><select defaultValue="balanced"><option value="balanced">平衡</option><option value="compact">紧凑</option><option value="relaxed">舒展</option></select></div>
          <div className="setting-row"><div><strong>字体</strong><p>界面使用系统无衬线；画布优先 LXGW WenKai，缺失时回退。</p></div><StatusPill tone="ok">系统可用</StatusPill></div>
        </Panel>

        <Panel eyebrow="能力报告" title="本机检测" action={<button className="small-button" onClick={() => setNotice("能力检查已完成：2 项就绪，1 项需人工，1 项缺失。")}><Icons.recheck size={13} /> 重新检测</button>}>
          <div className="capability-list">
            {capabilities.map((capability) => (
              <div className="capability-row" key={capability.id}>
                <span className={`capability-icon ${capability.status}`}>{capability.status === "ready" ? <Icons.ok size={15} /> : capability.status === "missing" ? <Icons.alert size={15} /> : <Icons.info size={15} />}</span>
                <div><strong>{capability.label}</strong><p>{capability.detail}</p></div>
                <StatusPill tone={capability.status === "ready" ? "ok" : capability.status === "missing" ? "warn" : "muted"}>{capability.status === "ready" ? "就绪" : capability.status === "missing" ? "缺失" : "人工"}</StatusPill>
              </div>
            ))}
          </div>
          {notice && <div className="toast inline"><Icons.ok size={14} /> {notice}</div>}
        </Panel>

        <Panel eyebrow="自动化" title="ChatGPT 交接">
          <div className="setting-row">
            <div><strong>实验性自动运行</strong><p>仅在“发现窗口、可靠输入、可靠完成判定、可靠取回”四项验收全部通过后启用。当前设置为默认关闭。</p></div>
            <button className={`switch ${automationEnabled ? "is-on" : ""}`} onClick={() => onToggleAutomation(!automationEnabled)} aria-pressed={automationEnabled} role="switch"><span /></button>
          </div>
          {automationEnabled && <div className="callout warn"><Icons.alert size={15} /><span>尚未通过兼容性验收。自动运行会在启动时再次检查，失败则安全暂停并保留草稿。</span></div>}
          <div className="setting-row"><div><strong>OCR 语言</strong><p>Tesseract 由用户安装；缺失时图片保留为待确认来源。</p></div><input className="compact-input" value={ocrLanguages} onChange={(event) => onSetOcrLanguages(event.target.value)} aria-label="OCR 语言" /></div>
        </Panel>

        <Panel eyebrow="隐私" title="本地数据边界">
          <div className="privacy-grid"><div><Icons.folder size={18} /><strong>项目目录</strong><span>SQLite / JSON · 所有版本只存本机</span></div><div><Icons.bot size={18} /><strong>GPT 调用</strong><span>仅在你显式开始后发生</span></div><div><Icons.ok size={18} /><strong>遥测</strong><span>0 个网络请求</span></div></div>
          <div className="callout subtle"><Icons.info size={15} /><span>不读取 Cookie、不调用私有接口、不绕过登录或额度限制、不进行坐标盲控。</span></div>
        </Panel>
      </div>
    </div>
  );
}
