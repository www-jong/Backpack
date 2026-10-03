import { useEffect, useMemo, useState } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { Backpack, Boxes, Cable, ChevronRight, CircleHelp, Code2, FileText, FolderOpen, Layers, LoaderCircle, RefreshCw, Search, Settings2, ShieldCheck, Terminal, Workflow, X } from "lucide-react";
import type { Resource, ResourceKind, ScanRequest, Snapshot } from "./types";

const kinds: Record<ResourceKind, { label: string; icon: typeof Boxes }> = {
  skill: { label: "스킬", icon: Boxes }, rule: { label: "룰", icon: FileText },
  tool: { label: "Tools", icon: Terminal }, hook: { label: "Hooks", icon: Workflow },
  mcp: { label: "MCP", icon: Cable }, plugin: { label: "플러그인", icon: Layers },
  setting: { label: "설정", icon: Settings2 },
};
const statusLabel: Record<string, string> = { present: "파일 발견", configured: "설정 등록", enabled: "활성 설정", disabled: "비활성 설정" };
const agents = ["codex", "claude", "antigravity", "opencode"];
const labels: Record<string, string> = { codex: "Codex", claude: "Claude Code", antigravity: "Antigravity", opencode: "OpenCode" };

export default function App() {
  const [snapshot, setSnapshot] = useState<Snapshot | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [agentId, setAgentId] = useState("all");
  const [kind, setKind] = useState("all");
  const [query, setQuery] = useState("");
  const [showBundled, setShowBundled] = useState(false);
  const [selected, setSelected] = useState<Resource | null>(null);
  const [settings, setSettings] = useState(false);
  const [project, setProject] = useState("");
  const [roots, setRoots] = useState<Record<string, string>>({});
  const [request, setRequest] = useState<ScanRequest>({});
  const native = isTauri();

  async function refresh(next: ScanRequest = request) {
    if (!native) return;
    setBusy(true); setError(""); setSelected(null);
    try {
      setSnapshot(await invoke<Snapshot>("scan_inventory", { request: next }));
      setRequest(next);
    } catch (e) { setError(typeof e === "string" ? e : "탐지를 완료하지 못했습니다."); }
    finally { setBusy(false); }
  }
  useEffect(() => { void refresh(); }, []);

  const all = useMemo(() => snapshot?.agents.flatMap(a => a.resources) ?? [], [snapshot]);
  const managed = useMemo(() => all.filter(r => showBundled || r.origin !== "bundled"), [all, showBundled]);
  const bundledCount = all.filter(r => r.origin === "bundled").length;
  const visible = useMemo(() => managed.filter(r => (agentId === "all" || r.agentId === agentId)
    && (kind === "all" || r.kind === kind) && [r.name, r.path, labels[r.agentId], r.scope].some(s => s.toLowerCase().includes(query.toLowerCase()))), [managed, agentId, kind, query]);
  const warnings = snapshot?.agents.flatMap(a => a.warnings.map(w => ({ name: a.name, message: w }))) ?? [];
  const foundAgents = snapshot?.agents.filter(a => a.executable || a.resources.length).length ?? 0;

  function configure() {
    const next: ScanRequest = { roots: Object.fromEntries(Object.entries(roots).filter(([,v]) => v.trim()).map(([k,v]) => [k,v.trim()])) };
    if (project.trim()) next.projectPath = project.trim();
    setSettings(false); void refresh(next);
  }
  return <div className="shell">
    <aside className="sidebar">
      <a className="brand" href="#" aria-label="Backpack 홈" onClick={e => { e.preventDefault(); setAgentId("all"); setKind("all"); }}>
        <span className="brand-icon"><Backpack size={23}/></span><span>Backpack<small>내 에이전트 환경, 어디서든</small></span>
      </a>
      <div className="nav-label">WORKSPACE</div>
      <button className={"nav-item " + (kind === "all" ? "active" : "")} onClick={() => { setKind("all"); setSelected(null); }}><Boxes size={18}/>에이전트 탐지<span className="nav-count">{managed.length}</span></button>
      <button className={"nav-item " + (kind === "tool" ? "active" : "")} onClick={() => { setKind("tool"); setSelected(null); }}><Terminal size={18}/>커스텀 Tools</button>
      <button className={"nav-item " + (kind === "hook" ? "active" : "")} onClick={() => { setKind("hook"); setSelected(null); }}><Workflow size={18}/>Hooks</button>
      <button className={"nav-item " + (kind === "mcp" ? "active" : "")} onClick={() => { setKind("mcp"); setSelected(null); }}><Cable size={18}/>MCP 연결</button>
      <div className="sidebar-note"><Layers size={19}/><strong>하나의 구성, 여러 PC</strong><p>라이브러리와 클라우드 동기화는 다음 단계에서 연결됩니다.</p><span className="pill">개발 중</span></div>
      <div className="sidebar-bottom"><button className="nav-item" onClick={() => setSettings(true)}><Settings2 size={18}/>탐지 경로 설정</button><div className="device"><span className="device-dot"/><span>이 PC<small>{snapshot?.platform ?? "로컬 데스크톱"} · 읽기 전용</small></span><span className="version">v0.1</span></div></div>
    </aside>
    <main className="main">
      <header className="topbar"><div className="breadcrumb">내 작업 공간<ChevronRight size={14}/><strong>에이전트 환경</strong></div><div className="read-only"><ShieldCheck size={15}/>읽기 전용 탐지</div></header>
      <div className="page">
        <section className="page-heading"><div><div className="eyebrow"><span/>LOCAL INVENTORY</div><h1>내 에이전트 환경</h1><p>이 PC에 있는 에이전트와 스킬, 도구, 연결 설정을 한눈에 확인하세요.</p></div><button className="primary" disabled={busy || !native} onClick={() => void refresh()}>{busy ? <LoaderCircle className="spin" size={17}/> : <RefreshCw size={17}/>} {busy ? "탐지 중" : "다시 탐지"}</button></section>
        {!native && <div className="notice"><CircleHelp size={18}/><div><strong>브라우저 미리보기</strong><p>실제 PC 탐지는 데스크톱 앱에서 사용할 수 있습니다. 예시 설치 상태를 표시하지 않습니다.</p></div></div>}
        {error && <div className="notice error" role="alert"><CircleHelp size={18}/><div><strong>탐지를 완료하지 못했습니다</strong><p>{error}</p><button className="text-button" onClick={() => setSettings(true)}>경로 설정 확인</button></div></div>}
        <div className="stats">
          <Stat label="발견한 에이전트" value={snapshot ? foundAgents : "—"} suffix="/ 4" icon={Code2}/>
          <Stat label="표시 대상 리소스" value={snapshot ? managed.length : "—"} suffix="개" icon={Boxes}/>
          <Stat label="Tools · Hooks · MCP" value={snapshot ? managed.filter(r => ["tool", "hook", "mcp"].includes(r.kind)).length : "—"} suffix="개" icon={Workflow}/>
          <Stat label="확인이 필요한 경로" value={snapshot ? warnings.length : "—"} suffix="개" icon={FolderOpen}/>
        </div>
        <div className="section-heading"><h2>에이전트</h2><span>{snapshot ? "마지막 탐지 " + new Date(snapshot.scannedAt * 1000).toLocaleTimeString("ko-KR") : "탐지 결과 대기 중"}</span></div>
        <div className="agent-grid">{agents.map(id => {
          const a = snapshot?.agents.find(a => a.id === id); const found = !!a?.executable;
          return <button key={id} className={"agent-card " + (agentId === id ? "selected" : "")} onClick={() => { setAgentId(agentId === id ? "all" : id); setSelected(null); }}>
            <div className="agent-top"><span className={"agent-logo " + id}>{id === "claude" ? "✳" : id === "antigravity" ? "△" : id === "opencode" ? ">_" : "⌘"}</span><span className={"agent-state " + (found ? "found" : "")}>{!a ? "대기" : found ? "실행 파일 발견" : a.resources.length ? "설정만 발견" : "미발견"}</span></div>
            <strong>{labels[id]}</strong><div className="agent-summary"><span>{a ? a.resources.filter(r => showBundled || r.origin !== "bundled").length + "개 리소스" : "아직 탐지하지 않음"}</span><ChevronRight size={16}/></div>
          </button>;
        })}</div>
        <section className="inventory">
          <div className="inventory-heading"><div><h2>로컬 리소스 <span>{visible.length}</span></h2><p>공유 저장소에 없는 항목도 함께 표시합니다.</p></div><label className="search"><Search size={16}/><input aria-label="리소스 검색" placeholder="이름, 경로, 에이전트 검색" value={query} onChange={e => setQuery(e.target.value)}/>{query && <button aria-label="검색 초기화" onClick={() => setQuery("")}><X size={14}/></button>}</label></div>
          <div className="filters"><button className={kind === "all" ? "chosen" : ""} onClick={() => setKind("all")}>전체</button>{Object.entries(kinds).map(([id,item]) => <button key={id} className={kind === id ? "chosen" : ""} onClick={() => setKind(id)}>{item.label}</button>)}{agentId !== "all" && <button className="agent-filter" onClick={() => setAgentId("all")}>{labels[agentId]}<X size={12}/></button>}</div>
          <label className="bundled-toggle"><input type="checkbox" checked={showBundled} onChange={e => { setShowBundled(e.target.checked); setSelected(null); }}/><span>기본 제공 항목 표시</span><small>{bundledCount ? bundledCount + "개 · 원본 수정 여부는 미확인" : "확인된 기본 항목만 숨깁니다"}</small></label>
          <div className="resource-table">
            <div className="table-head"><span>리소스 / 위치</span><span>에이전트</span><span>적용 범위</span><span>상태</span><span/></div>
            {visible.map(r => { const Icon = kinds[r.kind]?.icon ?? Boxes; return <button className={"resource-row " + (selected?.id === r.id ? "row-selected" : "")} key={r.id} onClick={() => setSelected(r)}>
              <div className="resource-name"><span className={"resource-icon " + r.kind}><Icon size={18}/></span><span><strong>{r.name}<small className="kind-label">{kinds[r.kind]?.label}</small></strong><small className="path" title={r.path}>{r.path}</small></span></div><span className="row-agent">{labels[r.agentId]}</span><span className="scope">{r.scope}</span><span className={"status " + r.status}>{statusLabel[r.status] ?? r.status}</span><ChevronRight size={15}/></button>; })}
            {!visible.length && <div className="empty">{busy ? <LoaderCircle className="spin" size={30}/> : <FolderOpen size={32}/>}<h3>{busy ? "로컬 환경을 확인하고 있어요" : snapshot ? "표시할 리소스가 없습니다" : "내 환경을 확인할 준비가 됐어요"}</h3><p>{snapshot ? "필터를 바꾸거나 탐지 경로에 프로젝트·설정 폴더를 추가하세요." : native ? "설정 파일과 알려진 경로를 확인합니다." : "데스크톱 앱으로 실행하면 실제 로컬 목록이 여기에 나타납니다."}</p>{snapshot && <button className="secondary" onClick={() => setSettings(true)}>탐지 경로 설정</button>}</div>}
          </div>
          <footer className="inventory-footer"><ShieldCheck size={14}/>설정은 변경하지 않습니다. 연결과 실제 실행 상태는 아직 검사하지 않습니다.</footer>
        </section>
        {warnings.length > 0 && <details className="warnings"><summary>일부 경로를 확인하지 못했습니다 · {warnings.length}개</summary>{warnings.map((w,i) => <p key={i}><strong>{w.name}</strong> {w.message}</p>)}</details>}
        <div className="bottom-note"><span className="device-dot"/><span>실제 파일에서 확인한 목록</span><span className="separator">·</span><span>인증 값과 설정 원문은 표시하지 않습니다.</span></div>
      </div>
    </main>
    {selected && <div className="overlay" onClick={() => setSelected(null)}><aside className="detail-panel" aria-label="리소스 상세" onClick={e => e.stopPropagation()}><div className="panel-top"><span>리소스 상세</span><button aria-label="상세 닫기" onClick={() => setSelected(null)}><X size={20}/></button></div><span className="detail-icon">{(() => { const Icon = kinds[selected.kind].icon; return <Icon size={28}/>; })()}</span><h2>{selected.name}</h2><p className="detail-sub">{labels[selected.agentId]} · {kinds[selected.kind].label}</p><dl className="detail-list">{[{ label: "발견 위치", value: selected.path }, { label: "제공 구분", value: selected.origin === "bundled" ? "기본 제공" : selected.origin === "user" ? "사용자 구성" : "미확인" }, { label: "적용 범위", value: selected.scope }, { label: "설정 상태", value: statusLabel[selected.status] ?? selected.status }, { label: "근거", value: selected.source }, ...selected.details].map((d,i) => <div key={i}><dt>{d.label}</dt><dd>{d.value}</dd></div>)}</dl><div className="notice compact"><ShieldCheck size={18}/><p>이 항목을 발견했습니다. 현재 세션에서 로드·실행되었는지는 아직 직접 조회하지 않았습니다.</p></div></aside></div>}
    {settings && <div className="overlay modal-overlay" onClick={() => setSettings(false)}><section className="settings-modal" role="dialog" aria-modal="true" aria-label="탐지 경로 설정" onClick={e => e.stopPropagation()}><div className="panel-top"><h2>탐지 경로 설정</h2><button aria-label="설정 닫기" onClick={() => setSettings(false)}><X size={20}/></button></div><p>기본 사용자 경로 외에 프로젝트와 에이전트 설정 루트를 지정할 수 있습니다. 절대 경로를 입력하세요.</p><label className="field">프로젝트 폴더<input value={project} onChange={e => setProject(e.target.value)} placeholder="예: C:\dev\my-project 또는 /Users/me/project"/></label><div className="field-divider">에이전트 설정 루트 <span>비워두면 기본값 사용</span></div>{agents.map(id => <label className="field" key={id}>{labels[id]}<input value={roots[id] ?? ""} onChange={e => setRoots({ ...roots, [id]: e.target.value })} placeholder={snapshot?.agents.find(a => a.id === id)?.configRoots[0] ?? "기본 설정 경로"}/></label>)}<div className="modal-actions"><button className="secondary" onClick={() => setSettings(false)}>닫기</button><button className="primary" disabled={!native || busy} onClick={configure}>이 경로로 탐지</button></div></section></div>}
  </div>;
}
function Stat({ label, value, suffix, icon: Icon }: { label: string; value: string | number; suffix: string; icon: typeof Boxes }) {
  return <div className="stat"><div className="stat-label">{label}<Icon size={17}/></div><div className="stat-value">{value}<span>{suffix}</span></div></div>;
}
