import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { Agent, BackupReceipt, EditorData, McpDraft, PreviewResult, ScanRequest } from "./types";
interface Props { agent:Agent;request:ScanRequest;scannedAt:number;disabled:boolean;onBusy:(busy:boolean)=>void;onChanged:()=>void }
const actions={register:"새 MCP 등록",enable:"활성화",disable:"비활성화"};
export default function McpEditor(props:Props) {
  const [data,setData]=useState<EditorData>({targets:[],backups:[]});
  const [path,setPath]=useState("");const [action,setAction]=useState<McpDraft["action"]>("register");const [name,setName]=useState("");
  const [remote,setRemote]=useState(false);const [command,setCommand]=useState("");const [args,setArgs]=useState("[]");const [url,setUrl]=useState("");const [env,setEnv]=useState("");const [tokenEnv,setTokenEnv]=useState("");const [enabled,setEnabled]=useState(false);
  const [preview,setPreview]=useState<PreviewResult|null>(null);const [restore,setRestore]=useState<BackupReceipt|null>(null);
  const [busy,setBusy]=useState(false);const [error,setError]=useState("");const [message,setMessage]=useState("");const [loading,setLoading]=useState(false);
  const loadVersion=useRef(0);
  async function load() {const version=++loadVersion.current;setLoading(true);try{const next=await invoke<EditorData>("mcp_editor_data",{request:props.request,agentId:props.agent.id});if(version!==loadVersion.current)return;setData(next);setPath(previous=>next.targets.includes(previous)?previous:next.targets[0]??"");}catch(e){if(version===loadVersion.current)setError(typeof e==="string"?e:"설정 정보를 읽지 못했습니다.");}finally{if(version===loadVersion.current)setLoading(false);}}
  useEffect(()=>{setPreview(null);setRestore(null);void load();return()=>{loadVersion.current++;};},[props.agent.id,props.request,props.scannedAt]);
  function edit(update:()=>void) {update();setPreview(null);setRestore(null);setError("");setMessage("");}
  async function perform(work:()=>Promise<void>) {setBusy(true);props.onBusy(true);setError("");setMessage("");try{await work();}catch(e){setError(typeof e==="string"?e:"작업을 완료하지 못했습니다.");}finally{setBusy(false);props.onBusy(false);}}
  function draft():McpDraft {let parsed:unknown=[];if(action==="register"&&!remote){try{parsed=JSON.parse(args);}catch{throw new Error("인자는 JSON 문자열 배열로 입력하세요. 예: [\"server.py\"]");}if(!Array.isArray(parsed)||!parsed.every(v=>typeof v==="string")){throw new Error("인자는 JSON 문자열 배열이어야 합니다.");}}
    return {agentId:props.agent.id,path,name:name.trim(),action,command:action==="register"&&!remote?command.trim():"",args:parsed as string[],url:action==="register"&&remote?url.trim():"",envNames:remote?[]:env.split(",").map(s=>s.trim()).filter(Boolean),tokenEnv:remote?tokenEnv.trim():"",enabled};}
  async function review() {let next:McpDraft;try{next=draft();}catch(e){setError(e instanceof Error?e.message:"입력을 확인하세요.");return;}await perform(async()=>{setPreview(await invoke<PreviewResult>("preview_mcp_change",{request:props.request,draft:next}));});}
  async function apply() {if(!preview)return;const token=preview.token;setPreview(null);await perform(async()=>{await invoke("apply_mcp_change",{token});setMessage("백업 후 설정을 저장했습니다. 에이전트를 다시 시작하거나 설정을 다시 읽혀 적용하세요.");await load();props.onChanged();});}
  async function recover() {if(!restore)return;const backupId=restore.id;setRestore(null);setPreview(null);await perform(async()=>{await invoke("restore_mcp_backup",{request:props.request,agentId:props.agent.id,backupId});setMessage("적용 전 설정으로 복원했습니다.");await load();props.onChanged();});}
  const locked=props.disabled||busy||loading;const existing=props.agent.resources.filter(r=>r.kind==="mcp"&&r.path===path);
  return <section className="mcp-editor" aria-label={`${props.agent.name} MCP 설정 변경`}>
    <h3>MCP 설정 변경</h3><p className="inspection-hint">미리보기 확인 후 원본을 백업하고 저장합니다. 이 동작은 서버를 실행하지 않으며, 현재 대화의 연결을 즉시 해제하지 않습니다.</p>
    <fieldset disabled={locked} className="editor-fields"><legend>변경할 설정</legend>
      <label className="field">대상 파일<select value={path} onChange={e=>edit(()=>setPath(e.target.value))}>{data.targets.map(p=><option key={p} value={p}>{p}</option>)}</select></label>
      <div className="editor-grid"><label className="field">작업<select value={action} onChange={e=>edit(()=>{setAction(e.target.value as McpDraft["action"]);setName("");})}>{Object.entries(actions).map(([id,label])=><option key={id} value={id}>{label}</option>)}</select></label>
      {action==="register"?<label className="field">서버 이름<input value={name} onChange={e=>edit(()=>setName(e.target.value))} placeholder="my-mcp-server"/></label>:<label className="field">등록된 서버<select value={name} onChange={e=>edit(()=>setName(e.target.value))}><option value="">서버 선택</option>{existing.map(r=><option key={r.id} value={r.name}>{r.name}</option>)}</select></label>}</div>
      {action==="register"&&<>
        <label className="field">연결 방식<select value={remote?"remote":"local"} onChange={e=>edit(()=>setRemote(e.target.value==="remote"))}><option value="local">로컬 명령</option><option value="remote">HTTP URL</option></select></label>
        {remote?<><label className="field">서버 URL<input value={url} onChange={e=>edit(()=>setUrl(e.target.value))} placeholder="https://example.com/mcp"/></label><label className="field">Bearer 토큰 환경 변수 이름 (선택)<input value={tokenEnv} onChange={e=>edit(()=>setTokenEnv(e.target.value))} placeholder="MY_MCP_TOKEN"/></label></>:<><label className="field">실행 명령<input value={command} onChange={e=>edit(()=>setCommand(e.target.value))} placeholder="python 또는 실행 파일의 절대 경로"/></label><label className="field">인자 (JSON 문자열 배열)<input value={args} onChange={e=>edit(()=>setArgs(e.target.value))} placeholder='["C:/dev/my-mcp/server.py"]'/></label><label className="field">환경 변수 이름 (선택)<input value={env} onChange={e=>edit(()=>setEnv(e.target.value))} placeholder="NOTION_TOKEN, OTHER_NAME"/></label></>}
        <p className="inspection-hint">환경 변수의 값은 복사하거나 저장하지 않습니다. 에이전트 실행 환경에서 해당 변수를 제공해야 합니다.</p>
        <label className="inspection-option"><input type="checkbox" checked={enabled} onChange={e=>edit(()=>setEnabled(e.target.checked))}/>등록 시 활성 설정으로 저장 (기본: 비활성)</label>
      </>}
      <button className="secondary" disabled={!path||!name} onClick={()=>void review()}>변경 미리보기</button>
    </fieldset>
    {preview&&<div className="change-preview" role="region" aria-label="MCP 변경 미리보기"><h4>{actions[preview.change.action as keyof typeof actions]} · {preview.change.name}</h4><code>{preview.change.path}</code><p>{preview.change.before} → {preview.change.after}</p><p>{preview.change.transport}{preview.change.createsFile?" · 새 설정 파일 생성":""}</p>{preview.change.envNames.length>0&&<p>환경 변수 이름: {preview.change.envNames.join(", ")}</p>}<p className="inspection-hint">입력한 연결 정의를 저장합니다. 명령·인자·URL과 기존 설정 원문은 미리보기에 다시 표시하지 않습니다. 다른 설정과 주석은 유지하며, 실행 중인 세션에는 재시작·재로드가 필요할 수 있습니다.</p><button className="primary" disabled={locked} onClick={()=>void apply()}>백업하고 적용</button><button className="secondary" disabled={locked} onClick={()=>setPreview(null)}>취소</button></div>}
    {error&&<p className="inspection-error" role="alert">{error}</p>}{message&&<p className="inspection-hint" role="status">{message}</p>}
    <details className="backup-list"><summary>적용 백업 · {data.backups.length}개</summary><p className="inspection-hint">백업은 이 PC의 Backpack 앱 데이터 폴더에 보관합니다. 적용 이후 파일이 바뀌면 덮어쓰지 않고 복원을 중단합니다.</p>
      {data.backups.map(b=><div className="backup-item" key={b.id}><strong>{b.name} · {actions[b.action as keyof typeof actions]??b.action}</strong><small>{new Date(b.createdAt*1000).toLocaleString("ko-KR")}</small><code>{b.path}</code><button className="secondary" disabled={locked||!b.restorable} onClick={()=>{setPreview(null);setRestore(b);}}>복원 내용 확인</button>{!b.restorable&&<small>현재 파일과 적용 결과가 달라 복원 불가</small>}</div>)}
    </details>
    {restore&&<div className="change-preview" role="region" aria-label="MCP 백업 복원 확인"><h4>적용 전 원본으로 복원</h4><code>{restore.path}</code><p>{restore.existed?"이 파일 전체를 선택한 적용 전 원본으로 복원합니다.":"이 적용에서 새로 만든 설정 파일을 제거해 원래 파일이 없던 상태로 복원합니다."}</p><button className="primary" disabled={locked} onClick={()=>void recover()}>원본으로 복원</button><button className="secondary" disabled={locked} onClick={()=>setRestore(null)}>취소</button></div>}
  </section>;
}
