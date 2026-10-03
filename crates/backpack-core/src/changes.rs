use crate::{display, scan_at, ScanRequest};
use jsonc_parser::{
    cst::{CstInputValue, CstObject, CstRootNode},
    ParseOptions,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
const MAX: u64 = 2 * 1024 * 1024;
#[derive(Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpDraft {
    pub agent_id: String,
    pub path: String,
    pub name: String,
    pub action: String,
    #[serde(default)]
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub env_names: Vec<String>,
    #[serde(default)]
    pub token_env: String,
    #[serde(default)]
    pub enabled: bool,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangePreview {
    pub path: String,
    pub name: String,
    pub action: String,
    pub before: String,
    pub after: String,
    pub transport: String,
    pub env_names: Vec<String>,
    pub creates_file: bool,
}
pub struct PreparedChange {
    pub view: ChangePreview,
    target: PathBuf,
    parent: PathBuf,
    before: Option<Vec<u8>>,
    after: Vec<u8>,
    agent_id: String,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    version: u8,
    agent_id: String,
    target: String,
    name: String,
    action: String,
    before_hash: String,
    after_hash: String,
    existed: bool,
    created_at: u64,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupReceipt {
    pub id: String,
    pub path: String,
    pub name: String,
    pub action: String,
    pub created_at: u64,
    pub restorable: bool,
    pub existed: bool,
}
pub fn backup_root() -> Result<PathBuf, String> {
    Ok(dirs::data_local_dir()
        .ok_or("앱 데이터 경로를 찾지 못했습니다.")?
        .join("Backpack/backups"))
}
fn hash(bytes: &Option<Vec<u8>>) -> String {
    bytes
        .as_ref()
        .map(|v| format!("{:x}", Sha256::digest(v)))
        .unwrap_or_else(|| "missing".into())
}
fn read_file(path: &Path) -> Result<Option<Vec<u8>>, String> {
    let meta = match fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err("설정 파일에 접근하지 못했습니다.".into()),
    };
    if !meta.is_file() || meta.file_type().is_symlink() {
        return Err("일반 설정 파일만 변경할 수 있습니다. 링크 파일은 지원하지 않습니다.".into());
    }
    if meta.len() > MAX {
        return Err("설정 파일 크기 제한을 초과했습니다.".into());
    }
    let mut bytes = vec![];
    fs::File::open(path)
        .map_err(|_| "파일을 열지 못했습니다.")?
        .take(MAX + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "파일을 읽지 못했습니다.")?;
    if bytes.len() as u64 > MAX {
        return Err("설정 파일 크기 제한을 초과했습니다.".into());
    }
    Ok(Some(bytes))
}
pub fn targets_at(home: &Path, request: &ScanRequest, id: &str) -> Result<Vec<String>, String> {
    if !["codex", "opencode"].contains(&id) {
        return Err("이 에이전트의 MCP 설정 적용은 아직 지원하지 않습니다.".into());
    }
    let snapshot = scan_at(home, request.clone())?;
    let agent = snapshot
        .agents
        .iter()
        .find(|a| a.id == id)
        .ok_or("에이전트가 없습니다.")?;
    let mut paths = vec![];
    let root = Path::new(&agent.config_roots[0]);
    if id == "codex" {
        paths.push(root.join("config.toml"));
        if let Some(project) = &snapshot.project_path {
            paths.push(Path::new(project).join(".codex/config.toml"));
        }
    } else {
        for file in ["opencode.json", "opencode.jsonc"] {
            paths.push(root.join(file));
            if let Some(project) = &snapshot.project_path {
                paths.push(Path::new(project).join(file));
            }
        }
        if let Some(file) = std::env::var_os("OPENCODE_CONFIG") {
            let file = PathBuf::from(file);
            if file.is_absolute() {
                paths.push(file);
            }
        }
    }
    paths.sort();
    paths.dedup();
    Ok(paths.into_iter().map(|p| display(&p)).collect())
}
fn target_at(
    home: &Path,
    request: &ScanRequest,
    id: &str,
    target: &str,
) -> Result<(PathBuf, PathBuf), String> {
    let target = PathBuf::from(target);
    if !targets_at(home, request, id)?
        .iter()
        .any(|p| Path::new(p) == target)
    {
        return Err("탐지 문맥의 MCP 설정 파일만 변경할 수 있습니다.".into());
    }
    let parent = target
        .parent()
        .ok_or("설정 파일 경로가 올바르지 않습니다.")?
        .canonicalize()
        .map_err(|_| "설정 폴더가 없습니다. 먼저 탐지 경로를 확인하세요.")?;
    read_file(&target)?;
    Ok((target, parent))
}
fn variable(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && name
            .bytes()
            .enumerate()
            .all(|(i, b)| b == b'_' || b.is_ascii_alphabetic() || i > 0 && b.is_ascii_digit())
}
fn validate(d: &McpDraft) -> Result<(), String> {
    if d.name.is_empty()
        || d.name.len() > 128
        || !d
            .name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    {
        return Err("서버 이름은 영문·숫자·밑줄·하이픈으로 입력하세요.".into());
    }
    if !["register", "enable", "disable"].contains(&d.action.as_str()) {
        return Err("지원하지 않는 MCP 변경입니다.".into());
    }
    if d.action != "register" {
        return Ok(());
    }
    if d.args.len() > 128
        || d.args.iter().any(|a| a.len() > 4096)
        || d.command.len() > 4096
        || d.url.len() > 8192
        || d.env_names.len() > 64
        || d.env_names.iter().any(|n| !variable(n))
        || (!d.token_env.is_empty() && !variable(&d.token_env))
    {
        return Err("명령·인자·환경 변수 입력을 확인하세요.".into());
    }
    if d.command.trim().is_empty() == d.url.trim().is_empty() {
        return Err("로컬 명령 또는 서버 URL 중 하나만 입력하세요.".into());
    }
    if !d.url.is_empty() {
        let url = url::Url::parse(&d.url).map_err(|_| "서버 URL 형식이 올바르지 않습니다.")?;
        if !["http", "https"].contains(&url.scheme())
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
        {
            return Err(
                "HTTP·HTTPS 서버 URL을 입력하고 인증 정보는 환경 변수로 지정하세요.".into(),
            );
        }
        if !d.args.is_empty() || !d.env_names.is_empty() {
            return Err("URL 서버에는 로컬 명령 인자·환경 변수 목록을 지정할 수 없습니다.".into());
        }
    } else if !d.token_env.is_empty() {
        return Err("토큰 환경 변수는 URL 서버에서만 지원합니다.".into());
    }
    Ok(())
}
fn input(v: Value) -> CstInputValue {
    match v {
        Value::Null => CstInputValue::Null,
        Value::Bool(b) => CstInputValue::Bool(b),
        Value::Number(n) => CstInputValue::Number(n.to_string()),
        Value::String(s) => CstInputValue::String(s),
        Value::Array(a) => CstInputValue::Array(a.into_iter().map(input).collect()),
        Value::Object(o) => {
            CstInputValue::Object(o.into_iter().map(|(k, v)| (k, input(v))).collect())
        }
    }
}
fn unique(object: &CstObject) -> Result<(), String> {
    let mut keys = HashSet::new();
    for p in object.properties() {
        let key = p
            .name()
            .and_then(|n| n.decoded_value().ok())
            .ok_or("설정 키를 해석하지 못했습니다.")?;
        if !keys.insert(key) {
            return Err("중복 설정 키가 있어 안전하게 변경할 수 없습니다.".into());
        }
    }
    Ok(())
}
fn edit_json(text: &str, d: &McpDraft) -> Result<(String, Option<bool>), String> {
    let options = ParseOptions {
        allow_comments: true,
        allow_trailing_commas: true,
        allow_loose_object_property_names: false,
        allow_missing_commas: false,
        allow_single_quoted_strings: false,
        allow_hexadecimal_numbers: false,
        allow_unary_plus_numbers: false,
    };
    let root = CstRootNode::parse(text, &options)
        .map_err(|_| "JSON·JSONC 설정 형식을 해석하지 못했습니다.")?;
    let obj = root.object_value().ok_or("설정 루트는 객체여야 합니다.")?;
    unique(&obj)?;
    let mcp = obj
        .object_value_or_create("mcp")
        .ok_or("mcp 설정은 객체여야 합니다.")?;
    unique(&mcp)?;
    if mcp
        .object_value("servers")
        .is_some_and(|s| s.get("type").is_none())
    {
        return Err("OpenCode v2 MCP 설정 형식은 아직 변경하지 않습니다.".into());
    }
    let existing = mcp.get(&d.name);
    if d.action == "register" {
        if existing.is_some() {
            return Err("같은 이름의 MCP가 이미 등록되어 있습니다.".into());
        }
        let mut server = if d.url.is_empty() {
            let mut command = vec![d.command.clone()];
            command.extend(d.args.clone());
            json!({"type":"local","command":command,"enabled":d.enabled})
        } else {
            json!({"type":"remote","url":d.url,"enabled":d.enabled})
        };
        if !d.env_names.is_empty() {
            server["environment"] = json!(d
                .env_names
                .iter()
                .map(|n| (n.clone(), format!("{{env:{n}}}")))
                .collect::<std::collections::BTreeMap<_, _>>());
        }
        if !d.token_env.is_empty() {
            server["headers"] = json!({"Authorization":format!("Bearer {{env:{}}}",d.token_env)});
        }
        mcp.append(&d.name, input(server));
        Ok((root.to_string(), None))
    } else {
        let server = existing
            .ok_or("이 설정 파일에 해당 MCP가 없습니다.")?
            .object_value()
            .ok_or("해당 MCP는 서버 객체가 아닙니다.")?;
        unique(&server)?;
        let current: Value =
            json5::from_str(&server.to_string()).map_err(|_| "MCP 형식이 올바르지 않습니다.")?;
        if ![Some("local"), Some("remote")].contains(&current.get("type").and_then(Value::as_str)) {
            return Err("이 MCP 정의 형식은 변경하지 않습니다.".into());
        }
        let before = current
            .get("enabled")
            .map(|v| v.as_bool().ok_or("enabled 설정이 불리언이 아닙니다."))
            .transpose()?;
        if let Some(enabled) = server.get("enabled") {
            enabled.set_value(CstInputValue::Bool(d.action == "enable"));
        } else {
            server.append("enabled", CstInputValue::Bool(d.action == "enable"));
        }
        Ok((root.to_string(), before))
    }
}
fn edit_toml(text: &str, d: &McpDraft) -> Result<(String, Option<bool>), String> {
    let mut doc = text
        .parse::<toml_edit::DocumentMut>()
        .map_err(|_| "TOML 설정 형식을 해석하지 못했습니다.")?;
    if doc.get("mcp_servers").is_none() {
        doc["mcp_servers"] = toml_edit::Item::Table(toml_edit::Table::new());
    }
    let servers = doc["mcp_servers"]
        .as_table_like_mut()
        .ok_or("mcp_servers 형식이 올바르지 않습니다.")?;
    if d.action == "register" {
        if servers.contains_key(&d.name) {
            return Err("같은 이름의 MCP가 이미 등록되어 있습니다.".into());
        }
        let mut server = toml_edit::Table::new();
        server["enabled"] = toml_edit::value(d.enabled);
        if d.url.is_empty() {
            server["command"] = toml_edit::value(&d.command);
            let mut args = toml_edit::Array::new();
            for arg in &d.args {
                args.push(arg);
            }
            server["args"] = toml_edit::value(args);
            if !d.env_names.is_empty() {
                let mut names = toml_edit::Array::new();
                for name in &d.env_names {
                    names.push(name);
                }
                server["env_vars"] = toml_edit::value(names);
            }
        } else {
            server["url"] = toml_edit::value(&d.url);
            if !d.token_env.is_empty() {
                server["bearer_token_env_var"] = toml_edit::value(&d.token_env);
            }
        }
        if doc["mcp_servers"].as_table().is_none() {
            return Err("인라인 MCP 테이블에 새 서버 등록은 아직 지원하지 않습니다.".into());
        }
        doc["mcp_servers"][&d.name] = toml_edit::Item::Table(server);
        Ok((doc.to_string(), None))
    } else {
        let server = servers
            .get_mut(&d.name)
            .and_then(|s| s.as_table_like_mut())
            .ok_or("이 파일에 변경 가능한 MCP 정의가 없습니다.")?;
        let before = server
            .get("enabled")
            .map(|v| v.as_bool().ok_or("enabled 설정이 불리언이 아닙니다."))
            .transpose()?;
        let mut enabled = toml_edit::value(d.action == "enable");
        if let (Some(old), Some(new)) = (
            server.get("enabled").and_then(|v| v.as_value()),
            enabled.as_value_mut(),
        ) {
            *new.decor_mut() = old.decor().clone();
        }
        server.insert("enabled", enabled);
        Ok((doc.to_string(), before))
    }
}
pub fn prepare_at(
    home: &Path,
    request: &ScanRequest,
    draft: McpDraft,
) -> Result<PreparedChange, String> {
    validate(&draft)?;
    let (target, parent) = target_at(home, request, &draft.agent_id, &draft.path)?;
    let before = read_file(&target)?;
    if before.is_none() && draft.action != "register" {
        return Err("설정 파일이 없습니다.".into());
    }
    let text = before
        .as_ref()
        .map(|b| std::str::from_utf8(b))
        .transpose()
        .map_err(|_| "UTF-8 설정 파일만 변경할 수 있습니다.")?;
    let (after, old) = if draft.agent_id == "codex" {
        edit_toml(text.unwrap_or(""), &draft)?
    } else {
        edit_json(text.unwrap_or("{}\n"), &draft)?
    };
    if after.len() as u64 > MAX {
        return Err("변경 결과가 크기 제한을 초과합니다.".into());
    }
    if before.as_deref() == Some(after.as_bytes()) {
        return Err("이미 해당 설정 상태입니다.".into());
    }
    let enabled = if draft.action == "register" {
        draft.enabled
    } else {
        draft.action == "enable"
    };
    Ok(PreparedChange {
        view: ChangePreview {
            path: display(&target),
            name: draft.name,
            action: draft.action.clone(),
            before: if draft.action == "register" {
                "미등록".into()
            } else {
                old.map(|v| {
                    if v {
                        "enabled = true"
                    } else {
                        "enabled = false"
                    }
                    .into()
                })
                .unwrap_or_else(|| "enabled 미지정 (기본값)".into())
            },
            after: format!("enabled = {enabled}"),
            transport: if draft.action != "register" {
                "기존 연결 정의 유지"
            } else if draft.url.is_empty() {
                "로컬 명령"
            } else {
                "HTTP URL"
            }
            .into(),
            env_names: if draft.token_env.is_empty() {
                draft.env_names
            } else {
                vec![draft.token_env]
            },
            creates_file: before.is_none(),
        },
        target,
        parent,
        before,
        after: after.into_bytes(),
        agent_id: draft.agent_id,
    })
}
fn atomic_write(target: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = target.parent().ok_or("파일 경로가 올바르지 않습니다.")?;
    let mut temp = tempfile::Builder::new()
        .prefix(".backpack-write-")
        .tempfile_in(parent)
        .map_err(|_| "임시 파일을 만들지 못했습니다.")?;
    if let Ok(meta) = fs::metadata(target) {
        temp.as_file()
            .set_permissions(meta.permissions())
            .map_err(|_| "파일 권한을 유지하지 못했습니다.")?;
    }
    temp.write_all(bytes)
        .and_then(|_| temp.as_file().sync_all())
        .map_err(|_| "변경 내용을 임시 파일에 저장하지 못했습니다.")?;
    #[cfg(windows)]
    if target.exists() {
        use std::os::windows::ffi::OsStrExt;
        let path = temp.into_temp_path();
        let target_w: Vec<u16> = target.as_os_str().encode_wide().chain(Some(0)).collect();
        let temp_w: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        let result = unsafe {
            windows_sys::Win32::Storage::FileSystem::ReplaceFileW(
                target_w.as_ptr(),
                temp_w.as_ptr(),
                std::ptr::null(),
                0,
                std::ptr::null(),
                std::ptr::null(),
            )
        };
        if result == 0 {
            return Err("원본 파일을 교체하지 못했습니다. 백업은 보존됩니다.".into());
        }
        return Ok(());
    }
    temp.persist(target)
        .map_err(|_| "원본 파일을 교체하지 못했습니다. 백업은 보존됩니다.".to_string())?;
    Ok(())
}
fn protected_dir(path: &Path) -> Result<(), String> {
    fs::create_dir_all(path).map_err(|_| "백업 폴더를 만들지 못했습니다.")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .map_err(|_| "백업 폴더 권한을 설정하지 못했습니다.")?;
    }
    Ok(())
}
fn ensure_current(prepared: &PreparedChange) -> Result<(), String> {
    if prepared
        .target
        .parent()
        .and_then(|p| p.canonicalize().ok())
        .as_ref()
        != Some(&prepared.parent)
        || hash(&read_file(&prepared.target)?) != hash(&prepared.before)
    {
        return Err(
            "미리보기 이후 설정 파일 또는 폴더가 변경됐습니다. 다시 미리보기를 확인하세요.".into(),
        );
    }
    Ok(())
}
pub fn apply(prepared: PreparedChange, backups: &Path) -> Result<BackupReceipt, String> {
    ensure_current(&prepared)?;
    protected_dir(backups)?;
    let dir = tempfile::Builder::new()
        .prefix("change-")
        .tempdir_in(backups)
        .map_err(|_| "백업 기록을 만들지 못했습니다.")?;
    protected_dir(dir.path())?;
    atomic_write(
        &dir.path().join("original"),
        prepared.before.as_deref().unwrap_or_default(),
    )?;
    let manifest = Manifest {
        version: 1,
        agent_id: prepared.agent_id.clone(),
        target: display(&prepared.target),
        name: prepared.view.name.clone(),
        action: prepared.view.action.clone(),
        before_hash: hash(&prepared.before),
        after_hash: hash(&Some(prepared.after.clone())),
        existed: prepared.before.is_some(),
        created_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
    };
    atomic_write(
        &dir.path().join("manifest.json"),
        &serde_json::to_vec(&manifest).map_err(|_| "백업 기록을 생성하지 못했습니다.")?,
    )?;
    ensure_current(&prepared)?;
    let directory = dir.keep();
    atomic_write(&prepared.target, &prepared.after)?;
    Ok(BackupReceipt {
        id: directory
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into(),
        path: manifest.target,
        name: manifest.name,
        action: manifest.action,
        created_at: manifest.created_at,
        restorable: true,
        existed: manifest.existed,
    })
}
fn manifest(backups: &Path, id: &str) -> Result<Manifest, String> {
    if !id.starts_with("change-")
        || id.len() > 80
        || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
    {
        return Err("백업 ID가 올바르지 않습니다.".into());
    }
    let dir = backups.join(id);
    if fs::symlink_metadata(&dir)
        .map_err(|_| "백업을 찾지 못했습니다.")?
        .file_type()
        .is_symlink()
    {
        return Err("링크 백업은 지원하지 않습니다.".into());
    }
    let bytes = read_file(&dir.join("manifest.json"))?.ok_or("백업 기록이 없습니다.")?;
    let manifest: Manifest =
        serde_json::from_slice(&bytes).map_err(|_| "백업 기록 형식이 올바르지 않습니다.")?;
    if manifest.version != 1 {
        return Err("지원하지 않는 백업 버전입니다.".into());
    }
    Ok(manifest)
}
pub fn list_backups_at(
    home: &Path,
    request: &ScanRequest,
    id: &str,
    backups: &Path,
) -> Result<Vec<BackupReceipt>, String> {
    let targets = targets_at(home, request, id)?;
    let mut out = vec![];
    let entries = match fs::read_dir(backups) {
        Ok(v) => v,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(out),
        Err(_) => return Err("백업 목록을 읽지 못했습니다.".into()),
    };
    for entry in entries.take(2000).flatten() {
        let key = entry.file_name().to_string_lossy().into_owned();
        let Ok(m) = manifest(backups, &key) else {
            continue;
        };
        if m.agent_id != id || !targets.contains(&m.target) {
            continue;
        }
        let restorable =
            read_file(Path::new(&m.target)).is_ok_and(|bytes| hash(&bytes) == m.after_hash);
        out.push(BackupReceipt {
            id: key,
            path: m.target,
            name: m.name,
            action: m.action,
            created_at: m.created_at,
            restorable,
            existed: m.existed,
        });
    }
    out.sort_by(|a, b| (b.created_at, &b.id).cmp(&(a.created_at, &a.id)));
    out.truncate(100);
    Ok(out)
}
pub fn restore_at(
    home: &Path,
    request: &ScanRequest,
    id: &str,
    backups: &Path,
    key: &str,
) -> Result<(), String> {
    let m = manifest(backups, key)?;
    if m.agent_id != id {
        return Err("이 에이전트의 백업이 아닙니다.".into());
    }
    let (target, parent) = target_at(home, request, id, &m.target)?;
    let current = read_file(&target)?;
    if hash(&current) != m.after_hash {
        return Err("적용 이후 설정이 바뀌어 복원을 중단했습니다. 현재 파일을 확인하세요.".into());
    }
    let original =
        read_file(&backups.join(key).join("original"))?.ok_or("원본 백업이 없습니다.")?;
    if hash(&if m.existed {
        Some(original.clone())
    } else {
        None
    }) != m.before_hash
    {
        return Err("원본 백업 무결성 검사를 통과하지 못했습니다.".into());
    }
    if target.parent().and_then(|p| p.canonicalize().ok()).as_ref() != Some(&parent)
        || hash(&read_file(&target)?) != m.after_hash
    {
        return Err("복원 직전에 설정이 변경됐습니다.".into());
    }
    if m.existed {
        atomic_write(&target, &original)?;
    } else {
        fs::remove_file(&target).map_err(|_| "새 설정 파일을 제거하지 못했습니다.")?;
    }
    Ok(())
}
