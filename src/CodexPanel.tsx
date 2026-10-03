import { useState } from "react";
import { Cable, LoaderCircle, RefreshCw, X } from "lucide-react";
import type { CodexInspection, QueryState, Snapshot } from "./types";
interface Props { result: CodexInspection | null; snapshot: Snapshot | null; busy: boolean; disabled: boolean; error: string; showBundled: boolean; onInspect: (includeMcp: boolean) => void; onCancel: () => void }
const runtime: Record<string,string> = { notStarted:"미시작", starting:"시작 중", connected:"연결됨", authenticationRequired:"인증 필요", failed:"실패", cancelled:"취소됨", disabled:"비활성" };
const auth: Record<string,string> = { unknown:"미확인", unsupported:"인증 방식 보고 없음", notLoggedIn:"로그인 필요", bearerToken:"토큰 방식", oAuth:"OAuth 방식" };
const queryLabel: Record<string,string> = {success:"조회 완료",partial:"일부 확인",error:"조회 실패",unsupported:"버전 미지원",skipped:"조회 안 함"};
function Query({label,state}:{label:string;state:QueryState}) { return <div className={"query-state " + state.status}><strong>{label}</strong><span>{queryLabel[state.status]}</span><small>{state.message}</small></div>; }
export default function CodexPanel(props:Props) {
  const [includeMcp,setIncludeMcp] = useState(false);
  const report = props.result;
  const servers = report?.mcpServers.filter(s => props.showBundled || s.origin !== "bundled") ?? [];
  const reportedSkills = report?.skills.filter(s => props.showBundled || s.origin !== "bundled") ?? [];
  const fileSkills = props.snapshot?.agents.find(a => a.id === "codex")?.resources.filter(r => r.kind === "skill" && (props.showBundled || r.origin !== "bundled")) ?? [];
  const pathKey = (path: string) => props.snapshot?.platform === "windows" ? path.toLowerCase() : path;
  const fileOnly = report && ["success","partial"].includes(report.skillsQuery.status) ? fileSkills.filter(file => !report.skills.some(skill => pathKey(skill.path) === pathKey(file.path))) : [];
  return <section className="codex-inspection" aria-label="Codex 직접 조회">
    <div className="inspection-heading"><div><h2>Codex 직접 조회</h2><p>Codex가 보고한 설정과 스킬을 파일 탐지 결과와 함께 확인합니다.</p></div><div className="inspection-actions">{props.busy ? <button className="secondary" onClick={props.onCancel}><X size={15}/>조회 취소</button> : null}<button className="secondary" disabled={props.disabled || props.busy} onClick={() => props.onInspect(includeMcp)}>{props.busy ? <LoaderCircle size={15} className="spin"/> : <RefreshCw size={15}/>} {props.busy ? "조회 중" : "Codex 조회"}</button></div></div>
    <label className="inspection-option"><input type="checkbox" checked={includeMcp} disabled={props.busy} onChange={e => setIncludeMcp(e.target.checked)}/>MCP 상태·도구 목록도 조회</label>
    <p className="inspection-hint">{includeMcp ? "MCP 조회는 등록된 서버 실행과 외부 서비스 접속을 유발할 수 있습니다. 개별 도구는 호출하지 않습니다." : "별도 로컬 조회 프로세스를 사용합니다. 모델 대화나 도구 호출은 하지 않습니다."}</p>
    {props.error && <p className="inspection-error" role="alert">{props.error}</p>}
    {report && <>
      <div className="inspection-context"><span>Codex {report.version ?? "버전 미확인"}</span><span>{new Date(report.observedAt * 1000).toLocaleTimeString("ko-KR")} 조회</span><small>{report.context}</small><code title={report.cwd}>조회 경로: {report.cwd}</code><code title={report.codexHome}>설정 경로: {report.codexHome}</code></div>
      <div className="query-grid"><Query label="적용 설정" state={report.configQuery}/><Query label="스킬" state={report.skillsQuery}/><Query label="MCP" state={report.mcpQuery}/></div>
      {report.settings.length > 0 && <dl className="inspection-settings">{report.settings.map(d => <div key={d.label}><dt>{d.label}</dt><dd>{d.value}</dd></div>)}</dl>}
      {["success","partial"].includes(report.skillsQuery.status) && <p className="inspection-comparison">파일에서 발견한 스킬 {fileSkills.length}개 · Codex가 보고한 스킬 {reportedSkills.length}개{fileOnly.length ? ` · 파일에서만 발견 ${fileOnly.length}개 (비활성으로 판단하지 않음)` : ""}. 상세 화면에서 조회 근거를 확인할 수 있습니다.</p>}
      {["success","partial","error"].includes(report.mcpQuery.status) && report.mcpServers.length > 0 && <div className="mcp-inventory"><h3><Cable size={15}/>MCP 서버 <span>{servers.length}개 표시 / {report.mcpServers.length}개 조회</span></h3>{servers.map(server => <details className="mcp-result" key={server.name}><summary><strong>{server.name}</strong><span>{server.runtimeStatus ? runtime[server.runtimeStatus] ?? "미확인" : "실행 상태 미확인"}</span><span>도구 {server.toolNames.length}개</span></summary><p>인증: {auth[server.authStatus] ?? "미확인"}{server.pluginId ? ` · ${server.pluginId}` : ""}</p>{server.toolsError && <p className="inspection-error">도구 목록 조회에서 오류가 보고됐습니다.</p>}<p>도구 목록 반환만으로 현재 대화에서 호출할 수 있다고 판단하지 않습니다.</p><div className="mcp-tools">{server.toolNames.map(name => <code key={name}>{name}</code>)}</div></details>)}{!servers.length && <p>조회된 서버는 모두 기본 제공 항목입니다. 기본 제공 항목 표시를 켜면 확인할 수 있습니다.</p>}</div>}
    </>}
  </section>;
}
