import { useEffect, useMemo, useState } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { Backpack, Boxes, Cable, ChevronRight, CircleHelp, Code2, FileText, FolderOpen, Layers, LoaderCircle, RefreshCw, Search, Settings2, ShieldCheck, Terminal, Workflow, X } from "lucide-react";
import { readCustomAgents, saveCustomAgents } from "./agentPreferences";
import type { CliInspection, CustomAgent, CodexInspection, Resource, ResourceKind, ScanRequest, Snapshot } from "./types";
import CliPanel from "./CliPanel";
import CodexPanel from "./CodexPanel";
import Settings, { type SettingsTab } from "./Settings";
import { applyTheme, readPreferences, savePreferences } from "./preferences";

const kinds: Record<ResourceKind, { label: string; icon: typeof Boxes }> = {
  skill: { label: "스킬", icon: Boxes }, rule: { label: "룰", icon: FileText },
  tool: { label: "Tools", icon: Terminal }, hook: { label: "Hooks", icon: Workflow },
  mcp: { label: "MCP", icon: Cable }, plugin: { label: "플러그인", icon: Layers },
  setting: { label: "설정", icon: Settings2 },
};
const statusLabel: Record<string, string> = { present: "파일 발견", configured: "설정 등록", enabled: "활성 설정", disabled: "비활성 설정", connected:"연결됨", failed:"연결 실패", authenticationRequired:"인증 필요", pendingApproval:"승인 대기", clientRegistrationRequired:"클라이언트 등록 필요", notStarted:"미시작", notConfigured:"설정 미완료" };

export default function App() {
  const [customAgents, setCustomAgents] = useState(readCustomAgents);
  const [showUndetected, setShowUndetected] = useState(false);
  const [snapshot, setSnapshot] = useState<Snapshot | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [inspection, setInspection] = useState<CodexInspection | null>(null);
  const [inspectionError, setInspectionError] = useState("");
  const [cliReports,setCliReports] = useState<Record<string,CliInspection>>({});
  const [cliErrors,setCliErrors] = useState<Record<string,string>>({});
  const [inspectionId, setInspectionId] = useState<string | null>(null);
  const inspectionBusy = inspectionId !== null;
  const [agentId, setAgentId] = useState("all");
  const [kind, setKind] = useState("all");
  const [query, setQuery] = useState("");
  const [preferences, setPreferences] = useState(readPreferences);
  const [saveError, setSaveError] = useState(false);
  const showBundled = preferences.showBundled;
  const [settingsTab, setSettingsTab] = useState<SettingsTab>("general");
  const [selected, setSelected] = useState<Resource | null>(null);
  const [settings, setSettings] = useState(false);
  const [project, setProject] = useState("");
  const [roots, setRoots] = useState<Record<string, string>>({});
  const [request, setRequest] = useState<ScanRequest>(()=>({customAgents:readCustomAgents()}));
  const native = isTauri();

  async function refresh(next: ScanRequest = request) {
    if (!native) return;
    setBusy(true); setError(""); setSelected(null); setInspection(null); setInspectionError(""); setCliReports({}); setCliErrors({});
    try {
      setSnapshot(await invoke<Snapshot>("scan_inventory", { request: next }));
      setRequest(next);
      return true;
    } catch (e) { setError(typeof e === "string" ? e : "탐지를 완료하지 못했습니다."); return false; }
    finally { setBusy(false); }
  }
  async function registerAgents(next:CustomAgent[]) {
    const scanned = await refresh({...request,customAgents:next});
    if (!scanned) return false;
    setCustomAgents(next);
    if (!saveCustomAgents(next)) { setError("기타 에이전트를 저장하지 못했습니다. 현재 실행 중에만 적용됩니다."); return false; }
    if (agentId.startsWith("custom-") && !next.some(a=>a.id===agentId)) setAgentId("all");
    setSettings(false);
    return true;
  }
  async function inspect(includeMcp: boolean) {
    if (!native || busy || inspectionBusy) return;
    const requestId = crypto.randomUUID();
    setInspectionId(requestId); setInspectionError(""); setSelected(null);
    try { setInspection(await invoke<CodexInspection>("inspect_codex_inventory", { request, includeMcp, requestId })); }
    catch (e) { setInspectionError(typeof e === "string" ? e : "Codex 조회를 완료하지 못했습니다."); }
    finally { setInspectionId(null); }
  }
  async function inspectCli(id:string) {
    if (!native || busy || inspectionBusy) return;
    const requestId=crypto.randomUUID(); setInspectionId(requestId); setCliErrors(previous=>({...previous,[id]:""}));
    try { const result=await invoke<CliInspection>("inspect_cli_inventory",{request,agentId:id,requestId}); setCliReports(previous=>({...previous,[id]:result})); }
    catch(e) { setCliErrors(previous=>({...previous,[id]:typeof e==="string"?e:"조회하지 못했습니다."})); }
    finally {setInspectionId(null);}
  }
  async function cancelInspection() {
    if (!inspectionId) return;
    try { await invoke("cancel_inspection", { requestId: inspectionId }); }
    catch { setInspectionError("취소 요청을 보내지 못했습니다. 조회는 제한 시간 후 종료됩니다."); }
  }
  useEffect(() => { if (preferences.scanOnStartup) void refresh(); }, []);
  useEffect(() => { applyTheme(preferences.theme); setSaveError(!savePreferences(preferences)); }, [preferences]);
  function openSettings(tab: SettingsTab) { setSettingsTab(tab); setSettings(true); }

  const labels = Object.fromEntries((snapshot?.agents ?? []).map(a=>[a.id,a.name]));
  const displayedAgents = snapshot?.agents.filter(a=>showUndetected || a.custom || a.executable || a.resources.length) ?? [];
  const currentAgent = snapshot?.agents.find(a=>a.id===agentId);
  const all = useMemo(() => {
    const files = snapshot?.agents.flatMap(a => a.resources) ?? [];
    const mergeCli = (resources:Resource[]) => resources.map(resource=>{
      const report=cliReports[resource.agentId];
      if (resource.kind!=="mcp" || !report || !["success","partial"].includes(report.query.status)) return resource;
      const server=report.servers.find(s=>s.name===resource.name);
      if (!server) return {...resource,details:[...resource.details,{label:"직접 조회",value:"CLI 목록에서 확인하지 못함 · 비활성으로 판단하지 않음"}]};
      return {...resource,status:server.status,source:"파일 탐지 + CLI mcp list",details:[...resource.details,{label:"파일 기준 상태",value:statusLabel[resource.status]??resource.status},{label:"직접 조회 문맥",value:report.cwd},{label:"직접 조회 시간",value:new Date(report.observedAt*1000).toLocaleString("ko-KR")}]};
    });
    if (!inspection || !["success", "partial"].includes(inspection.skillsQuery.status)) return mergeCli(files);
    const pathKey = (path: string) => snapshot?.platform === "windows" ? path.toLowerCase() : path;
    const reported = new Map(inspection.skills.map(r => [pathKey(r.path), r]));
    const merged = files.map(resource => {
      if (resource.agentId !== "codex" || resource.kind !== "skill") return resource;
      const direct = reported.get(pathKey(resource.path));
      if (!direct) return { ...resource, details: [...resource.details, { label: "직접 조회", value: "파일에서만 발견 · 비활성으로 판단하지 않음" }] };
      reported.delete(pathKey(resource.path));
      return { ...direct, details: [...resource.details, { label: "파일 탐지 근거", value: resource.source }, ...direct.details] };
    });
    return mergeCli([...merged, ...reported.values()]);
  }, [snapshot, inspection, cliReports]);
  const managed = useMemo(() => all.filter(r => showBundled || r.origin !== "bundled"), [all, showBundled]);
  const bundledCount = all.filter(r => r.origin === "bundled").length;
  const visible = useMemo(() => managed.filter(r => (agentId === "all" || r.agentId === agentId)
    && (kind === "all" || r.kind === kind) && [r.name, r.path, labels[r.agentId], r.scope].some(s => s.toLowerCase().includes(query.toLowerCase()))), [managed, agentId, kind, query]);
  const warnings = snapshot?.agents.flatMap(a => a.warnings.map(w => ({ name: a.name, message: w }))) ?? [];
  const foundAgents = snapshot?.agents.filter(a => a.executable || a.resources.length).length ?? 0;

  function configure() {
    const next: ScanRequest = { customAgents, roots: Object.fromEntries(Object.entries(roots).filter(([,v]) => v.trim()).map(([k,v]) => [k,v.trim()])) };
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
      <div className="sidebar-bottom"><button className="nav-item" onClick={() => openSettings("general")}><Settings2 size={18}/>앱 설정</button><div className="device"><span className="device-dot"/><span>이 PC<small>{snapshot?.platform ?? "로컬 데스크톱"} · 읽기 전용</small></span><span className="version">v0.1</span></div></div>
    </aside>
    <main className="main">
      <header className="topbar"><div className="breadcrumb">내 작업 공간<ChevronRight size={14}/><strong>에이전트 환경</strong></div><div className="read-only"><ShieldCheck size={15}/>읽기 전용 탐지</div></header>
      <div className="page">
        <section className="page-heading"><div><div className="eyebrow"><span/>LOCAL INVENTORY</div><h1>내 에이전트 환경</h1><p>이 PC에 있는 에이전트와 스킬, 도구, 연결 설정을 한눈에 확인하세요.</p></div><button className="primary" disabled={busy || inspectionBusy || !native} onClick={() => void refresh()}>{busy ? <LoaderCircle className="spin" size={17}/> : <RefreshCw size={17}/>} {busy ? "탐지 중" : "다시 탐지"}</button></section>
        {!native && <div className="notice"><CircleHelp size={18}/><div><strong>브라우저 미리보기</strong><p>실제 PC 탐지는 데스크톱 앱에서 사용할 수 있습니다. 예시 설치 상태를 표시하지 않습니다.</p></div></div>}
        {error && <div className="notice error" role="alert"><CircleHelp size={18}/><div><strong>탐지를 완료하지 못했습니다</strong><p>{error}</p><button className="text-button" onClick={() => openSettings("paths")}>경로 설정 확인</button></div></div>}
        {inspectionBusy && <div className="notice" role="status"><LoaderCircle size={18} className="spin"/><div><strong>직접 조회 중</strong><p>45초 안에 완료하거나 종료합니다.</p><button className="text-button" onClick={()=>void cancelInspection()}>조회 취소</button></div></div>}
        <div className="stats">
          <Stat label="발견한 에이전트" value={snapshot ? foundAgents : "—"} suffix={snapshot ? `/ ${snapshot.agents.length} 지원·등록` : "개"} icon={Code2}/>
          <Stat label="표시 대상 리소스" value={snapshot ? managed.length : "—"} suffix="개" icon={Boxes}/>
          <Stat label="Tools · Hooks · MCP" value={snapshot ? managed.filter(r => ["tool", "hook", "mcp"].includes(r.kind)).length : "—"} suffix="개" icon={Workflow}/>
          <Stat label="확인이 필요한 경로" value={snapshot ? warnings.length : "—"} suffix="개" icon={FolderOpen}/>
        </div>
        <div className="section-heading"><h2>발견한 에이전트</h2><button className="text-button" onClick={()=>openSettings("agents")}>기타 AI 등록</button></div>
        <label className="bundled-toggle"><input type="checkbox" checked={showUndetected} onChange={e=>setShowUndetected(e.target.checked)}/><span>설치 미확인 에이전트도 표시</span></label>
        <div className="agent-grid">{displayedAgents.map(a => {
          const id = a.id; const found = !!a.executable;
          return <button key={id} disabled={inspectionBusy} className={"agent-card " + (agentId === id ? "selected" : "")} onClick={() => { setAgentId(agentId === id ? "all" : id); setSelected(null); }}>
            <div className="agent-top"><span className={"agent-logo " + id}>{a.name.slice(0,2)}</span><span className={"agent-state " + (found ? "found" : "")}>{found ? "실행 파일 발견" : a.resources.length ? "설정만 발견" : "설치 미확인"}</span></div>
            <strong>{a.name}</strong><div className="agent-summary"><span>{managed.filter(r => r.agentId === id).length}개 리소스 · {a.custom ? "사용자 등록" : a.inspection !== "file" ? "직접 조회 지원" : "파일 탐지"}</span><ChevronRight size={16}/></div>
          </button>;
        })}</div>
        {!displayedAgents.length && <p className="inspection-hint">아직 발견한 에이전트가 없습니다. 다시 탐지하거나 기타 AI를 등록하세요.</p>}
        {currentAgent ? <section className="agent-section" aria-label={`${currentAgent.name} 설정`}>
          <div className="inspection-heading"><div><h2>{currentAgent.name} 설정</h2><p>스킬·룰·Tools·Hooks·MCP·플러그인은 아래 목록에서 종류별로 확인하세요.</p></div><button className="secondary" disabled={busy || inspectionBusy || !native} onClick={()=>void refresh()}>파일 설정 다시 확인</button></div>
          <dl className="detail-list"><div><dt>실행 파일</dt><dd>{currentAgent.executable ?? "발견하지 못함"}</dd></div><div><dt>설정 경로</dt><dd>{currentAgent.configRoots.join(" · ")}</dd></div><div><dt>파일 확인 시간</dt><dd>{snapshot && new Date(snapshot.scannedAt*1000).toLocaleString("ko-KR")}</dd></div></dl>
          {currentAgent.inspection === "app-server" ? <CodexPanel result={inspection} snapshot={snapshot} busy={inspectionBusy} disabled={!native || busy || !snapshot} error={inspectionError} showBundled={showBundled} onInspect={includeMcp => void inspect(includeMcp)} onCancel={() => void cancelInspection()}/> : currentAgent.inspection === "mcp-cli" ? <CliPanel key={currentAgent.id} agent={currentAgent} result={cliReports[currentAgent.id]??null} error={cliErrors[currentAgent.id]??""} busy={inspectionBusy} disabled={!native||busy||!currentAgent.executable} showBundled={showBundled} onInspect={()=>void inspectCli(currentAgent.id)} onCancel={()=>void cancelInspection()}/> : <p className="inspection-hint">직접 조회 어댑터는 아직 지원하지 않습니다. 현재는 설정 파일에서 확인한 결과를 표시하며, 실제 실행·연결 상태는 미확인입니다.</p>}
          <button className="text-button" onClick={()=>openSettings(currentAgent.custom ? "agents" : "paths")}>{currentAgent.custom ? "등록 설정 변경" : "탐지 경로 변경"}</button>
        </section> : <p className="inspection-hint agent-selection-hint">에이전트를 선택하면 해당 AI의 설정 경로와 조회 기능이 열립니다.</p>}
        <section className="inventory">
          <div className="inventory-heading"><div><h2>로컬 리소스 <span>{visible.length}</span></h2><p>공유 저장소에 없는 항목도 함께 표시합니다.</p></div><label className="search"><Search size={16}/><input aria-label="리소스 검색" placeholder="이름, 경로, 에이전트 검색" value={query} onChange={e => setQuery(e.target.value)}/>{query && <button aria-label="검색 초기화" onClick={() => setQuery("")}><X size={14}/></button>}</label></div>
          <div className="filters"><button className={kind === "all" ? "chosen" : ""} onClick={() => setKind("all")}>전체</button>{Object.entries(kinds).map(([id,item]) => <button key={id} className={kind === id ? "chosen" : ""} onClick={() => setKind(id)}>{item.label}</button>)}{agentId !== "all" && <button className="agent-filter" onClick={() => setAgentId("all")}>{labels[agentId]}<X size={12}/></button>}</div>
          <label className="bundled-toggle"><input type="checkbox" checked={showBundled} onChange={e => { setPreferences({ ...preferences, showBundled: e.target.checked }); setSelected(null); }}/><span>기본 제공 항목 표시</span><small>{bundledCount ? bundledCount + "개 · 원본 수정 여부는 미확인" : "확인된 기본 항목만 숨깁니다"}</small></label>
          <div className="resource-table">
            <div className="table-head"><span>리소스 / 위치</span><span>에이전트</span><span>적용 범위</span><span>상태</span><span/></div>
            {visible.map(r => { const Icon = kinds[r.kind]?.icon ?? Boxes; return <button className={"resource-row " + (selected?.id === r.id ? "row-selected" : "")} key={r.id} onClick={() => setSelected(r)}>
              <div className="resource-name"><span className={"resource-icon " + r.kind}><Icon size={18}/></span><span><strong>{r.name}<small className="kind-label">{kinds[r.kind]?.label}</small></strong><small className="path" title={r.path}>{r.path}</small></span></div><span className="row-agent">{labels[r.agentId]}</span><span className="scope">{r.scope}</span><span className={"status " + r.status}>{statusLabel[r.status] ?? r.status}</span><ChevronRight size={15}/></button>; })}
            {!visible.length && <div className="empty">{busy ? <LoaderCircle className="spin" size={30}/> : <FolderOpen size={32}/>}<h3>{busy ? "로컬 환경을 확인하고 있어요" : snapshot ? "표시할 리소스가 없습니다" : "내 환경을 확인할 준비가 됐어요"}</h3><p>{snapshot ? "필터를 바꾸거나 탐지 경로에 프로젝트·설정 폴더를 추가하세요." : native ? "설정 파일과 알려진 경로를 확인합니다." : "데스크톱 앱으로 실행하면 실제 로컬 목록이 여기에 나타납니다."}</p>{snapshot && <button className="secondary" onClick={() => openSettings("paths")}>탐지 경로 설정</button>}</div>}
          </div>
          <footer className="inventory-footer"><ShieldCheck size={14}/>설정은 변경하지 않습니다. 항목 상세에서 조회 근거와 문맥을 확인하세요.</footer>
        </section>
        {warnings.length > 0 && <details className="warnings"><summary>일부 경로를 확인하지 못했습니다 · {warnings.length}개</summary>{warnings.map((w,i) => <p key={i}><strong>{w.name}</strong> {w.message}</p>)}</details>}
        <div className="bottom-note"><span className="device-dot"/><span>{inspection ? "파일 탐지와 Codex 직접 조회 결과" : "실제 파일에서 확인한 목록"}</span><span className="separator">·</span><span>인증 값과 설정 원문은 표시하지 않습니다.</span></div>
      </div>
    </main>
    {selected && <div className="overlay" onClick={() => setSelected(null)}><aside className="detail-panel" aria-label="리소스 상세" onClick={e => e.stopPropagation()}><div className="panel-top"><span>리소스 상세</span><button aria-label="상세 닫기" onClick={() => setSelected(null)}><X size={20}/></button></div><span className="detail-icon">{(() => { const Icon = kinds[selected.kind].icon; return <Icon size={28}/>; })()}</span><h2>{selected.name}</h2><p className="detail-sub">{labels[selected.agentId]} · {kinds[selected.kind].label}</p><dl className="detail-list">{[{ label: "발견 위치", value: selected.path }, { label: "제공 구분", value: selected.origin === "bundled" ? "기본 제공" : selected.origin === "user" ? "사용자 구성" : "미확인" }, { label: "적용 범위", value: selected.scope }, { label: "확인 상태", value: statusLabel[selected.status] ?? selected.status }, { label: "근거", value: selected.source }, ...selected.details].map((d,i) => <div key={i}><dt>{d.label}</dt><dd>{d.value}</dd></div>)}</dl><div className="notice compact"><ShieldCheck size={18}/><p>파일 탐지 또는 별도 조회 프로세스의 결과입니다. 현재 대화에서의 로드·실행 여부와 다를 수 있습니다.</p></div></aside></div>}
    {settings && <Settings customAgents={customAgents} onCustomAgents={registerAgents} initialTab={settingsTab} preferences={preferences} saveError={saveError} onPreferences={setPreferences} onClose={() => setSettings(false)} project={project} onProject={setProject} roots={roots} onRoots={setRoots} snapshot={snapshot} canScan={native && !busy && !inspectionBusy} onScan={configure}/> }
  </div>;
}
function Stat({ label, value, suffix, icon: Icon }: { label: string; value: string | number; suffix: string; icon: typeof Boxes }) {
  return <div className="stat"><div className="stat-label">{label}<Icon size={17}/></div><div className="stat-value">{value}<span>{suffix}</span></div></div>;
}
