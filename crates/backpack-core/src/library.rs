use crate::{scan_at, Resource, ScanRequest};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashSet},
    fs,
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
const FILE_MAX: u64 = 2 * 1024 * 1024;
const TOTAL_MAX: usize = 16 * 1024 * 1024;
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryFile {
    pub path: String,
    pub hash: String,
    pub size: usize,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryEntry {
    pub version: u8,
    pub id: String,
    pub name: String,
    pub agent_id: String,
    pub kind: String,
    pub created_at: u64,
    pub files: Vec<LibraryFile>,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportPreview {
    pub name: String,
    pub kind: String,
    pub agent_id: String,
    pub files: Vec<LibraryFile>,
    pub skipped: Vec<String>,
    pub total_bytes: usize,
    pub note: String,
}
pub struct PreparedImport {
    pub view: ImportPreview,
    root: PathBuf,
    home: PathBuf,
    request: ScanRequest,
    resource_id: String,
    payload: BTreeMap<String, Vec<u8>>,
    fingerprint: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Comparison {
    pub files: Vec<FileComparison>,
    pub identical: bool,
}
#[derive(Serialize)]
pub struct FileComparison {
    pub path: String,
    pub status: String,
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn regular(path: &Path) -> Result<fs::Metadata, String> {
    let meta = fs::symlink_metadata(path).map_err(|_| "파일이나 폴더에 접근하지 못했습니다.")?;
    if meta.file_type().is_symlink() {
        return Err("링크 파일·폴더는 라이브러리 작업에서 지원하지 않습니다.".into());
    }
    Ok(meta)
}
fn read(path: &Path) -> Result<Vec<u8>, String> {
    let meta = regular(path)?;
    if !meta.is_file() || meta.len() > FILE_MAX {
        return Err("일반 파일과 파일당 2MB 이하만 지원합니다.".into());
    }
    let mut bytes = vec![];
    fs::File::open(path)
        .map_err(|_| "파일을 열지 못했습니다.")?
        .take(FILE_MAX + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "파일을 읽지 못했습니다.")?;
    if bytes.len() as u64 > FILE_MAX {
        return Err("파일 크기 제한을 초과했습니다.".into());
    }
    Ok(bytes)
}
fn root(path: &str) -> Result<PathBuf, String> {
    let path = Path::new(path);
    if !path.is_absolute() || !regular(path)?.is_dir() {
        return Err("기존 라이브러리 폴더의 절대 경로를 입력하세요.".into());
    }
    path.canonicalize()
        .map_err(|_| "라이브러리 경로를 확인하지 못했습니다.".into())
}
fn storage(root: &Path) -> Result<PathBuf, String> {
    let path = root.join(".backpack-library");
    if !regular(&path)?.is_dir() {
        return Err("Backpack 라이브러리 폴더가 아닙니다.".into());
    }
    if read(&path.join("format.json"))? != b"{\"version\":1}\n" {
        return Err("지원하지 않는 라이브러리 형식입니다.".into());
    }
    if !regular(&path.join("items"))?.is_dir() {
        return Err("라이브러리 항목 폴더를 확인하세요.".into());
    }
    Ok(path)
}
fn write_new(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|_| "파일을 새로 저장하지 못했습니다.")?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|_| "파일 저장을 완료하지 못했습니다.".into())
}
pub fn connect(path: &str) -> Result<Vec<LibraryEntry>, String> {
    let root = root(path)?;
    let directory = root.join(".backpack-library");
    if !directory.exists() {
        let temp = tempfile::Builder::new()
            .prefix(".backpack-init-")
            .tempdir_in(&root)
            .map_err(|_| "라이브러리를 초기화하지 못했습니다.")?;
        fs::create_dir(temp.path().join("items"))
            .map_err(|_| "라이브러리 항목 폴더를 만들지 못했습니다.")?;
        write_new(&temp.path().join("format.json"), b"{\"version\":1}\n")?;
        fs::rename(temp.path(), &directory)
            .map_err(|_| "라이브러리 초기화가 충돌했습니다. 다시 연결하세요.")?;
    }
    list(path)
}
fn valid_path(path: &str) -> bool {
    !path.is_empty()
        && path.len() < 512
        && !path.contains(['\\', ':'])
        && !path.chars().any(char::is_control)
        && Path::new(path)
            .components()
            .all(|p| matches!(p, Component::Normal(_)))
        && path
            .split('/')
            .all(|p| !p.is_empty() && !p.ends_with(['.', ' ']) && portable_name(p))
}
fn portable_name(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or("").to_ascii_uppercase();
    ![
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ]
    .contains(&stem.as_str())
}
fn entry(storage: &Path, id: &str) -> Result<LibraryEntry, String> {
    if !id.starts_with("item-")
        || id.len() > 80
        || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
    {
        return Err("라이브러리 항목 ID가 올바르지 않습니다.".into());
    }
    let directory = storage.join("items").join(id);
    regular(&directory)?;
    let value: LibraryEntry = serde_json::from_slice(&read(&directory.join("manifest.json"))?)
        .map_err(|_| "항목 기록 형식을 해석하지 못했습니다.")?;
    let mut names = HashSet::new();
    if value.version != 1
        || value.id != id
        || value.files.is_empty()
        || value.files.len() > 128
        || value.name.len() > 256
        || value.agent_id.len() > 64
        || !supported(&value.kind)
        || value.files.iter().any(|f| {
            !valid_path(&f.path)
                || !names.insert(f.path.to_lowercase())
                || f.size as u64 > FILE_MAX
                || f.hash.len() != 64
                || !f.hash.bytes().all(|b| b.is_ascii_hexdigit())
        })
        || value.files.iter().map(|f| f.size).sum::<usize>() > TOTAL_MAX
    {
        return Err("지원하지 않거나 잘못된 라이브러리 항목입니다.".into());
    }
    Ok(value)
}
pub fn list(path: &str) -> Result<Vec<LibraryEntry>, String> {
    let store = storage(&root(path)?)?;
    let mut out = vec![];
    for child in fs::read_dir(store.join("items"))
        .map_err(|_| "라이브러리 목록을 읽지 못했습니다.")?
        .take(201)
    {
        let child = child.map_err(|_| "항목에 접근하지 못했습니다.")?;
        let id = child.file_name().to_string_lossy().into_owned();
        if id.starts_with(".pending-") {
            continue;
        }
        out.push(entry(&store, &id)?);
        if out.len() > 200 {
            return Err("라이브러리 항목은 최대 200개까지 지원합니다.".into());
        }
    }
    out.sort_by(|a, b| (b.created_at, &b.id).cmp(&(a.created_at, &a.id)));
    Ok(out)
}
fn supported(kind: &str) -> bool {
    ["skill", "rule", "tool", "hook", "plugin"].contains(&kind)
}
fn resource(home: &Path, request: &ScanRequest, id: &str) -> Result<Resource, String> {
    let item = scan_at(home, request.clone())?
        .agents
        .into_iter()
        .flat_map(|a| a.resources)
        .find(|r| r.id == id)
        .ok_or("탐지 결과에서 선택한 항목을 다시 찾지 못했습니다.")?;
    if !supported(&item.kind) || item.origin == "bundled" {
        return Err("사용자 스킬·룰·도구·hook·플러그인 파일만 가져올 수 있습니다.".into());
    }
    Ok(item)
}
fn blocked(name: &str) -> bool {
    let name = name.to_lowercase();
    name.starts_with(".env")
        || ["credentials.json", "auth.json", "tokens.json"].contains(&name.as_str())
        || ["pem", "key", "p12", "pfx", "db", "sqlite"].contains(
            &Path::new(&name)
                .extension()
                .and_then(|v| v.to_str())
                .unwrap_or(""),
        )
}
fn text_file(path: &str, bytes: &[u8]) -> Result<(), String> {
    let ext = Path::new(path)
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_lowercase();
    if ![
        "md", "mdc", "rules", "txt", "json", "jsonc", "toml", "yaml", "yml", "ts", "tsx", "js",
        "mjs", "cjs", "py", "sh", "ps1", "bat", "cmd", "rs", "go", "html", "css", "svg", "sql",
    ]
    .contains(&ext.as_str())
    {
        return Err("현재 가져오기는 문서·설정·소스 텍스트 파일만 지원합니다.".into());
    }
    let text = std::str::from_utf8(bytes).map_err(|_| "UTF-8 텍스트 파일만 가져올 수 있습니다.")?;
    if text.contains('\0')
        || [
            "-----BEGIN PRIVATE KEY",
            "-----BEGIN RSA PRIVATE KEY",
            "ghp_",
            "github_pat_",
            "sk-proj-",
        ]
        .iter()
        .any(|p| text.contains(p))
    {
        return Err("파일에 키·토큰으로 보이는 값이 있어 가져오기를 중단했습니다. 원본에서 환경 변수 참조로 분리하세요.".into());
    }
    Ok(())
}
fn gather_dir(
    base: &Path,
    current: &Path,
    depth: usize,
    out: &mut BTreeMap<String, Vec<u8>>,
    skipped: &mut Vec<String>,
    count: &mut usize,
) -> Result<(), String> {
    if depth > 8 {
        return Err("스킬 폴더 깊이 제한을 초과했습니다.".into());
    }
    for child in fs::read_dir(current).map_err(|_| "스킬 폴더를 읽지 못했습니다.")? {
        *count += 1;
        if *count > 512 {
            return Err("스킬 폴더 탐지 제한을 초과했습니다.".into());
        }
        let path = child
            .map_err(|_| "스킬 파일에 접근하지 못했습니다.")?
            .path();
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        let relative = path
            .strip_prefix(base)
            .map_err(|_| "파일 경로가 범위를 벗어났습니다.")?
            .to_string_lossy()
            .replace('\\', "/");
        let meta = regular(&path)?;
        if blocked(&name)
            || [".git", "node_modules", ".venv", "__pycache__", ".DS_Store"]
                .contains(&name.as_ref())
        {
            skipped.push(relative);
            continue;
        }
        if !valid_path(&relative) {
            return Err("공유할 수 없는 상대 파일 경로입니다.".into());
        }
        if meta.is_dir() {
            gather_dir(base, &path, depth + 1, out, skipped, count)?;
        } else {
            let bytes = read(&path)?;
            text_file(&relative, &bytes)?;
            out.insert(relative, bytes);
        }
        if out.len() > 128 || out.values().map(Vec::len).sum::<usize>() > TOTAL_MAX {
            return Err("가져오기는 128개 파일·총 16MB까지 지원합니다.".into());
        }
    }
    Ok(())
}
type ImportPayload = (BTreeMap<String, Vec<u8>>, Vec<String>, String);
fn payload(item: &Resource) -> Result<ImportPayload, String> {
    let path = Path::new(&item.path);
    let original = read(path)?;
    let mut out = BTreeMap::new();
    let mut skipped = vec![];
    if item.kind == "skill" {
        let base = path.parent().ok_or("스킬 폴더가 없습니다.")?;
        regular(base)?;
        gather_dir(base, base, 0, &mut out, &mut skipped, &mut 0)?;
    } else if item.kind == "hook"
        && [Some("json"), Some("jsonc")].contains(&path.extension().and_then(|v| v.to_str()))
    {
        let value: serde_json::Value = json5::from_str(
            std::str::from_utf8(&original).map_err(|_| "UTF-8 파일만 지원합니다.")?,
        )
        .map_err(|_| "hook 설정 형식을 해석하지 못했습니다.")?;
        let isolated = if let Some(hook) = value.get("hooks").and_then(|v| v.get(&item.name)) {
            serde_json::json!({"hooks":{item.name.clone():hook}})
        } else {
            let definition = value.get(&item.name).ok_or("해당 hook 정의가 없습니다.")?;
            if !definition
                .as_object()
                .is_some_and(|events| events.values().any(serde_json::Value::is_array))
            {
                return Err("이 항목에서 hook 이벤트 배열을 확인하지 못했습니다. 인증 설정 등은 가져오지 않습니다.".into());
            }
            serde_json::json!({item.name.clone():definition})
        };
        let bytes =
            serde_json::to_vec_pretty(&isolated).map_err(|_| "hook 정의를 추출하지 못했습니다.")?;
        text_file("hook.json", &bytes)?;
        out.insert("hook.json".into(), bytes);
    } else {
        let name = path
            .file_name()
            .ok_or("파일 이름이 없습니다.")?
            .to_string_lossy()
            .into_owned();
        if blocked(&name) || !valid_path(&name) {
            return Err("인증 파일은 가져올 수 없습니다.".into());
        }
        text_file(&name, &original)?;
        out.insert(name, original.clone());
    }
    let mut hash = Sha256::new();
    hash.update(&original);
    for (path, bytes) in &out {
        hash.update(path.as_bytes());
        hash.update(digest(bytes));
    }
    skipped.sort();
    for path in &skipped {
        hash.update(path.as_bytes());
    }
    let fingerprint = format!("{:x}", hash.finalize());
    Ok((out, skipped, fingerprint))
}
fn files(payload: &BTreeMap<String, Vec<u8>>) -> Vec<LibraryFile> {
    payload
        .iter()
        .map(|(path, bytes)| LibraryFile {
            path: path.clone(),
            hash: digest(bytes),
            size: bytes.len(),
        })
        .collect()
}
pub fn prepare_at(
    home: &Path,
    request: ScanRequest,
    library_path: &str,
    resource_id: &str,
) -> Result<PreparedImport, String> {
    let library = root(library_path)?;
    storage(&library)?;
    if list(library_path)?.len() >= 200 {
        return Err("라이브러리 항목 제한을 초과했습니다.".into());
    }
    let item = resource(home, &request, resource_id)?;
    if item.name.len() > 256 {
        return Err("리소스 이름이 길어 현재 라이브러리에 저장할 수 없습니다.".into());
    }
    let source = Path::new(&item.path)
        .canonicalize()
        .map_err(|_| "원본 경로를 확인하지 못했습니다.")?;
    if source.starts_with(&library)
        || library.starts_with(source.parent().ok_or("원본 경로가 올바르지 않습니다.")?)
    {
        return Err("원본 리소스 폴더와 라이브러리 폴더는 분리하세요.".into());
    }
    let (payload, skipped, fingerprint) = payload(&item)?;
    let mut case = HashSet::new();
    if payload.keys().any(|p| !case.insert(p.to_lowercase())) {
        return Err("대소문자만 다른 파일은 세 OS에서 공유할 수 없습니다.".into());
    }
    let view=ImportPreview{name:item.name,kind:item.kind.clone(),agent_id:item.agent_id,files:files(&payload),skipped,total_bytes:payload.values().map(Vec::len).sum(),note:if item.kind=="hook" {"선택한 hook 정의만 저장합니다. 호출하는 외부 스크립트·인증 값·PC별 절대 경로는 자동으로 이식하지 않습니다."} else {"파일 내용을 그대로 저장합니다. 알려진 인증 파일·캐시는 제외하지만 비밀 값을 모두 판별할 수는 없습니다. 공유할 파일을 검토하세요."}.into()};
    Ok(PreparedImport {
        view,
        root: library,
        home: home.into(),
        request,
        resource_id: resource_id.into(),
        payload,
        fingerprint,
    })
}
pub fn import(prepared: PreparedImport) -> Result<LibraryEntry, String> {
    let store = storage(&prepared.root)?;
    let item = resource(&prepared.home, &prepared.request, &prepared.resource_id)?;
    if payload(&item)?.2 != prepared.fingerprint {
        return Err("미리보기 이후 원본이 변경됐습니다. 다시 확인하세요.".into());
    }
    let directory = tempfile::Builder::new()
        .prefix(".pending-")
        .tempdir_in(store.join("items"))
        .map_err(|_| "라이브러리 항목을 만들지 못했습니다.")?;
    let id = directory
        .path()
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
        .replace(".pending-", "item-");
    let data = directory.path().join("files");
    fs::create_dir(&data).map_err(|_| "파일 묶음 폴더를 만들지 못했습니다.")?;
    for (name, bytes) in &prepared.payload {
        let target = data.join(name);
        fs::create_dir_all(target.parent().ok_or("파일 경로가 올바르지 않습니다.")?)
            .map_err(|_| "파일 폴더를 만들지 못했습니다.")?;
        write_new(&target, bytes)?;
    }
    let entry = LibraryEntry {
        version: 1,
        id,
        name: prepared.view.name,
        agent_id: prepared.view.agent_id,
        kind: prepared.view.kind,
        created_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        files: prepared.view.files,
    };
    write_new(
        &directory.path().join("manifest.json"),
        &serde_json::to_vec_pretty(&entry).map_err(|_| "항목 기록을 저장하지 못했습니다.")?,
    )?;
    if payload(&item)?.2 != prepared.fingerprint {
        return Err("저장 직전에 원본이 변경됐습니다. 다시 확인하세요.".into());
    }
    fs::rename(directory.path(), store.join("items").join(&entry.id))
        .map_err(|_| "라이브러리 항목 발행을 완료하지 못했습니다.")?;
    let _ = directory.keep();
    Ok(entry)
}
pub fn compare_at(
    home: &Path,
    request: &ScanRequest,
    path: &str,
    id: &str,
    resource_id: &str,
) -> Result<Comparison, String> {
    let store = storage(&root(path)?)?;
    let saved = entry(&store, id)?;
    let local = resource(home, request, resource_id)?;
    if local.kind != saved.kind {
        return Err("같은 종류의 리소스끼리 비교하세요.".into());
    }
    let current = payload(&local)?.0;
    let current = files(&current)
        .into_iter()
        .map(|f| (f.path, f.hash))
        .collect::<BTreeMap<_, _>>();
    let mut result = vec![];
    let mut pending = current.clone();
    for file in saved.files {
        let target = store.join("items").join(id).join("files").join(&file.path);
        let base = store.join("items").join(id).join("files");
        regular(&base)?;
        let mut parent = target.parent();
        while let Some(dir) = parent {
            regular(dir)?;
            if dir == base {
                break;
            }
            parent = dir.parent();
        }
        let bytes = read(&target)?;
        if digest(&bytes) != file.hash || bytes.len() != file.size {
            return Err("라이브러리 파일 무결성 검사를 통과하지 못했습니다.".into());
        }
        let status = match pending.remove(&file.path) {
            Some(hash) if hash == file.hash => "equal",
            Some(_) => "changed",
            None => "libraryOnly",
        };
        result.push(FileComparison {
            path: file.path,
            status: status.into(),
        });
    }
    for (path, _) in pending {
        result.push(FileComparison {
            path,
            status: "localOnly".into(),
        });
    }
    let identical = result.iter().all(|f| f.status == "equal");
    Ok(Comparison {
        files: result,
        identical,
    })
}
