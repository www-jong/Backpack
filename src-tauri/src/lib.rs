use backpack_core::changes::{self, BackupReceipt, ChangePreview, McpDraft, PreparedChange};
use backpack_core::cli::{inspect_cli, CliInspection};
use backpack_core::codex::{inspect_codex, CodexInspection};
use backpack_core::{scan, ScanRequest, Snapshot};
use serde::Serialize;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[derive(Default)]
struct InspectionState(Mutex<Option<(String, Arc<AtomicBool>)>>);

#[derive(Default)]
struct EditorState(Arc<Mutex<Option<PendingChange>>>);
struct PendingChange {
    token: String,
    created: Instant,
    patch: PreparedChange,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EditorData {
    targets: Vec<String>,
    backups: Vec<BackupReceipt>,
}
#[derive(Serialize)]
struct PreviewResult {
    token: String,
    change: ChangePreview,
}
fn home() -> Result<std::path::PathBuf, String> {
    dirs::home_dir().ok_or_else(|| "사용자 홈 경로를 찾지 못했습니다.".into())
}
fn begin_edit(state: &InspectionState) -> Result<(), String> {
    let mut active = state.0.lock().map_err(|_| "작업 상태를 읽지 못했습니다.")?;
    if active.is_some() {
        return Err("다른 조회·설정 적용 작업이 진행 중입니다.".into());
    }
    *active = Some(("settings-change".into(), Arc::new(AtomicBool::new(false))));
    Ok(())
}
fn end_edit(state: &InspectionState) {
    if let Ok(mut active) = state.0.lock() {
        if active
            .as_ref()
            .is_some_and(|(id, _)| id == "settings-change")
        {
            *active = None;
        }
    }
}
#[tauri::command]
async fn mcp_editor_data(request: ScanRequest, agent_id: String) -> Result<EditorData, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let home = home()?;
        Ok(EditorData {
            targets: changes::targets_at(&home, &request, &agent_id)?,
            backups: changes::list_backups_at(
                &home,
                &request,
                &agent_id,
                &changes::backup_root()?,
            )?,
        })
    })
    .await
    .map_err(|_| "설정 정보를 읽지 못했습니다.".to_string())?
}
#[tauri::command]
async fn preview_mcp_change(
    request: ScanRequest,
    draft: McpDraft,
    editor: tauri::State<'_, EditorState>,
) -> Result<PreviewResult, String> {
    let editor = editor.0.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut pending = editor
            .lock()
            .map_err(|_| "미리보기 상태를 읽지 못했습니다.")?;
        *pending = None;
        let patch = changes::prepare_at(&home()?, &request, draft)?;
        let token = format!(
            "{:x}-{:x}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos(),
            std::process::id()
        );
        let result = PreviewResult {
            token: token.clone(),
            change: patch.view.clone(),
        };
        *pending = Some(PendingChange {
            token,
            created: Instant::now(),
            patch,
        });
        Ok(result)
    })
    .await
    .map_err(|_| "미리보기를 만들지 못했습니다.".to_string())?
}
#[tauri::command]
async fn apply_mcp_change(
    token: String,
    editor: tauri::State<'_, EditorState>,
    inspection: tauri::State<'_, InspectionState>,
) -> Result<BackupReceipt, String> {
    begin_edit(&inspection)?;
    let editor = editor.0.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let mut pending = editor
            .lock()
            .map_err(|_| "미리보기 상태를 읽지 못했습니다.")?;
        let view = pending.as_ref().ok_or("먼저 변경 미리보기를 확인하세요.")?;
        if view.token != token || view.created.elapsed() > Duration::from_secs(600) {
            return Err("미리보기가 만료됐습니다. 다시 확인하세요.".into());
        }
        let view = pending.take().ok_or("미리보기가 없습니다.")?;
        changes::apply(view.patch, &changes::backup_root()?)
    })
    .await
    .map_err(|_| "설정 적용 작업을 완료하지 못했습니다.".to_string())
    .and_then(|r| r);
    end_edit(&inspection);
    result
}
#[tauri::command]
async fn restore_mcp_backup(
    request: ScanRequest,
    agent_id: String,
    backup_id: String,
    editor: tauri::State<'_, EditorState>,
    inspection: tauri::State<'_, InspectionState>,
) -> Result<(), String> {
    begin_edit(&inspection)?;
    let editor = editor.0.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let mut pending = editor.lock().map_err(|_| "설정 상태를 읽지 못했습니다.")?;
        *pending = None;
        changes::restore_at(
            &home()?,
            &request,
            &agent_id,
            &changes::backup_root()?,
            &backup_id,
        )
    })
    .await
    .map_err(|_| "복원 작업을 완료하지 못했습니다.".to_string())
    .and_then(|r| r);
    end_edit(&inspection);
    result
}

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
        .manage(EditorState::default())
        .invoke_handler(tauri::generate_handler![
            scan_inventory,
            inspect_codex_inventory,
            inspect_cli_inventory,
            cancel_inspection,
            mcp_editor_data,
            preview_mcp_change,
            apply_mcp_change,
            restore_mcp_backup
        ])
        .run(tauri::generate_context!())
        .expect("Backpack 실행 실패");
}
