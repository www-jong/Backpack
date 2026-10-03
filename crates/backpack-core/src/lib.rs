use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeMap, HashSet},
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

const MAX_BYTES: u64 = 2 * 1024 * 1024;
const MAX_ENTRIES: usize = 2000;

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanRequest {
    pub project_path: Option<String>,
    #[serde(default)]
    pub roots: BTreeMap<String, String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub scanned_at: u64,
    pub platform: String,
    pub home: String,
    pub project_path: Option<String>,
    pub agents: Vec<Agent>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Agent {
    pub id: String,
    pub name: String,
    pub executable: Option<String>,
    pub config_roots: Vec<String>,
    pub resources: Vec<Resource>,
    pub warnings: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Resource {
    pub id: String,
    pub agent_id: String,
    pub kind: String,
    pub name: String,
    pub path: String,
    pub scope: String,
    pub status: String,
    pub source: String,
    pub origin: String,
    pub details: Vec<Detail>,
}

#[derive(Serialize)]
pub struct Detail {
    pub label: String,
    pub value: String,
}

pub fn scan(request: ScanRequest) -> Result<Snapshot, String> {
    let home = dirs::home_dir().ok_or("사용자 홈 경로를 찾을 수 없습니다.")?;
    scan_at(&home, request)
}

pub fn scan_at(home: &Path, request: ScanRequest) -> Result<Snapshot, String> {
    let project = request
        .project_path
        .as_ref()
        .filter(|s| !s.trim().is_empty())
        .map(PathBuf::from);
    if let Some(path) = &project {
        if !path.is_absolute() || !path.is_dir() {
            return Err("프로젝트 경로는 존재하는 폴더의 절대 경로여야 합니다.".into());
        }
    }
    for (id, root) in &request.roots {
        if !["codex", "claude", "antigravity", "opencode"].contains(&id.as_str()) {
            return Err("지원하지 않는 에이전트 경로입니다.".into());
        }
        if !Path::new(root).is_absolute() || !Path::new(root).is_dir() {
            return Err(format!(
                "{id}: 설정 루트는 존재하는 폴더의 절대 경로여야 합니다."
            ));
        }
    }
    let defs = [
        ("codex", "Codex", ".codex"),
        ("claude", "Claude Code", ".claude"),
        ("antigravity", "Antigravity", ".gemini"),
        ("opencode", "OpenCode", ".config/opencode"),
    ];
    let mut agents = Vec::new();
    for (id, name, default_root) in defs {
        let env_root = match id {
            "codex" => std::env::var_os("CODEX_HOME"),
            "claude" => std::env::var_os("CLAUDE_CONFIG_DIR"),
            "opencode" => std::env::var_os("OPENCODE_CONFIG_DIR").or_else(|| {
                std::env::var_os("XDG_CONFIG_HOME")
                    .map(|p| PathBuf::from(p).join("opencode").into_os_string())
            }),
            _ => None,
        };
        let root = request
            .roots
            .get(id)
            .map(PathBuf::from)
            .or_else(|| env_root.map(PathBuf::from))
            .unwrap_or_else(|| home.join(default_root));
        let mut agent = Agent {
            id: id.into(),
            name: name.into(),
            executable: find_executable(id, home),
            config_roots: vec![display(&root)],
            resources: vec![],
            warnings: vec![],
        };
        match id {
            "codex" => {
                scan_config(&mut agent, &root.join("config.toml"), "사용자");
                scan_skills(&mut agent, &root.join("skills"), "사용자");
                let system_start = agent.resources.len();
                scan_skills(&mut agent, &root.join("skills/.system"), "기본 제공");
                for resource in &mut agent.resources[system_start..] {
                    resource.origin = "bundled".into();
                    resource
                        .details
                        .push(detail("제공 근거", "Codex 시스템 스킬 경로"));
                    resource
                        .details
                        .push(detail("원본 비교", "수정 여부 미확인"));
                }
                scan_rules(&mut agent, &root.join("rules"), "사용자");
                scan_skills(&mut agent, &home.join(".agents/skills"), "공통 사용자");
            }
            "claude" => {
                scan_config(&mut agent, &root.join("settings.json"), "사용자");
                scan_config(&mut agent, &home.join(".claude.json"), "사용자");
                scan_skills(&mut agent, &root.join("skills"), "사용자");
                scan_rules(&mut agent, &root.join("rules"), "사용자");
                add_file(&mut agent, &root.join("CLAUDE.md"), "rule", "사용자");
            }
            "antigravity" => {
                scan_config(&mut agent, &root.join("config/hooks.json"), "사용자");
                scan_config(&mut agent, &root.join("config/mcp_config.json"), "사용자");
                scan_skills(&mut agent, &root.join("antigravity/skills"), "사용자");
                scan_skills(&mut agent, &root.join("skills"), "사용자");
                scan_rules(&mut agent, &root.join("antigravity/rules"), "사용자");
                add_file(&mut agent, &root.join("GEMINI.md"), "rule", "사용자");
            }
            "opencode" => {
                for file in ["opencode.json", "opencode.jsonc"] {
                    scan_config(&mut agent, &root.join(file), "사용자");
                }
                scan_scripts(&mut agent, &root.join("tools"), "tool", "사용자");
                scan_scripts(&mut agent, &root.join("plugins"), "plugin", "사용자");
                scan_skills(&mut agent, &root.join("skills"), "사용자");
                if let Some(path) = std::env::var_os("OPENCODE_CONFIG") {
                    scan_config(&mut agent, Path::new(&path), "환경 변수 지정");
                }
            }
            _ => {}
        }
        if let Some(p) = &project {
            let sub = match id {
                "codex" => ".codex",
                "claude" => ".claude",
                "antigravity" => ".agents",
                _ => ".opencode",
            };
            let pr = p.join(sub);
            agent.config_roots.push(display(&pr));
            scan_skills(&mut agent, &pr.join("skills"), "프로젝트");
            scan_rules(&mut agent, &pr.join("rules"), "프로젝트");
            match id {
                "codex" => {
                    scan_config(&mut agent, &pr.join("config.toml"), "프로젝트");
                    add_file(&mut agent, &p.join("AGENTS.md"), "rule", "프로젝트");
                    scan_skills(&mut agent, &p.join(".agents/skills"), "프로젝트 공통");
                }
                "claude" => {
                    for f in ["settings.json", "settings.local.json"] {
                        scan_config(&mut agent, &pr.join(f), "프로젝트");
                    }
                    scan_config(&mut agent, &p.join(".mcp.json"), "프로젝트");
                    add_file(&mut agent, &p.join("CLAUDE.md"), "rule", "프로젝트");
                }
                "antigravity" => {
                    scan_config(&mut agent, &pr.join("hooks.json"), "프로젝트");
                    scan_config(&mut agent, &pr.join("mcp.json"), "프로젝트");
                }
                "opencode" => {
                    for f in ["opencode.json", "opencode.jsonc"] {
                        scan_config(&mut agent, &p.join(f), "프로젝트");
                    }
                    scan_scripts(&mut agent, &pr.join("tools"), "tool", "프로젝트");
                    scan_scripts(&mut agent, &pr.join("plugins"), "plugin", "프로젝트");
                    add_file(&mut agent, &p.join("AGENTS.md"), "rule", "프로젝트");
                }
                _ => {}
            }
        }
        let mut seen = HashSet::new();
        agent.resources.retain(|r| seen.insert(r.id.clone()));
        agent
            .resources
            .sort_by(|a, b| (&a.kind, &a.name, &a.path).cmp(&(&b.kind, &b.name, &b.path)));
        agents.push(agent);
    }
    Ok(Snapshot {
        scanned_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        platform: std::env::consts::OS.into(),
        home: display(home),
        project_path: project.map(|p| display(&p)),
        agents,
    })
}

fn display(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn read_text(agent: &mut Agent, path: &Path) -> Option<String> {
    let meta = match fs::metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
        Err(_) => {
            agent
                .warnings
                .push(format!("접근할 수 없음: {}", display(path)));
            return None;
        }
    };
    if !meta.is_file() {
        return None;
    }
    if meta.len() > MAX_BYTES {
        agent
            .warnings
            .push(format!("탐지 크기 제한 초과: {}", display(path)));
        return None;
    }
    match fs::read_to_string(path) {
        Ok(t) => Some(t),
        Err(_) => {
            agent
                .warnings
                .push(format!("파일을 읽을 수 없음: {}", display(path)));
            None
        }
    }
}

fn entries(agent: &mut Agent, path: &Path) -> Vec<PathBuf> {
    let iter = match fs::read_dir(path) {
        Ok(i) => i,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return vec![],
        Err(_) => {
            agent
                .warnings
                .push(format!("폴더를 읽을 수 없음: {}", display(path)));
            return vec![];
        }
    };
    let mut out = Vec::new();
    for entry in iter.take(MAX_ENTRIES + 1) {
        if out.len() == MAX_ENTRIES {
            agent
                .warnings
                .push(format!("폴더 탐지 제한 초과: {}", display(path)));
            break;
        }
        match entry {
            Ok(e) => out.push(e.path()),
            Err(_) => agent
                .warnings
                .push(format!("일부 항목 접근 실패: {}", display(path))),
        }
    }
    out.sort();
    out
}

fn push(
    agent: &mut Agent,
    path: &Path,
    kind: &str,
    name: &str,
    scope: &str,
    status: &str,
    details: Vec<Detail>,
) {
    agent.resources.push(Resource {
        id: format!("{}:{}:{}:{}", agent.id, kind, display(path), name),
        agent_id: agent.id.clone(),
        kind: kind.into(),
        name: name.into(),
        path: display(path),
        scope: scope.into(),
        status: status.into(),
        source: "파일 기준 · 로컬".into(),
        origin: "user".into(),
        details,
    });
}
fn detail(label: &str, value: impl Into<String>) -> Detail {
    Detail {
        label: label.into(),
        value: value.into(),
    }
}
fn add_file(agent: &mut Agent, path: &Path, kind: &str, scope: &str) {
    if read_text(agent, path).is_some() {
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        push(agent, path, kind, &name, scope, "present", vec![]);
    }
}
fn scan_skills(agent: &mut Agent, root: &Path, scope: &str) {
    for dir in entries(agent, root) {
        let path = dir.join("SKILL.md");
        if read_text(agent, &path).is_some() {
            let name = dir.file_name().unwrap_or_default().to_string_lossy();
            push(
                agent,
                &path,
                "skill",
                &name,
                scope,
                "present",
                vec![detail("파일 묶음", display(&dir))],
            );
        }
    }
}
fn scan_rules(agent: &mut Agent, root: &Path, scope: &str) {
    for path in entries(agent, root) {
        if ["md", "rules", "mdc"].contains(
            &path
                .extension()
                .unwrap_or_default()
                .to_string_lossy()
                .as_ref(),
        ) {
            add_file(agent, &path, "rule", scope);
        }
    }
}
fn scan_scripts(agent: &mut Agent, root: &Path, kind: &str, scope: &str) {
    for path in entries(agent, root) {
        if !["ts", "js", "mjs", "cjs", "py", "sh", "ps1"].contains(
            &path
                .extension()
                .unwrap_or_default()
                .to_string_lossy()
                .as_ref(),
        ) {
            continue;
        }
        if let Some(text) = read_text(agent, &path) {
            let mut details = vec![detail("탐지 방식", "정적 파일 탐지 · 코드 실행 안 함")];
            if text.contains("session.idle") {
                details.push(detail(
                    "이벤트 근거",
                    "session.idle 문자열 발견 · 실제 로딩은 미확인",
                ));
            }
            if text.contains("run.py") {
                details.push(detail("실행 연동 근거", "run.py 참조 발견"));
            }
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            push(agent, &path, kind, &name, scope, "present", details);
        }
    }
}
fn status(config: &Value) -> &'static str {
    match config.get("enabled").and_then(Value::as_bool) {
        Some(false) => "disabled",
        Some(true) => "enabled",
        _ => "configured",
    }
}

fn scan_config(agent: &mut Agent, path: &Path, scope: &str) {
    let Some(text) = read_text(agent, path) else {
        return;
    };
    let parsed: Result<Value, ()> = if path.extension().is_some_and(|x| x == "toml") {
        toml::from_str::<toml::Value>(&text)
            .ok()
            .and_then(|v| serde_json::to_value(v).ok())
            .ok_or(())
    } else {
        json5::from_str(&text).map_err(|_| ())
    };
    let value = match parsed {
        Ok(v) if v.is_object() => v,
        _ => {
            agent
                .warnings
                .push(format!("설정 형식을 해석할 수 없음: {}", display(path)));
            return;
        }
    };
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    push(
        agent,
        path,
        "setting",
        &name,
        scope,
        "present",
        vec![detail(
            "설정 내용",
            "비밀 값 보호를 위해 원문을 표시하지 않음",
        )],
    );
    for key in ["mcp_servers", "mcpServers", "mcp"] {
        if let Some(map) = value.get(key).and_then(Value::as_object) {
            for (name, config) in map {
                let mut details = vec![
                    detail("설정 키", key),
                    detail("연결 상태", "아직 직접 조회하지 않음"),
                ];
                let mode = if config.get("url").is_some() {
                    "원격 URL"
                } else {
                    "로컬 명령"
                };
                details.push(detail("연결 방식", mode));
                for env_key in ["env", "environment"] {
                    if let Some(env) = config.get(env_key).and_then(Value::as_object) {
                        details.push(detail(
                            "환경 변수 이름",
                            env.keys().cloned().collect::<Vec<_>>().join(", "),
                        ));
                    }
                }
                push(agent, path, "mcp", name, scope, status(config), details);
            }
        }
    }
    if let Some(plugins) = value.get("plugins").and_then(Value::as_object) {
        for (name, config) in plugins {
            push(agent, path, "plugin", name, scope, status(config), vec![]);
            if agent.id == "codex"
                && name.rsplit_once('@').is_some_and(|(_, marketplace)| {
                    ["openai-bundled", "openai-primary-runtime"].contains(&marketplace)
                })
            {
                let resource = agent.resources.last_mut().expect("just inserted resource");
                resource.origin = if status(config) == "disabled" {
                    "user"
                } else {
                    "bundled"
                }
                .into();
                resource
                    .details
                    .push(detail("제공 근거", "Codex 기본 제공 마켓플레이스 등록"));
                if resource.origin == "user" {
                    resource
                        .details
                        .push(detail("사용자 설정", "기본 제공 플러그인의 비활성 설정"));
                }
            }
        }
    }
    if let Some(plugins) = value.get("enabledPlugins").and_then(Value::as_object) {
        for (name, enabled) in plugins {
            push(
                agent,
                path,
                "plugin",
                name,
                scope,
                if enabled.as_bool() == Some(false) {
                    "disabled"
                } else {
                    "configured"
                },
                vec![],
            );
        }
    }
    if let Some(plugins) = value.get("plugin").and_then(Value::as_array) {
        for (index, _) in plugins.iter().enumerate() {
            push(
                agent,
                path,
                "plugin",
                &format!("등록 플러그인 {}", index + 1),
                scope,
                "configured",
                vec![detail(
                    "참조",
                    "원본 파일에 등록됨 · 경로/URL 값은 표시하지 않음",
                )],
            );
        }
    }
    if value.get("notify").is_some() {
        push(
            agent,
            path,
            "hook",
            "notify",
            scope,
            "configured",
            vec![
                detail("이벤트", "에이전트 알림 명령"),
                detail("실행 상태", "명령 등록 확인 · 실행 미검증"),
            ],
        );
    }
    if let Some(hooks) = value.get("hooks").and_then(Value::as_object) {
        for (event, handlers) in hooks {
            if handlers.is_array() {
                push(
                    agent,
                    path,
                    "hook",
                    event,
                    scope,
                    "configured",
                    vec![
                        detail("이벤트", event),
                        detail("입력 형식", "에이전트별 검증 필요"),
                    ],
                );
            }
        }
    }
    if path.file_name().is_some_and(|x| x == "hooks.json") {
        if let Some(map) = value.as_object() {
            for (name, config) in map {
                if let Some(events) = config.as_object() {
                    let names = events
                        .keys()
                        .filter(|k| *k != "enabled")
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(", ");
                    let mut details = vec![
                        detail("이벤트", names),
                        detail("실행", "명령 등록 확인 · 실행 미검증"),
                    ];
                    if let Some(timeout) = config
                        .get("Stop")
                        .and_then(Value::as_array)
                        .and_then(|a| a.first())
                        .and_then(|x| x.get("timeout"))
                        .and_then(Value::as_u64)
                    {
                        details.push(detail("제한 시간", format!("{timeout}초")));
                    }
                    push(agent, path, "hook", name, scope, status(config), details);
                }
            }
        }
    }
}

fn find_executable(id: &str, home: &Path) -> Option<String> {
    let names: &[&str] = match id {
        "codex" => &["codex"],
        "claude" => &["claude"],
        "opencode" => &["opencode"],
        _ => &["agy", "antigravity"],
    };
    let mut paths: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    paths.extend([
        home.join(".local/bin"),
        home.join(".cargo/bin"),
        home.join(".opencode/bin"),
        home.join(".antigravity/bin"),
    ]);
    if cfg!(target_os = "windows") {
        paths.push(home.join("AppData/Roaming/npm"));
        paths.push(home.join("AppData/Local/agy/bin"));
        let root = home.join("AppData/Local/OpenAI/Codex/bin");
        if let Ok(entries) = fs::read_dir(root) {
            let mut versions = entries.filter_map(Result::ok).collect::<Vec<_>>();
            versions.sort_by_key(|e| e.metadata().and_then(|m| m.modified()).ok());
            paths.extend(versions.into_iter().rev().map(|e| e.path()));
        }
    } else {
        paths.extend([
            PathBuf::from("/usr/local/bin"),
            PathBuf::from("/opt/homebrew/bin"),
            PathBuf::from("/usr/bin"),
        ]);
    }
    for name in names {
        for root in &paths {
            let suffixes: &[&str] = if cfg!(target_os = "windows") {
                &[".exe", ".cmd", ".ps1", ".bat", ""]
            } else {
                &[""]
            };
            for suffix in suffixes {
                let path = root.join(format!("{name}{suffix}"));
                if path.is_file() {
                    return Some(display(&path));
                }
            }
        }
    }
    None
}
