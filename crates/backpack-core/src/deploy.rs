use crate::{
    display,
    library::{self, LibraryEntry, LibraryFile},
    scan_at, ScanRequest,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Write,
    path::{Path, PathBuf},
};
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallTarget {
    pub id: String,
    pub agent_id: String,
    pub scope: String,
    pub path: String,
    pub base: String,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeploymentView {
    pub action: String,
    pub name: String,
    pub agent_id: String,
    pub scope: String,
    pub path: String,
    pub files: Vec<LibraryFile>,
    pub note: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Installation {
    pub id: String,
    pub item_id: String,
    pub name: String,
    pub agent_id: String,
    pub scope: String,
    pub path: String,
    pub state: String,
    pub can_remove: bool,
    pub can_restore: bool,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeploymentData {
    pub targets: Vec<InstallTarget>,
    pub installations: Vec<Installation>,
    pub notice: String,
}
#[derive(Clone, Serialize, Deserialize)]
struct Record {
    version: u8,
    id: String,
    entry: LibraryEntry,
    target: InstallTarget,
}
pub struct PreparedDeployment {
    pub view: DeploymentView,
    record: Record,
    payload: BTreeMap<String, Vec<u8>>,
    anchor: PathBuf,
    canonical: PathBuf,
    records: PathBuf,
    library: Option<String>,
}
pub fn record_root() -> Result<PathBuf, String> {
    Ok(crate::changes::backup_root()?
        .parent()
        .ok_or("앱 데이터 경로가 올바르지 않습니다.")?
        .join("installations"))
}
fn absent(path: &Path) -> Result<bool, String> {
    match fs::symlink_metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(true),
        Ok(_) => Ok(false),
        Err(_) => Err("대상 경로에 접근하지 못했습니다.".into()),
    }
}
fn parents(path: &Path) -> Result<(), String> {
    for ancestor in path.ancestors().skip(1) {
        match fs::symlink_metadata(ancestor) {
            Ok(meta) if !meta.is_dir() || meta.file_type().is_symlink() => {
                return Err("설치 경로의 링크·일반 파일을 폴더로 사용할 수 없습니다.".into())
            }
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err("설치 경로에 접근하지 못했습니다.".into()),
        }
    }
    Ok(())
}
fn anchor(path: &Path) -> Result<(PathBuf, PathBuf), String> {
    parents(path)?;
    let parent = path
        .ancestors()
        .skip(1)
        .find(|p| p.is_dir())
        .ok_or("설치 부모 폴더가 없습니다.")?;
    Ok((
        parent.into(),
        parent
            .canonicalize()
            .map_err(|_| "설치 폴더를 확인하지 못했습니다.")?,
    ))
}
fn check_anchor(prepared: &PreparedDeployment) -> Result<(), String> {
    if prepared.anchor.canonicalize().ok().as_ref() != Some(&prepared.canonical) {
        return Err("미리보기 이후 설치 폴더가 변경됐습니다.".into());
    }
    parents(Path::new(&prepared.record.target.path))?;
    Ok(())
}
fn skill_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && !name.starts_with('-')
        && !name.ends_with('-')
        && !name.contains("--")
}
fn eligible(entry: &LibraryEntry) -> bool {
    if entry.kind == "skill" {
        return skill_name(&entry.name) && entry.files.iter().any(|f| f.path == "SKILL.md");
    }
    if entry.files.len() != 1
        || !library::valid_path(&entry.files[0].path)
        || entry.files[0].path.contains('/')
    {
        return false;
    }
    let ext = Path::new(&entry.files[0].path)
        .extension()
        .and_then(|v| v.to_str());
    (entry.kind == "rule" && entry.agent_id == "claude" && ext == Some("md"))
        || (["tool", "plugin"].contains(&entry.kind.as_str())
            && entry.agent_id == "opencode"
            && [Some("ts"), Some("js")].contains(&ext))
}
fn targets(
    home: &Path,
    request: &ScanRequest,
    entry: &LibraryEntry,
) -> Result<Vec<InstallTarget>, String> {
    let snapshot = scan_at(home, request.clone())?;
    let mut out = vec![];
    if !eligible(entry) {
        return Ok(out);
    }
    for agent in &snapshot.agents {
        if agent.custom
            || !if entry.kind == "skill" {
                ["codex", "claude", "antigravity", "opencode", "gemini"]
                    .contains(&agent.id.as_str())
            } else {
                agent.id == entry.agent_id
            }
        {
            continue;
        }
        for scope in ["user", "project"] {
            let base = if scope == "user" {
                if agent.id == "codex" {
                    home.join(".agents")
                } else if agent.id == "antigravity" {
                    Path::new(&agent.config_roots[0]).join("config")
                } else {
                    PathBuf::from(&agent.config_roots[0])
                }
            } else {
                let Some(project) = snapshot.project_path.as_ref() else {
                    continue;
                };
                Path::new(project).join(match agent.id.as_str() {
                    "codex" | "antigravity" => ".agents",
                    "claude" => ".claude",
                    "gemini" => ".gemini",
                    _ => ".opencode",
                })
            };
            let container = base.join(match entry.kind.as_str() {
                "skill" => "skills",
                "rule" => "rules",
                "tool" => "tools",
                _ => "plugins",
            });
            let target = container.join(if entry.kind == "skill" {
                entry.name.as_str()
            } else {
                entry.files[0].path.as_str()
            });
            if !target.is_absolute() {
                return Err("설치 경로는 절대 경로여야 합니다.".into());
            }
            out.push(InstallTarget {
                id: format!("{}:{scope}", agent.id),
                agent_id: agent.id.clone(),
                scope: scope.into(),
                path: display(&target),
                base: display(&base),
            });
        }
    }
    Ok(out)
}
fn target_allowed(home: &Path, request: &ScanRequest, record: &Record) -> Result<(), String> {
    if !targets(home, request, &record.entry)?.iter().any(|t| {
        t.id == record.target.id
            && t.agent_id == record.target.agent_id
            && t.scope == record.target.scope
            && t.path == record.target.path
            && t.base == record.target.base
    }) {
        return Err("현재 탐지 문맥에 속한 설치 기록만 변경할 수 있습니다.".into());
    }
    Ok(())
}
fn quarantine(record: &Record) -> PathBuf {
    Path::new(&record.target.base)
        .join(".backpack-disabled")
        .join(&record.id)
}
fn walk(
    base: &Path,
    path: &Path,
    depth: usize,
    out: &mut BTreeMap<String, String>,
    dirs: &mut BTreeSet<String>,
) -> Result<(), String> {
    if depth > 8 || out.len() > 128 || dirs.len() > 512 {
        return Err("설치 파일 검사 제한을 초과했습니다.".into());
    }
    for (index, child) in fs::read_dir(path)
        .map_err(|_| "설치 파일 목록을 읽지 못했습니다.")?
        .take(513)
        .enumerate()
    {
        if index >= 512 {
            return Err("설치 폴더 항목 제한을 초과했습니다.".into());
        }
        let path = child
            .map_err(|_| "설치 항목에 접근하지 못했습니다.")?
            .path();
        let meta = library::regular(&path)?;
        let relative = path
            .strip_prefix(base)
            .map_err(|_| "설치 파일 범위를 벗어났습니다.")?
            .to_string_lossy()
            .replace('\\', "/");
        if meta.is_dir() {
            dirs.insert(relative);
            walk(base, &path, depth + 1, out, dirs)?;
        } else {
            if out.len() >= 128 {
                return Err("설치 파일 개수 제한을 초과했습니다.".into());
            }
            out.insert(relative, library::digest(&library::read(&path)?));
        }
    }
    Ok(())
}
fn matches(path: &Path, entry: &LibraryEntry) -> bool {
    if parents(path).is_err() {
        return false;
    }
    if entry.kind != "skill" {
        return library::read(path).is_ok_and(|b| {
            b.len() == entry.files[0].size && library::digest(&b) == entry.files[0].hash
        });
    }
    if !library::regular(path).is_ok_and(|m| m.is_dir()) {
        return false;
    }
    let mut actual = BTreeMap::new();
    let mut dirs = BTreeSet::new();
    if walk(path, path, 0, &mut actual, &mut dirs).is_err() {
        return false;
    }
    let expected = entry
        .files
        .iter()
        .map(|f| (f.path.clone(), f.hash.clone()))
        .collect::<BTreeMap<_, _>>();
    let mut expected_dirs = BTreeSet::new();
    for f in &entry.files {
        let mut p = Path::new(&f.path).parent();
        while let Some(dir) = p {
            if !dir.as_os_str().is_empty() {
                expected_dirs.insert(dir.to_string_lossy().replace('\\', "/"));
            }
            p = dir.parent();
        }
    }
    actual == expected && dirs == expected_dirs
}
fn record(records: &Path, id: &str) -> Result<Record, String> {
    if !id.starts_with("install-")
        || id.len() > 80
        || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
    {
        return Err("설치 기록 ID가 올바르지 않습니다.".into());
    }
    let folder = records.join(id);
    library::regular(&folder)?;
    let r: Record = serde_json::from_slice(&library::read(&folder.join("record.json"))?)
        .map_err(|_| "설치 기록 형식을 해석하지 못했습니다.")?;
    let unique = r
        .entry
        .files
        .iter()
        .map(|f| f.path.to_ascii_lowercase())
        .collect::<BTreeSet<_>>();
    if unique.len() != r.entry.files.len()
        || r.version != 1
        || r.id != id
        || !eligible(&r.entry)
        || r.entry.files.len() > 128
        || r.entry.files.iter().any(|f| {
            !library::valid_path(&f.path)
                || f.size > 2 * 1024 * 1024
                || f.hash.len() != 64
                || !f.hash.bytes().all(|b| b.is_ascii_hexdigit())
        })
        || r.entry.files.iter().map(|f| f.size).sum::<usize>() > 16 * 1024 * 1024
    {
        return Err("잘못된 설치 기록입니다.".into());
    }
    Ok(r)
}
fn receipt(r: &Record) -> Installation {
    let target = Path::new(&r.target.path);
    let removed = quarantine(r);
    let target_absent = absent(target).unwrap_or(false);
    let removed_absent = absent(&removed).unwrap_or(false);
    let installed = !target_absent && removed_absent && matches(target, &r.entry);
    let disabled = target_absent && !removed_absent && matches(&removed, &r.entry);
    Installation {
        id: r.id.clone(),
        item_id: r.entry.id.clone(),
        name: r.entry.name.clone(),
        agent_id: r.target.agent_id.clone(),
        scope: r.target.scope.clone(),
        path: r.target.path.clone(),
        state: if installed {
            "installed"
        } else if disabled {
            "disabled"
        } else if target_absent && removed_absent {
            "missing"
        } else {
            "changed"
        }
        .into(),
        can_remove: installed,
        can_restore: disabled,
    }
}
pub fn data_at(
    home: &Path,
    request: &ScanRequest,
    path: &str,
    item_id: &str,
    records: &Path,
) -> Result<DeploymentData, String> {
    let (entry, _) = library::load_verified(path, item_id)?;
    let mut installations = vec![];
    match fs::read_dir(records) {
        Ok(entries) => {
            for child in entries.take(1000).flatten() {
                let id = child.file_name().to_string_lossy().into_owned();
                let Ok(r) = record(records, &id) else {
                    continue;
                };
                if r.entry.id == item_id && target_allowed(home, request, &r).is_ok() {
                    installations.push(receipt(&r));
                }
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err("설치 기록 폴더를 읽지 못했습니다.".into()),
    }
    Ok(DeploymentData{targets:targets(home,request,&entry)?,installations,notice:"새 경로에만 설치합니다. 스킬은 다섯 에이전트, Markdown 룰은 Claude, JS·TS 도구·플러그인은 OpenCode를 지원합니다. hooks와 형식 변환은 미지원입니다. 파일 배치가 실행 중 세션의 로드를 보장하지 않으며 플러그인은 에이전트가 읽을 때 실행될 수 있습니다. 공통 .agents/skills 경로는 다른 에이전트도 읽을 수 있습니다.".into()})
}
fn view(record: &Record, action: &str) -> DeploymentView {
    DeploymentView{action:action.into(),name:record.entry.name.clone(),agent_id:record.target.agent_id.clone(),scope:record.target.scope.clone(),path:record.target.path.clone(),files:record.entry.files.clone(),note:if action=="remove" {"수정되지 않은 관리 항목만 에이전트 설정 폴더의 .backpack-disabled로 옮겨 보관합니다. 원래 위치가 비어 있으면 복원할 수 있습니다. 현재 세션에는 다시 로드가 필요할 수 있습니다."} else {"검증한 파일을 배치합니다. 기존 파일은 덮어쓰지 않습니다. 스크립트·의존성을 설치하거나 실행하지 않으며 설정 재로드 때 에이전트가 코드를 실행할 수 있습니다."}.into()}
}
pub fn prepare_install_at(
    home: &Path,
    request: &ScanRequest,
    path: &str,
    item_id: &str,
    target_id: &str,
    records: &Path,
) -> Result<PreparedDeployment, String> {
    let (entry, payload) = library::load_verified(path, item_id)?;
    let target = targets(home, request, &entry)?
        .into_iter()
        .find(|t| t.id == target_id)
        .ok_or("지원하지 않는 설치 대상입니다.")?;
    let library_path = Path::new(path)
        .canonicalize()
        .map_err(|_| "라이브러리 경로를 확인하지 못했습니다.")?;
    let destination = Path::new(&target.path);
    let (existing, resolved) = anchor(destination)?;
    let resolved_target = resolved.join(
        destination
            .strip_prefix(existing)
            .map_err(|_| "설치 경로를 확인하지 못했습니다.")?,
    );
    if resolved_target.starts_with(&library_path) || library_path.starts_with(&resolved_target) {
        return Err("라이브러리와 설치 폴더가 겹칩니다.".into());
    }
    if !absent(Path::new(&target.path))? {
        return Err("설치 위치에 기존 항목이 있습니다. 덮어쓰지 않습니다.".into());
    }
    if entry.kind == "skill" {
        let skill = std::str::from_utf8(payload.get("SKILL.md").ok_or("SKILL.md가 없습니다.")?)
            .map_err(|_| "스킬 파일은 UTF-8이어야 합니다.")?;
        let front = skill
            .strip_prefix("---\n")
            .or_else(|| skill.strip_prefix("---\r\n"))
            .and_then(|s| s.split_once("\n---").map(|(front, _)| front));
        if !front.is_some_and(|f| {
            f.lines().any(|l| {
                l.strip_prefix("name:")
                    .is_some_and(|v| v.trim().trim_matches(['\'', '"']) == entry.name)
            }) && f.lines().any(|l| {
                l.strip_prefix("description:")
                    .is_some_and(|v| !v.trim().is_empty())
            })
        }) {
            return Err(
                "스킬의 name·description YAML 머리말을 확인하세요. 자동으로 변환하지 않습니다."
                    .into(),
            );
        }
    }
    let (anchor, canonical) = anchor(Path::new(&target.path))?;
    let record = Record {
        version: 1,
        id: String::new(),
        entry,
        target,
    };
    Ok(PreparedDeployment {
        view: view(&record, "install"),
        record,
        payload,
        anchor,
        canonical,
        records: records.into(),
        library: Some(path.into()),
    })
}
pub fn prepare_existing_at(
    home: &Path,
    request: &ScanRequest,
    id: &str,
    action: &str,
    records: &Path,
) -> Result<PreparedDeployment, String> {
    let record = record(records, id)?;
    target_allowed(home, request, &record)?;
    let current = receipt(&record);
    if (action == "remove" && !current.can_remove)
        || (action == "restore" && !current.can_restore)
        || !["remove", "restore"].contains(&action)
    {
        return Err(
            "현재 파일 상태에서는 작업할 수 없습니다. 외부 변경이나 경로 충돌을 확인하세요.".into(),
        );
    }
    let (anchor, canonical) = anchor(Path::new(&record.target.path))?;
    Ok(PreparedDeployment {
        view: view(&record, action),
        record,
        payload: BTreeMap::new(),
        anchor,
        canonical,
        records: records.into(),
        library: None,
    })
}
fn create_file(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .map_err(|_| "파일을 새로 만들지 못했습니다.".to_string())?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|_| "파일 저장을 완료하지 못했습니다.".into())
}
fn private_dir(path: &Path) -> Result<(), String> {
    parents(&path.join("placeholder"))?;
    fs::create_dir_all(path).map_err(|_| "설치 기록 폴더를 만들지 못했습니다.")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .map_err(|_| "기록 폴더 권한을 설정하지 못했습니다.")?;
    }
    Ok(())
}
fn rename_new(source: &Path, target: &Path) -> Result<(), String> {
    #[cfg(windows)]
    let result = {
        use std::os::windows::ffi::OsStrExt;
        let from = source
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>();
        let to = target
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>();
        unsafe {
            windows_sys::Win32::Storage::FileSystem::MoveFileW(from.as_ptr(), to.as_ptr()) != 0
        }
    };
    #[cfg(target_os = "linux")]
    let result = {
        use std::os::unix::ffi::OsStrExt;
        let from = std::ffi::CString::new(source.as_os_str().as_bytes())
            .map_err(|_| "이동 경로가 올바르지 않습니다.")?;
        let to = std::ffi::CString::new(target.as_os_str().as_bytes())
            .map_err(|_| "이동 경로가 올바르지 않습니다.")?;
        unsafe {
            libc::renameat2(
                libc::AT_FDCWD,
                from.as_ptr(),
                libc::AT_FDCWD,
                to.as_ptr(),
                libc::RENAME_NOREPLACE,
            ) == 0
        }
    };
    #[cfg(target_os = "macos")]
    let result = {
        use std::os::unix::ffi::OsStrExt;
        let from = std::ffi::CString::new(source.as_os_str().as_bytes())
            .map_err(|_| "이동 경로가 올바르지 않습니다.")?;
        let to = std::ffi::CString::new(target.as_os_str().as_bytes())
            .map_err(|_| "이동 경로가 올바르지 않습니다.")?;
        unsafe { libc::renamex_np(from.as_ptr(), to.as_ptr(), libc::RENAME_EXCL) == 0 }
    };
    if !result {
        return Err("기존 항목을 덮어쓰지 않고 이동하는 작업이 실패했습니다. 권한·다른 드라이브·경로 충돌을 확인하세요.".into());
    }
    Ok(())
}
pub fn apply(mut prepared: PreparedDeployment) -> Result<Installation, String> {
    check_anchor(&prepared)?;
    let target = PathBuf::from(&prepared.record.target.path);
    if prepared.view.action == "install" {
        let library = prepared
            .library
            .as_ref()
            .ok_or("라이브러리 문맥이 없습니다.")?;
        let (entry, payload) = library::load_verified(library, &prepared.record.entry.id)?;
        if serde_json::to_vec(&entry).ok() != serde_json::to_vec(&prepared.record.entry).ok()
            || payload != prepared.payload
            || !absent(&target)?
        {
            return Err("미리보기 이후 라이브러리 또는 설치 위치가 바뀌었습니다.".into());
        }
        private_dir(&prepared.records)?;
        let journal = tempfile::Builder::new()
            .prefix("install-")
            .tempdir_in(&prepared.records)
            .map_err(|_| "설치 기록을 만들지 못했습니다.")?;
        prepared.record.id = journal
            .path()
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into();
        let backup = journal.path().join("files");
        fs::create_dir(&backup).map_err(|_| "설치 원본 보관 폴더를 만들지 못했습니다.")?;
        for (name, bytes) in &prepared.payload {
            let file = backup.join(name);
            fs::create_dir_all(file.parent().ok_or("백업 경로가 올바르지 않습니다.")?)
                .map_err(|_| "백업 폴더를 만들지 못했습니다.")?;
            create_file(&file, bytes)?;
        }
        create_file(
            &journal.path().join("record.json"),
            &serde_json::to_vec(&prepared.record)
                .map_err(|_| "설치 기록을 작성하지 못했습니다.")?,
        )?;
        let parent = target.parent().ok_or("설치 경로가 올바르지 않습니다.")?;
        check_anchor(&prepared)?;
        fs::create_dir_all(parent).map_err(|_| "설치 폴더를 만들지 못했습니다.")?;
        let stage = tempfile::Builder::new()
            .prefix(".backpack-install-")
            .tempdir_in(&prepared.record.target.base)
            .map_err(|_| "설치 임시 폴더를 만들지 못했습니다.")?;
        let staged = stage.path().join("payload");
        if entry.kind == "skill" {
            fs::create_dir(&staged).map_err(|_| "스킬 임시 폴더를 만들지 못했습니다.")?;
            for (name, bytes) in &prepared.payload {
                let file = staged.join(name);
                fs::create_dir_all(file.parent().ok_or("설치 파일 경로가 올바르지 않습니다.")?)
                    .map_err(|_| "설치 파일 폴더를 만들지 못했습니다.")?;
                create_file(&file, bytes)?;
            }
        } else {
            create_file(
                &staged,
                prepared.payload.values().next().ok_or("파일이 없습니다.")?,
            )?;
        }
        check_anchor(&prepared)?;
        if !absent(&target)? {
            return Err("설치 직전에 다른 항목이 생겼습니다.".into());
        }
        let _ = journal.keep();
        rename_new(&staged, &target)?;
    } else {
        let current = receipt(&prepared.record);
        let removed = quarantine(&prepared.record);
        if prepared.view.action == "remove" {
            if !current.can_remove {
                return Err("미리보기 이후 설치 파일이 변경됐습니다. 해제를 중단합니다.".into());
            }
            parents(&removed)?;
            fs::create_dir_all(removed.parent().ok_or("보관 경로가 올바르지 않습니다.")?)
                .map_err(|_| "해제 보관 폴더를 만들지 못했습니다.")?;
            check_anchor(&prepared)?;
            if !matches(&target, &prepared.record.entry) {
                return Err("해제 직전에 파일이 변경됐습니다.".into());
            }
            rename_new(&target, &removed)?;
        } else {
            if !current.can_restore {
                return Err("해제 항목이나 복원 위치가 바뀌어 복원을 중단합니다.".into());
            }
            check_anchor(&prepared)?;
            parents(&removed)?;
            if !matches(&removed, &prepared.record.entry) {
                return Err("복원 직전에 보관 파일이 변경됐습니다.".into());
            }
            fs::create_dir_all(target.parent().ok_or("복원 경로가 올바르지 않습니다.")?)
                .map_err(|_| "복원 폴더를 만들지 못했습니다.")?;
            check_anchor(&prepared)?;
            rename_new(&removed, &target)?;
        }
    }
    Ok(receipt(&prepared.record))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn atomic_move_refuses_existing_file_and_empty_directory() {
        let dir = tempfile::TempDir::new().unwrap();
        let source = dir.path().join("source");
        let target = dir.path().join("target");
        fs::write(&source, "source").unwrap();
        fs::write(&target, "existing").unwrap();
        assert!(rename_new(&source, &target).is_err());
        assert_eq!(fs::read_to_string(&target).unwrap(), "existing");
        assert!(source.exists());
        fs::remove_file(&source).unwrap();
        fs::remove_file(&target).unwrap();
        fs::create_dir(&source).unwrap();
        fs::create_dir(&target).unwrap();
        assert!(rename_new(&source, &target).is_err());
        assert!(source.exists());
        assert!(target.exists());
        fs::remove_dir(&target).unwrap();
        rename_new(&source, &target).unwrap();
        assert!(!source.exists());
        assert!(target.exists());
    }
}
