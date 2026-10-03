import { useState } from "react";
import { LoaderCircle, RefreshCw, X } from "lucide-react";
import type { Agent, CliInspection } from "./types";
const statuses:Record<string,string>={connected:"연결됨",failed:"연결 실패",authenticationRequired:"인증 필요",clientRegistrationRequired:"클라이언트 등록 필요",pendingApproval:"승인 대기",disabled:"비활성",notStarted:"미시작",notConfigured:"설정 미완료"};
interface Props { agent:Agent; result:CliInspection|null; error:string; busy:boolean; disabled:boolean; showBundled:boolean; onInspect:()=>void; onCancel:()=>void }
export default function CliPanel(props:Props) {
  const [allowMcp,setAllowMcp]=useState(false); const report=props.result;
  const servers=report?.servers.filter(s=>props.showBundled||s.origin!=="bundled") ?? [];
  return <section className="codex-inspection" aria-label={`${props.agent.name} 직접 조회`}>
    <div className="inspection-heading"><div><h2>{props.agent.name} 직접 조회</h2><p>설치된 CLI의 mcp list 명령으로 MCP 상태를 확인합니다.</p></div><div className="inspection-actions">
      {props.busy && <button className="secondary" onClick={props.onCancel}><X size={15}/>조회 취소</button>}
      <button className="secondary" disabled={props.disabled||props.busy||!allowMcp} onClick={props.onInspect}>{props.busy?<LoaderCircle size={15} className="spin"/>:<RefreshCw size={15}/>} {props.busy?"조회 중":"MCP 직접 조회"}</button>
    </div></div>
    <label className="inspection-option"><input type="checkbox" checked={allowMcp} disabled={props.busy} onChange={e=>setAllowMcp(e.target.checked)}/>MCP 서버 실행·연결 검사를 포함해 조회</label>
    <p className="inspection-hint">등록된 MCP 서버 실행과 외부 서비스 접속을 유발할 수 있습니다. 모델이나 개별 도구는 호출하지 않습니다. 설정·스킬·룰은 파일 탐지 결과에서 확인하세요.</p>
    {props.error && <p className="inspection-error" role="alert">{props.error}</p>}
    {report && <>
      <div className="inspection-context"><span>버전 {report.version ?? "미확인"}</span><span>{new Date(report.observedAt*1000).toLocaleTimeString("ko-KR")} 조회</span><small>Backpack 별도 CLI 프로세스 · 현재 대화 세션과 별개</small><code>실행 파일: {report.executable}</code><code>조회 경로: {report.cwd}</code><code>설정 경로: {report.configRoot}</code></div>
      <p className={report.query.status==="error"||report.query.status==="unsupported"?"inspection-error":"inspection-hint"}>{report.query.status==="success"?"조회 완료":report.query.status==="partial"?"일부 확인":"조회 실패"} · {report.query.message}</p>
      {servers.map(s=><div className="mcp-result" key={s.name}><strong>{s.name}</strong><span> · {statuses[s.status]??"미확인"}</span></div>)}
      {report.query.status==="success"&&!report.servers.length&&<p className="inspection-hint">CLI가 등록된 MCP 서버가 없다고 보고했습니다.</p>}
      <p className="inspection-hint">이 조회에서는 개별 tool 목록을 제공하지 않습니다. URL·명령·인자·오류 원문은 표시하지 않습니다.</p>
    </>}
  </section>;
}
