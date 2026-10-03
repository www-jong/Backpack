use backpack_core::cli::{inspect_cli, CliInspection};
use backpack_core::codex::{inspect_codex, CodexInspection};
use backpack_core::{scan, ScanRequest, Snapshot};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};

#[derive(Default)]
struct InspectionState(Mutex<Option<(String, Arc<AtomicBool>)>>);

#[tauri::command]
async fn scan_inventory(request: ScanRequest) -> Result<Snapshot, String> {
    tauri::async_runtime::spawn_blocking(move || scan(request))
        .await
        .map_err(|_| "탐지 작업을 완료하지 못했습니다.".to_string())?
}

#[tauri::command]
async fn inspect_codex_inventory(
    request: ScanRequest,
    include_mcp: bool,
    request_id: String,
    state: tauri::State<'_, InspectionState>,
) -> Result<CodexInspection, String> {
    if request_id.is_empty() || request_id.len() > 64 {
        return Err("조회 요청 ID가 올바르지 않습니다.".into());
    }
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut active = state.0.lock().map_err(|_| "조회 상태를 읽지 못했습니다.")?;
        if active.is_some() {
            return Err("다른 조회가 이미 진행 중입니다.".into());
        }
        *active = Some((request_id.clone(), cancel.clone()));
    }
    let result = inspect_codex(request, include_mcp, cancel).await;
    if let Ok(mut active) = state.0.lock() {
        if active.as_ref().is_some_and(|(id, _)| id == &request_id) {
            *active = None;
        }
    }
    result
}

#[tauri::command]
async fn inspect_cli_inventory(
    request: ScanRequest,
    agent_id: String,
    request_id: String,
    state: tauri::State<'_, InspectionState>,
) -> Result<CliInspection, String> {
    if request_id.is_empty() || request_id.len() > 64 {
        return Err("조회 요청 ID가 올바르지 않습니다.".into());
    }
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut active = state.0.lock().map_err(|_| "조회 상태를 읽지 못했습니다.")?;
        if active.is_some() {
            return Err("다른 조회가 이미 진행 중입니다.".into());
        }
        *active = Some((request_id.clone(), cancel.clone()));
    }
    let result = inspect_cli(request, agent_id, cancel).await;
    if let Ok(mut active) = state.0.lock() {
        if active.as_ref().is_some_and(|(id, _)| id == &request_id) {
            *active = None;
        }
    }
    result
}

#[tauri::command]
fn cancel_inspection(
    request_id: String,
    state: tauri::State<'_, InspectionState>,
) -> Result<bool, String> {
    let active = state.0.lock().map_err(|_| "조회 상태를 읽지 못했습니다.")?;
    if let Some((_, cancel)) = active.as_ref().filter(|(id, _)| id == &request_id) {
        cancel.store(true, Ordering::Relaxed);
        return Ok(true);
    }
    Ok(false)
}

pub fn run() {
    tauri::Builder::default()
        .manage(InspectionState::default())
        .invoke_handler(tauri::generate_handler![
            scan_inventory,
            inspect_codex_inventory,
            inspect_cli_inventory,
            cancel_inspection
        ])
        .run(tauri::generate_context!())
        .expect("Backpack 실행 실패");
}
