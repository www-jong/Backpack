use backpack_core::changes::{self, BackupReceipt, ChangePreview, McpDraft, PreparedChange};
use backpack_core::cli::{inspect_cli, CliInspection};
use backpack_core::codex::{inspect_codex, CodexInspection};
use backpack_core::deploy::{
    self, DeploymentData, DeploymentView, Installation, PreparedDeployment,
};
use backpack_core::library::{self, Comparison, ImportPreview, LibraryEntry, PreparedImport};
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
    servers: std::collections::BTreeMap<String, Vec<String>>,
    env_reference: bool,
    token_reference: bool,
    notice: String,
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
        let targets=changes::targets_at(&home,&request,&agent_id)?;
        let servers=targets.iter().map(|path|(path.clone(),changes::editor_servers_at(&home,&request,&agent_id,path).unwrap_or_default())).collect();
        Ok(EditorData {
            targets, servers,
            env_reference: agent_id!="antigravity",
            token_reference: !["antigravity","gemini"].contains(&agent_id.as_str()),
            notice: match agent_id.as_str() {
                "claude" => "프로젝트 폴더를 먼저 선택하세요. 새 서버는 ~/.claude.json의 해당 프로젝트 로컬 범위에 등록합니다. 기존 사용자·공유 프로젝트 MCP의 비활성 설정도 선택한 프로젝트에만 적용하며, 프로젝트 승인·조직 정책은 그대로 유지합니다.",
                "gemini" => "선택한 파일의 mcp.excluded 목록을 변경합니다. 허용 목록을 자동으로 넓히지 않으며 다른 범위·조직 정책이 연결을 제한할 수 있습니다. 원격 등록은 Streamable HTTP입니다.",
                "antigravity" => "disabled 설정으로 전환합니다. 서버 URL은 serverUrl로 저장하며, 환경 변수 참조와 새 인증 설정 등록은 아직 지원하지 않습니다.",
                _ => "선택한 파일의 활성 설정을 변경합니다. 다른 범위·조직 정책과 실행 중인 세션의 연결 상태는 별도로 확인하세요.",
            }.into(),
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

#[derive(Default)]
struct LibraryState(Arc<Mutex<Option<PendingImport>>>);
struct PendingImport {
    token: String,
    created: Instant,
    patch: PreparedImport,
}
#[derive(Serialize)]
struct LibraryPreview {
    token: String,
    change: ImportPreview,
}
#[tauri::command]
async fn connect_library(path: String) -> Result<Vec<LibraryEntry>, String> {
    tauri::async_runtime::spawn_blocking(move || library::connect(&path))
        .await
        .map_err(|_| "라이브러리 연결을 완료하지 못했습니다.".to_string())?
}
#[tauri::command]
async fn list_library(path: String) -> Result<Vec<LibraryEntry>, String> {
    tauri::async_runtime::spawn_blocking(move || library::list(&path))
        .await
        .map_err(|_| "라이브러리 목록을 읽지 못했습니다.".to_string())?
}
#[tauri::command]
async fn preview_library_import(
    path: String,
    request: ScanRequest,
    resource_id: String,
    state: tauri::State<'_, LibraryState>,
) -> Result<LibraryPreview, String> {
    let state = state.0.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut pending = state
            .lock()
            .map_err(|_| "가져오기 상태를 읽지 못했습니다.")?;
        *pending = None;
        let patch = library::prepare_at(&home()?, request, &path, &resource_id)?;
        let token = format!(
            "{:x}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        );
        let result = LibraryPreview {
            token: token.clone(),
            change: patch.view.clone(),
        };
        *pending = Some(PendingImport {
            token,
            created: Instant::now(),
            patch,
        });
        Ok(result)
    })
    .await
    .map_err(|_| "가져오기 미리보기를 만들지 못했습니다.".to_string())?
}
#[tauri::command]
async fn import_library_item(
    token: String,
    reviewed: bool,
    state: tauri::State<'_, LibraryState>,
    inspection: tauri::State<'_, InspectionState>,
) -> Result<LibraryEntry, String> {
    if !reviewed {
        return Err("공유할 파일과 비밀 값 포함 여부를 먼저 확인하세요.".into());
    }
    begin_edit(&inspection)?;
    let state = state.0.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let mut pending = state
            .lock()
            .map_err(|_| "가져오기 상태를 읽지 못했습니다.")?;
        let current = pending
            .as_ref()
            .ok_or("먼저 가져오기 미리보기를 확인하세요.")?;
        if current.token != token || current.created.elapsed() > Duration::from_secs(600) {
            return Err("미리보기가 만료됐습니다. 다시 확인하세요.".into());
        }
        library::import(pending.take().ok_or("미리보기가 없습니다.")?.patch)
    })
    .await
    .map_err(|_| "가져오기를 완료하지 못했습니다.".to_string())
    .and_then(|r| r);
    end_edit(&inspection);
    result
}
#[tauri::command]
async fn compare_library_item(
    path: String,
    item_id: String,
    resource_id: String,
    request: ScanRequest,
) -> Result<Comparison, String> {
    tauri::async_runtime::spawn_blocking(move || {
        library::compare_at(&home()?, &request, &path, &item_id, &resource_id)
    })
    .await
    .map_err(|_| "파일 비교를 완료하지 못했습니다.".to_string())?
}

#[derive(Default)]
struct DeployState(Arc<Mutex<Option<PendingDeployment>>>);
struct PendingDeployment {
    token: String,
    created: Instant,
    patch: PreparedDeployment,
}
#[derive(Serialize)]
struct DeploymentPreview {
    token: String,
    change: DeploymentView,
}
#[tauri::command]
async fn library_deployment_data(
    path: String,
    item_id: String,
    request: ScanRequest,
) -> Result<DeploymentData, String> {
    tauri::async_runtime::spawn_blocking(move || {
        deploy::data_at(&home()?, &request, &path, &item_id, &deploy::record_root()?)
    })
    .await
    .map_err(|_| "설치 정보를 읽지 못했습니다.".to_string())?
}
#[tauri::command]
async fn preview_library_deployment(
    path: String,
    item_id: String,
    target_id: String,
    installation_id: String,
    action: String,
    request: ScanRequest,
    state: tauri::State<'_, DeployState>,
) -> Result<DeploymentPreview, String> {
    let state = state.0.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut pending = state.lock().map_err(|_| "설치 상태를 읽지 못했습니다.")?;
        *pending = None;
        let patch = if action == "install" {
            deploy::prepare_install_at(
                &home()?,
                &request,
                &path,
                &item_id,
                &target_id,
                &deploy::record_root()?,
            )?
        } else {
            deploy::prepare_existing_at(
                &home()?,
                &request,
                &installation_id,
                &action,
                &deploy::record_root()?,
            )?
        };
        let token = format!(
            "{:x}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        );
        let result = DeploymentPreview {
            token: token.clone(),
            change: patch.view.clone(),
        };
        *pending = Some(PendingDeployment {
            token,
            created: Instant::now(),
            patch,
        });
        Ok(result)
    })
    .await
    .map_err(|_| "설치 미리보기를 만들지 못했습니다.".to_string())?
}
#[tauri::command]
async fn apply_library_deployment(
    token: String,
    state: tauri::State<'_, DeployState>,
    inspection: tauri::State<'_, InspectionState>,
) -> Result<Installation, String> {
    begin_edit(&inspection)?;
    let state = state.0.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let mut pending = state.lock().map_err(|_| "설치 상태를 읽지 못했습니다.")?;
        let current = pending.as_ref().ok_or("먼저 미리보기를 확인하세요.")?;
        if current.token != token || current.created.elapsed() > Duration::from_secs(600) {
            return Err("미리보기가 만료됐습니다. 다시 확인하세요.".into());
        }
        deploy::apply(pending.take().ok_or("미리보기가 없습니다.")?.patch)
    })
    .await
    .map_err(|_| "설치 변경을 완료하지 못했습니다.".to_string())
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
        .manage(LibraryState::default())
        .manage(DeployState::default())
        .invoke_handler(tauri::generate_handler![
            scan_inventory,
            library_deployment_data,
            preview_library_deployment,
            apply_library_deployment,
            connect_library,
            list_library,
            preview_library_import,
            import_library_item,
            compare_library_item,
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
