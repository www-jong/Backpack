import { useEffect, useRef, useState } from "react";
import { Monitor, Moon, Sun, X } from "lucide-react";
import type { Preferences, ThemePreference } from "./preferences";
import type { Snapshot } from "./types";

export type SettingsTab = "general" | "paths";
interface Props {
  initialTab: SettingsTab; preferences: Preferences; saveError: boolean;
  onPreferences: (value: Preferences) => void; onClose: () => void;
  project: string; onProject: (value: string) => void;
  roots: Record<string, string>; onRoots: (value: Record<string, string>) => void;
  snapshot: Snapshot | null; canScan: boolean; onScan: () => void;
}
const themes: { id: ThemePreference; label: string; icon: typeof Monitor }[] = [
  { id: "system", label: "시스템", icon: Monitor },
  { id: "light", label: "라이트", icon: Sun },
  { id: "dark", label: "다크", icon: Moon },
];
const agents = { codex: "Codex", claude: "Claude Code", antigravity: "Antigravity", opencode: "OpenCode" };
export default function Settings(props: Props) {
  const [tab, setTab] = useState(props.initialTab);
  const dialog = useRef<HTMLElement>(null);
  useEffect(() => {
    const previous = document.activeElement as HTMLElement | null;
    dialog.current?.focus();
    return () => previous?.focus();
  }, []);
  return <div className="overlay modal-overlay" onClick={props.onClose}>
    <section ref={dialog} tabIndex={-1} className="settings-modal" role="dialog" aria-modal="true" aria-label="앱 설정"
      onClick={e => e.stopPropagation()} onKeyDown={e => {
        if (e.key === "Escape") { e.stopPropagation(); props.onClose(); }
        if (e.key === "Tab") {
          const controls = Array.from(dialog.current?.querySelectorAll<HTMLElement>('button:not(:disabled),input:not(:disabled),[tabindex="0"]') ?? []);
          const first = controls[0]; const last = controls[controls.length - 1];
          if (e.shiftKey && (document.activeElement === first || document.activeElement === dialog.current)) { e.preventDefault(); last?.focus(); }
          else if (!e.shiftKey && (document.activeElement === last || document.activeElement === dialog.current)) { e.preventDefault(); first?.focus(); }
        }
      }}>
      <div className="panel-top"><h2>앱 설정</h2><button aria-label="설정 닫기" onClick={props.onClose}><X size={20}/></button></div>
      <div className="settings-tabs" role="tablist" aria-label="설정 종류">
        <button id="general-tab" role="tab" aria-selected={tab === "general"} aria-controls="general-settings" onClick={() => setTab("general")}>일반</button>
        <button id="paths-tab" role="tab" aria-selected={tab === "paths"} aria-controls="path-settings" onClick={() => setTab("paths")}>탐지 경로</button>
      </div>
      {tab === "general" ? <div role="tabpanel" id="general-settings" aria-labelledby="general-tab">
        <div className="settings-section"><h3>화면 테마</h3><p>시스템을 선택하면 운영체제의 테마 변경을 자동으로 따릅니다.</p>
          <div className="theme-options" role="group" aria-label="화면 테마">{themes.map(({id, label, icon: Icon}) => <button key={id} className={props.preferences.theme === id ? "selected" : ""} aria-pressed={props.preferences.theme === id} onClick={() => props.onPreferences({ ...props.preferences, theme: id })}><Icon size={22}/>{label}</button>)}</div>
        </div>
        <div className="settings-section"><h3>기능 설정</h3>
          <label className="setting-option"><span><strong>기본 제공 항목 표시</strong><small>에이전트가 기본으로 제공하는 스킬과 플러그인을 목록에 포함합니다.</small></span><input type="checkbox" checked={props.preferences.showBundled} onChange={e => props.onPreferences({ ...props.preferences, showBundled: e.target.checked })}/></label>
          <label className="setting-option"><span><strong>시작 시 자동 탐지</strong><small>앱을 시작할 때 이 PC의 에이전트 설정을 읽어옵니다.</small></span><input type="checkbox" checked={props.preferences.scanOnStartup} onChange={e => props.onPreferences({ ...props.preferences, scanOnStartup: e.target.checked })}/></label>
        </div>
        {props.saveError ? <p className="settings-save-error" role="alert">설정을 저장하지 못했습니다. 현재 실행 중에는 적용되지만 재실행 후 유지되지 않을 수 있습니다.</p> : <p className="settings-saved">변경 즉시 적용되며, 이 PC에 자동으로 저장됩니다.</p>}
        <div className="modal-actions"><button className="secondary" onClick={props.onClose}>닫기</button></div>
      </div> : <div role="tabpanel" id="path-settings" aria-labelledby="paths-tab">
        <p className="settings-description">프로젝트와 에이전트 설정 폴더의 절대 경로를 입력하세요. 경로는 이번 실행에만 적용됩니다.</p>
        <label className="field">프로젝트 폴더<input value={props.project} onChange={e => props.onProject(e.target.value)} placeholder="프로젝트 폴더의 절대 경로"/></label>
        <div className="field-divider">에이전트 설정 루트 <span>비워두면 기본값 사용</span></div>
        {Object.entries(agents).map(([id, name]) => <label className="field" key={id}>{name}<input value={props.roots[id] ?? ""} onChange={e => props.onRoots({ ...props.roots, [id]: e.target.value })} placeholder={props.snapshot?.agents.find(a => a.id === id)?.configRoots[0] ?? "기본 설정 경로"}/></label>)}
        <div className="modal-actions"><button className="secondary" onClick={props.onClose}>닫기</button><button className="primary" disabled={!props.canScan} onClick={props.onScan}>이 경로로 탐지</button></div>
      </div>}
    </section>
  </div>;
}
