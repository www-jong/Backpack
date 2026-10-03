import { useState } from "react";
import type { CustomAgent } from "./types";
interface Props { agents: CustomAgent[]; canSave: boolean; onSave: (agents:CustomAgent[]) => Promise<boolean> }
export default function AgentRegistration(props:Props) {
  const [draft,setDraft] = useState(props.agents);
  const [fileNames,setFileNames] = useState<Record<string,string>>({});
  const [saving,setSaving] = useState(false);
  const [error,setError] = useState("");
  function update(id:string,patch:Partial<CustomAgent>) { setDraft(draft.map(a=>a.id===id ? {...a,...patch} : a)); }
  async function save() {
    setError(""); setSaving(true);
    if (!(await props.onSave(draft.map(a=>({...a,name:a.name.trim(),configRoot:a.configRoot.trim(),executable:a.executable?.trim(),configFiles:(fileNames[a.id] ?? a.configFiles.join(",")).split(",").map(s=>s.trim()).filter(Boolean)}))))) setError("등록을 저장하지 못했습니다. 탐지 화면의 오류와 입력 경로를 확인하세요.");
    setSaving(false);
  }
  return <div>
    <p className="settings-description">지원 목록에 없는 AI도 등록할 수 있습니다. 실행 파일과 설정 폴더를 확인하고, 공통 형식의 스킬·룰·Tools·MCP를 파일에서 탐지합니다. 등록한 프로그램은 실행하지 않습니다. 이 PC에 저장됩니다.</p>
    {draft.map(agent=><fieldset className="custom-agent-form" key={agent.id}><legend>{agent.name || "새 에이전트"}</legend>
      <label className="field">이름<input value={agent.name} maxLength={100} onChange={e=>update(agent.id,{name:e.target.value})}/></label>
      <label className="field">설정 폴더<input value={agent.configRoot} placeholder="설정 폴더의 절대 경로" onChange={e=>update(agent.id,{configRoot:e.target.value})}/></label>
      <label className="field">실행 파일 (선택)<input value={agent.executable ?? ""} placeholder="실행 파일의 절대 경로" onChange={e=>update(agent.id,{executable:e.target.value})}/></label>
      <label className="field">설정 파일 이름 (선택)<input value={fileNames[agent.id] ?? agent.configFiles.join(", ")} placeholder="settings.json, mcp.json (비우면 공통 파일 탐지)" onChange={e=>setFileNames({...fileNames,[agent.id]:e.target.value})}/></label>
      <button className="text-button" onClick={()=>setDraft(draft.filter(a=>a.id!==agent.id))}>등록 제거</button>
    </fieldset>)}
    {!draft.length && <p className="settings-description">등록한 기타 에이전트가 없습니다.</p>}
    <button className="secondary" disabled={saving || draft.length>=32} onClick={()=>setDraft([...draft,{id:"custom-"+crypto.randomUUID(),name:"",configRoot:"",configFiles:[]}])}>기타 에이전트 추가</button>
    {error && <p className="settings-save-error" role="alert">{error}</p>}
    <div className="modal-actions"><button className="primary" disabled={!props.canSave || saving} onClick={()=>void save()}>{saving ? "확인 중" : "등록 저장하고 탐지"}</button></div>
  </div>;
}
