import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { ScanRequest } from "./types";
interface Target { id:string; agentId:string; scope:string; path:string }
interface Installation {id:string;agentId:string;scope:string;path:string;state:string;canRemove:boolean;canRestore:boolean}
interface Data {targets:Target[];installations:Installation[];notice:string}
interface Preview {token:string;change:{action:string;agentId:string;scope:string;path:string;note:string;files:{path:string;size:number}[]}}
interface Props {path:string;itemId:string;request:ScanRequest;scannedAt:number;disabled:boolean;onBusy:(busy:boolean)=>void;onChanged:()=>void}
const actionLabels:Record<string,string>={install:"설치",remove:"설치 해제",restore:"복원"};
const stateLabels:Record<string,string>={installed:"설치됨",disabled:"해제 · 보관됨",changed:"파일 변경 또는 경로 충돌",missing:"파일 없음"};
const scope=(value:string)=>value==="user"?"사용자":"프로젝트";
export default function LibraryInstall(props:Props) {
  const [data,setData]=useState<Data|null>(null);const [target,setTarget]=useState("");
  const [preview,setPreview]=useState<Preview|null>(null);const [error,setError]=useState("");const [message,setMessage]=useState("");const [busy,setBusy]=useState(false);
  const locked=props.disabled||busy;
  useEffect(()=>{let current=true;setPreview(null);setError("");setData(null);
    void invoke<Data>("library_deployment_data",{path:props.path,itemId:props.itemId,request:props.request}).then(next=>{if(current){setData(next);setTarget(previous=>next.targets.some(t=>t.id===previous)?previous:next.targets[0]?.id??"");}}).catch(e=>{if(current)setError(typeof e==="string"?e:"설치 정보를 읽지 못했습니다.");});
    return()=>{current=false;};
  },[props.path,props.itemId,props.request,props.scannedAt]);
  useEffect(()=>{if(props.disabled)setPreview(null);},[props.disabled]);
  async function work(run:()=>Promise<void>){setBusy(true);props.onBusy(true);setError("");setMessage("");try{await run();}catch(e){setError(typeof e==="string"?e:"설치 변경을 완료하지 못했습니다.");}finally{setBusy(false);props.onBusy(false);}}
  async function review(action:string,installationId=""){setPreview(null);await work(async()=>setPreview(await invoke<Preview>("preview_library_deployment",{path:props.path,itemId:props.itemId,targetId:target,installationId,action,request:props.request})));}
  async function apply(){if(!preview)return;const current=preview;setPreview(null);await work(async()=>{await invoke("apply_library_deployment",{token:current.token});setData(await invoke<Data>("library_deployment_data",{path:props.path,itemId:props.itemId,request:props.request}));setMessage(`${actionLabels[current.change.action]}했습니다. 에이전트에서 다시 로드해 확인하세요.`);props.onChanged();});}
  return <div className="library-install"><h4>에이전트 설치</h4>{data&&<><p className="inspection-hint">{data.notice}</p>{data.targets.length>0?<><label className="field">설치 대상<select disabled={locked} value={target} onChange={e=>{setTarget(e.target.value);setPreview(null);}}>{data.targets.map(t=><option key={t.id} value={t.id}>{t.agentId} · {scope(t.scope)}</option>)}</select></label><code className="library-location">{data.targets.find(t=>t.id===target)?.path}</code><button className="secondary" disabled={locked||!target} onClick={()=>void review("install")}>설치 미리보기</button></>:<p className="inspection-hint">이 종류·파일 형식의 설치는 아직 지원하지 않습니다.</p>}
    {data.installations.map(i=><div className="library-installation" key={i.id}><strong>{i.agentId} · {scope(i.scope)} · {stateLabels[i.state]??i.state}</strong><code className="library-location">{i.path}</code><button className="secondary" disabled={locked||!i.canRemove} onClick={()=>void review("remove",i.id)}>해제 미리보기</button><button className="secondary" disabled={locked||!i.canRestore} onClick={()=>void review("restore",i.id)}>복원 미리보기</button></div>)}</>}
    {preview&&<div className="change-preview"><h4>{actionLabels[preview.change.action]} · {preview.change.agentId} · {scope(preview.change.scope)}</h4><code className="library-location">{preview.change.path}</code><p>{preview.change.note}</p><ul className="library-files">{preview.change.files.map(f=><li key={f.path}><code>{f.path}</code><small>{f.size.toLocaleString()} bytes</small></li>)}</ul><button className="primary" disabled={locked} onClick={()=>void apply()}>{actionLabels[preview.change.action]} 적용</button><button className="secondary" disabled={locked} onClick={()=>setPreview(null)}>취소</button></div>}
    {error&&<p className="inspection-error" role="alert">{error}</p>}{message&&<p className="inspection-hint" role="status">{message}</p>}
  </div>;
}
