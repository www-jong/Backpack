use backpack_core::{scan, ScanRequest, Snapshot};

#[tauri::command]
async fn scan_inventory(request: ScanRequest) -> Result<Snapshot, String> {
    tauri::async_runtime::spawn_blocking(move || scan(request))
        .await
        .map_err(|_| "탐지 작업을 완료하지 못했습니다.".to_string())?
}

pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![scan_inventory])
        .run(tauri::generate_context!())
        .expect("Backpack 실행 실패");
}
