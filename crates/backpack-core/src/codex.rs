use crate::{detail, display, scan, Detail, Resource, ScanRequest};
use serde::Serialize;
use serde_json::{json, Value};
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    process::Stdio,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, ChildStdout, Command},
};

const MAX_MESSAGE: usize = 2 * 1024 * 1024;
const MAX_TOTAL: usize = 16 * 1024 * 1024;
const MAX_ITEMS: usize = 2000;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryState {
    pub status: String,
    pub message: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexInspection {
    pub observed_at: u64,
    pub version: Option<String>,
    pub executable: String,
    pub codex_home: String,
    pub cwd: String,
    pub context: String,
    pub config_query: QueryState,
    pub skills_query: QueryState,
    pub mcp_query: QueryState,
    pub settings: Vec<Detail>,
    pub skills: Vec<Resource>,
    pub mcp_servers: Vec<McpStatus>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpStatus {
    pub name: String,
    pub origin: String,
    pub auth_status: String,
    pub runtime_status: Option<String>,
    pub tool_names: Vec<String>,
    pub tools_error: bool,
    pub plugin_id: Option<String>,
}
fn state(status: &str, message: &str) -> QueryState {
    QueryState {
        status: status.into(),
        message: message.into(),
    }
}
#[derive(Debug)]
enum RpcError {
    Unsupported,
    Failed,
    Timeout,
    Cancelled,
    OutputLimit,
    Protocol,
}
impl RpcError {
    fn state(&self) -> QueryState {
        match self {
            Self::Unsupported => state(
                "unsupported",
                "현재 Codex 버전에서 지원하지 않는 조회입니다.",
            ),
            Self::Failed => state(
                "error",
                "Codex가 조회 오류를 반환했습니다. 원문은 표시하지 않습니다.",
            ),
            Self::Timeout => state("error", "조회 제한 시간을 초과했습니다."),
            Self::Cancelled => state("error", "조회를 취소했습니다."),
            Self::OutputLimit => state("error", "조회 결과 크기 제한을 초과했습니다."),
            Self::Protocol => state("error", "Codex 응답 형식을 해석하지 못했습니다."),
        }
    }
    fn fatal(&self) -> bool {
        matches!(
            self,
            Self::Timeout | Self::Cancelled | Self::OutputLimit | Self::Protocol
        )
    }
}

struct Session {
    child: Child,
    input: Option<ChildStdin>,
    output: BufReader<ChildStdout>,
    ownership: ProcessOwnership,
    next_id: u64,
    deadline: Instant,
    bytes: usize,
    cancel: Arc<AtomicBool>,
}
impl Session {
    fn start(
        executable: &Path,
        root: &Path,
        cwd: &Path,
        cancel: Arc<AtomicBool>,
    ) -> Result<Self, String> {
        let mut command = Command::new(executable);
        command
            .args(["app-server", "--stdio"])
            .current_dir(cwd)
            .env("CODEX_HOME", root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        #[cfg(windows)]
        command.creation_flags(0x08000000);
        #[cfg(unix)]
        command.as_std_mut().process_group(0);
        Self::from_command(command, cancel)
    }
    fn from_command(mut command: Command, cancel: Arc<AtomicBool>) -> Result<Self, String> {
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        #[cfg(windows)]
        command.creation_flags(0x08000000);
        #[cfg(unix)]
        command.as_std_mut().process_group(0);
        let mut child = command
            .spawn()
            .map_err(|_| "Codex 조회 프로세스를 시작하지 못했습니다.")?;
        let ownership = ProcessOwnership::attach(&child)?;
        let input = child
            .stdin
            .take()
            .ok_or("Codex 입력 채널을 열지 못했습니다.")?;
        let output = child
            .stdout
            .take()
            .ok_or("Codex 출력 채널을 열지 못했습니다.")?;
        Ok(Self {
            child,
            input: Some(input),
            output: BufReader::new(output),
            ownership,
            next_id: 1,
            deadline: Instant::now() + Duration::from_secs(45),
            bytes: 0,
            cancel,
        })
    }
    async fn send(&mut self, message: Value) -> Result<(), RpcError> {
        let mut data = serde_json::to_vec(&message).map_err(|_| RpcError::Protocol)?;
        data.push(b'\n');
        let input = self.input.as_mut().ok_or(RpcError::Failed)?;
        input.write_all(&data).await.map_err(|_| RpcError::Failed)?;
        input.flush().await.map_err(|_| RpcError::Failed)
    }
    async fn request(
        &mut self,
        method: &str,
        params: Value,
        seconds: u64,
    ) -> Result<Value, RpcError> {
        let remaining = self
            .deadline
            .saturating_duration_since(Instant::now())
            .min(Duration::from_secs(seconds));
        if remaining.is_zero() {
            return Err(RpcError::Timeout);
        }
        let cancel = self.cancel.clone();
        tokio::select! {
            result = tokio::time::timeout(remaining, self.request_inner(method, params)) => result.map_err(|_| RpcError::Timeout)?,
            _ = cancelled(cancel) => Err(RpcError::Cancelled),
        }
    }
    async fn request_inner(&mut self, method: &str, params: Value) -> Result<Value, RpcError> {
        let id = self.next_id;
        self.next_id += 1;
        self.send(json!({"id":id,"method":method,"params":params}))
            .await?;
        loop {
            let mut line = Vec::new();
            (&mut self.output)
                .take((MAX_MESSAGE + 1) as u64)
                .read_until(b'\n', &mut line)
                .await
                .map_err(|_| RpcError::Failed)?;
            self.bytes += line.len();
            if line.len() > MAX_MESSAGE || self.bytes > MAX_TOTAL {
                return Err(RpcError::OutputLimit);
            }
            if line.is_empty() {
                return Err(RpcError::Failed);
            }
            let message: Value = serde_json::from_slice(&line).map_err(|_| RpcError::Protocol)?;
            if message.get("method").is_some() && message.get("id").is_some() {
                self.send(json!({"id":message["id"],"error":{"code":-32601,"message":"Backpack inspection declines interactive requests"}})).await?;
                continue;
            }
            if message.get("id").and_then(Value::as_u64) != Some(id) {
                continue;
            }
            if let Some(error) = message.get("error") {
                return Err(
                    if error
                        .get("code")
                        .and_then(Value::as_i64)
                        .is_some_and(|c| [-32601, -32602].contains(&c))
                    {
                        RpcError::Unsupported
                    } else {
                        RpcError::Failed
                    },
                );
            }
            return message.get("result").cloned().ok_or(RpcError::Protocol);
        }
    }
    async fn close(&mut self) {
        self.input.take();
        if tokio::time::timeout(Duration::from_secs(2), self.child.wait())
            .await
            .is_err()
        {
            let _ = self.child.kill().await;
        }
        self.ownership.terminate();
    }
}
async fn cancelled(cancel: Arc<AtomicBool>) {
    loop {
        if cancel.load(Ordering::Relaxed) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(80)).await;
    }
}

#[cfg(windows)]
struct ProcessOwnership(windows_sys::Win32::Foundation::HANDLE);
#[cfg(windows)]
unsafe impl Send for ProcessOwnership {}
#[cfg(windows)]
impl ProcessOwnership {
    fn attach(child: &Child) -> Result<Self, String> {
        use windows_sys::Win32::{Foundation::CloseHandle, System::JobObjects::*};
        // The job owns only this inspection process and its descendants.
        unsafe {
            let handle = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if handle.is_null() {
                return Err("Codex 프로세스 종료 관리자를 만들지 못했습니다.".into());
            }
            let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            let configured = SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                &limits as *const _ as *const _,
                std::mem::size_of_val(&limits) as u32,
            );
            let assigned = child
                .raw_handle()
                .is_some_and(|process| AssignProcessToJobObject(handle, process as _) != 0);
            if configured == 0 || !assigned {
                CloseHandle(handle);
                return Err("Codex 조회 프로세스의 안전한 종료를 준비하지 못했습니다.".into());
            }
            Ok(Self(handle))
        }
    }
    fn terminate(&mut self) {
        if !self.0.is_null() {
            unsafe {
                windows_sys::Win32::Foundation::CloseHandle(self.0);
            }
            self.0 = std::ptr::null_mut();
        }
    }
}
#[cfg(unix)]
struct ProcessOwnership(Option<i32>);
#[cfg(unix)]
impl ProcessOwnership {
    fn attach(child: &Child) -> Result<Self, String> {
        Ok(Self(Some(
            child.id().ok_or("조회 프로세스 ID를 얻지 못했습니다.")? as i32,
        )))
    }
    fn terminate(&mut self) {
        if let Some(group) = self.0.take() {
            unsafe {
                libc::kill(-group, libc::SIGKILL);
            }
        }
    }
}
impl Drop for ProcessOwnership {
    fn drop(&mut self) {
        self.terminate();
    }
}

fn identifier(value: &Value) -> Option<String> {
    value
        .as_str()
        .filter(|s| !s.is_empty() && s.len() <= 512 && !s.chars().any(char::is_control))
        .map(str::to_owned)
}
fn bundled_plugin(id: &str) -> bool {
    id.rsplit_once('@').is_some_and(|(_, marketplace)| {
        ["openai-bundled", "openai-primary-runtime"].contains(&marketplace)
    })
}
fn skill_resources(value: &Value, cwd: &Path) -> Result<(Vec<Resource>, usize), RpcError> {
    let groups = value
        .get("data")
        .and_then(Value::as_array)
        .ok_or(RpcError::Protocol)?;
    let mut resources = Vec::new();
    let mut errors = 0;
    let mut seen = HashSet::new();
    let mut matched = false;
    for group in groups {
        if !group
            .get("cwd")
            .and_then(Value::as_str)
            .is_some_and(|p| Path::new(p) == cwd)
        {
            continue;
        }
        matched = true;
        errors += group
            .get("errors")
            .and_then(Value::as_array)
            .map_or(0, Vec::len);
        for skill in group
            .get("skills")
            .and_then(Value::as_array)
            .ok_or(RpcError::Protocol)?
        {
            if resources.len() >= MAX_ITEMS {
                return Err(RpcError::OutputLimit);
            }
            let path = skill
                .get("path")
                .and_then(Value::as_str)
                .map(PathBuf::from)
                .filter(|p| p.is_absolute())
                .ok_or(RpcError::Protocol)?;
            let name = skill
                .get("name")
                .and_then(identifier)
                .ok_or(RpcError::Protocol)?;
            if !seen.insert(path.clone()) {
                continue;
            }
            let scope = skill
                .get("scope")
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            let plugin = skill.get("pluginId").and_then(identifier);
            let origin = if scope == "system" || plugin.as_deref().is_some_and(bundled_plugin) {
                "bundled"
            } else if ["user", "repo"].contains(&scope) {
                "user"
            } else {
                "unknown"
            };
            let status = match skill.get("enabled").and_then(Value::as_bool) {
                Some(true) => "enabled",
                Some(false) => "disabled",
                None => "configured",
            };
            let origin = if origin == "bundled" && status == "disabled" {
                "user"
            } else {
                origin
            };
            let mut details = vec![
                detail("Codex 보고 범위", scope),
                detail(
                    "조회 문맥",
                    "Backpack 별도 조회 프로세스 · 현재 대화 세션과 별개",
                ),
            ];
            if let Some(plugin) = plugin {
                details.push(detail("소속 플러그인", plugin));
            }
            resources.push(Resource {
                id: format!("codex:skill:{}:{name}", display(&path)),
                agent_id: "codex".into(),
                kind: "skill".into(),
                name,
                path: display(&path),
                scope: match scope {
                    "user" => "사용자",
                    "repo" => "프로젝트",
                    "system" => "기본 제공",
                    "admin" => "관리자",
                    _ => "미확인",
                }
                .into(),
                status: status.into(),
                source: "Codex 직접 조회 · skills/list".into(),
                origin: origin.into(),
                details,
            });
        }
    }
    if !matched {
        return Err(RpcError::Protocol);
    }
    Ok((resources, errors))
}
fn configuration(value: &Value) -> Result<Vec<Detail>, RpcError> {
    let config = value
        .get("config")
        .and_then(Value::as_object)
        .ok_or(RpcError::Protocol)?;
    let mut details = Vec::new();
    for (key, label) in [
        ("model", "설정 모델"),
        ("model_reasoning_effort", "추론 수준"),
        ("sandbox_mode", "샌드박스 모드"),
        ("approval_policy", "승인 정책"),
        ("web_search", "웹 검색 모드"),
    ] {
        if let Some(value) = config.get(key).and_then(identifier) {
            details.push(detail(label, value));
        }
    }
    Ok(details)
}
fn mcp_page(
    value: &Value,
    file_resources: &[Resource],
) -> Result<(Vec<McpStatus>, Option<String>), RpcError> {
    let data = value
        .get("data")
        .and_then(Value::as_array)
        .ok_or(RpcError::Protocol)?;
    let mut servers = Vec::new();
    for server in data {
        let name = server
            .get("name")
            .and_then(identifier)
            .ok_or(RpcError::Protocol)?;
        let plugin_id = server.get("pluginId").and_then(identifier);
        let configured = file_resources
            .iter()
            .find(|r| r.kind == "mcp" && r.name == name);
        let origin = if plugin_id.as_deref().is_some_and(bundled_plugin)
            || configured.is_some_and(|r| r.origin == "bundled")
            || (name == "codex_apps" && configured.is_none())
        {
            "bundled"
        } else if configured.is_some() || plugin_id.is_some() {
            "user"
        } else {
            "unknown"
        };
        let auth_status = server
            .get("authStatus")
            .and_then(Value::as_str)
            .filter(|s| {
                [
                    "unknown",
                    "unsupported",
                    "notLoggedIn",
                    "bearerToken",
                    "oAuth",
                ]
                .contains(s)
            })
            .unwrap_or("unknown")
            .to_owned();
        let runtime_status = server
            .get("runtimeStatus")
            .and_then(Value::as_str)
            .filter(|s| {
                [
                    "notStarted",
                    "starting",
                    "connected",
                    "authenticationRequired",
                    "failed",
                    "cancelled",
                    "disabled",
                ]
                .contains(s)
            })
            .map(str::to_owned);
        let tools = server
            .get("tools")
            .and_then(Value::as_object)
            .ok_or(RpcError::Protocol)?;
        if tools.len() > MAX_ITEMS {
            return Err(RpcError::OutputLimit);
        }
        let tool_names = tools
            .keys()
            .filter(|n| n.len() <= 512 && !n.chars().any(char::is_control))
            .cloned()
            .collect();
        servers.push(McpStatus {
            name,
            origin: origin.into(),
            plugin_id,
            auth_status,
            runtime_status,
            tool_names,
            tools_error: server.get("toolsError").is_some_and(|e| !e.is_null()),
        });
    }
    let cursor = value
        .get("nextCursor")
        .filter(|c| !c.is_null())
        .map(|c| identifier(c).ok_or(RpcError::Protocol))
        .transpose()?;
    Ok((servers, cursor))
}

pub async fn inspect_codex(
    request: ScanRequest,
    include_mcp: bool,
    cancel: Arc<AtomicBool>,
) -> Result<CodexInspection, String> {
    let snapshot = scan(request)?;
    let codex = snapshot
        .agents
        .iter()
        .find(|a| a.id == "codex")
        .ok_or("Codex 탐지 결과가 없습니다.")?;
    let executable = codex
        .executable
        .as_ref()
        .ok_or("Codex 실행 파일을 찾지 못했습니다. 설치와 PATH를 확인하세요.")?;
    let executable_path = Path::new(executable);
    #[cfg(windows)]
    if !executable_path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("exe"))
    {
        return Err(
            "직접 조회는 Codex exe 실행 파일을 지원합니다. 현재는 셸 래퍼 설치입니다.".into(),
        );
    }
    let root = Path::new(&codex.config_roots[0]);
    let cwd = Path::new(snapshot.project_path.as_deref().unwrap_or(&snapshot.home));
    let mut report = CodexInspection {
        observed_at: 0,
        version: None,
        executable: executable.clone(),
        codex_home: display(root),
        cwd: display(cwd),
        context: "Backpack 별도 조회 프로세스 · 기존 Codex 대화 세션과 별개".into(),
        config_query: state("skipped", "조회하지 않았습니다."),
        skills_query: state("skipped", "조회하지 않았습니다."),
        mcp_query: state("skipped", "MCP 상태 조회를 선택하지 않았습니다."),
        settings: vec![],
        skills: vec![],
        mcp_servers: vec![],
    };
    let mut session = Session::start(executable_path, root, cwd, cancel.clone())?;
    let result = inspect_session(&mut session, &mut report, &codex.resources, include_mcp).await;
    session.close().await;
    if cancel.load(Ordering::Relaxed) {
        return Err("Codex 직접 조회를 취소했습니다.".into());
    }
    result?;
    report.observed_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    Ok(report)
}
async fn inspect_session(
    session: &mut Session,
    report: &mut CodexInspection,
    files: &[Resource],
    include_mcp: bool,
) -> Result<(), String> {
    let initialize = session.request("initialize",json!({"clientInfo":{"name":"backpack","title":"Backpack","version":env!("CARGO_PKG_VERSION")}}),8).await.map_err(|e| e.state().message)?;
    if let Some(home) = initialize.get("codexHome").and_then(Value::as_str) {
        if Path::new(home).canonicalize().ok() != Path::new(&report.codex_home).canonicalize().ok()
        {
            return Err("조회 프로세스의 Codex 설정 경로가 요청한 경로와 다릅니다.".into());
        }
    }
    report.version = initialize
        .get("userAgent")
        .and_then(Value::as_str)
        .and_then(|s| {
            s.split(|c: char| !(c.is_ascii_digit() || c == '.'))
                .find(|part| {
                    part.split('.').count() == 3
                        && part
                            .split('.')
                            .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
                })
        })
        .map(str::to_owned);
    session
        .send(json!({"method":"initialized","params":{}}))
        .await
        .map_err(|e| e.state().message)?;
    match session
        .request(
            "config/read",
            json!({"cwd":report.cwd,"includeLayers":false}),
            8,
        )
        .await
        .and_then(|v| configuration(&v))
    {
        Ok(settings) => {
            report.settings = settings;
            report.config_query = state("success", "조회 문맥에서 적용되는 설정을 확인했습니다.");
        }
        Err(e) => {
            report.config_query = e.state();
            if e.fatal() {
                return Ok(());
            }
        }
    }
    match session
        .request(
            "skills/list",
            json!({"cwds":[report.cwd],"forceReload":true}),
            12,
        )
        .await
        .and_then(|v| skill_resources(&v, Path::new(&report.cwd)))
    {
        Ok((skills, errors)) => {
            report.skills = skills;
            report.skills_query = if errors == 0 {
                state("success", "Codex가 보고한 스킬 목록입니다.")
            } else {
                state(
                    "partial",
                    &format!("스킬 일부를 확인하지 못했습니다 · {errors}개 오류"),
                )
            };
        }
        Err(e) => {
            report.skills_query = e.state();
            if e.fatal() {
                return Ok(());
            }
        }
    }
    if include_mcp {
        let mut cursor: Option<String> = None;
        let mut cursors = HashSet::new();
        let mut names = HashSet::new();
        for _ in 0..20 {
            match session
                .request(
                    "mcpServerStatus/list",
                    json!({"cursor":cursor,"limit":100,"detail":"toolsAndAuthOnly"}),
                    15,
                )
                .await
                .and_then(|v| mcp_page(&v, files))
            {
                Ok((servers, next)) => {
                    for server in servers {
                        if names.insert(server.name.clone()) {
                            report.mcp_servers.push(server);
                        }
                    }
                    if report.mcp_servers.len() > MAX_ITEMS {
                        report.mcp_query = RpcError::OutputLimit.state();
                        return Ok(());
                    }
                    report.mcp_query=state("success","별도 조회 프로세스가 보고한 MCP 목록입니다. 현재 대화 세션의 연결 상태와 다를 수 있습니다.");
                    if next.is_none() {
                        return Ok(());
                    }
                    if !cursors.insert(next.clone()) {
                        report.mcp_query = RpcError::Protocol.state();
                        return Ok(());
                    }
                    cursor = next;
                }
                Err(e) => {
                    report.mcp_query = e.state();
                    return Ok(());
                }
            }
        }
        report.mcp_query = state("partial", "페이지 제한으로 MCP 목록 일부만 가져왔습니다.");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::OnceLock;
    use tempfile::TempDir;
    struct Fixture {
        _directory: TempDir,
        executable: PathBuf,
    }
    fn fixture() -> &'static Fixture {
        static FIXTURE: OnceLock<Fixture> = OnceLock::new();
        FIXTURE.get_or_init(|| {
            let directory = TempDir::new().unwrap();
            let executable = directory.path().join(if cfg!(windows) {
                "app-server-fixture.exe"
            } else {
                "app-server-fixture"
            });
            let status = std::process::Command::new("rustc")
                .arg("--edition=2021")
                .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/app_server.rs"))
                .arg("-o")
                .arg(&executable)
                .status()
                .unwrap();
            assert!(status.success());
            Fixture {
                _directory: directory,
                executable,
            }
        })
    }
    fn fixture_session(mode: &str, cwd: &Path, path: &Path, cancel: Arc<AtomicBool>) -> Session {
        let mut command = Command::new(&fixture().executable);
        command.args([
            mode,
            &serde_json::to_string(&display(cwd)).unwrap(),
            &serde_json::to_string(&display(path)).unwrap(),
        ]);
        Session::from_command(command, cancel).unwrap()
    }
    fn report(cwd: &Path) -> CodexInspection {
        CodexInspection {
            observed_at: 0,
            version: None,
            executable: "fixture".into(),
            codex_home: display(cwd),
            cwd: display(cwd),
            context: "test".into(),
            config_query: state("skipped", ""),
            skills_query: state("skipped", ""),
            mcp_query: state("skipped", ""),
            settings: vec![],
            skills: vec![],
            mcp_servers: vec![],
        }
    }
    #[tokio::test]
    async fn protocol_handshake_projects_safe_data_without_running_tools() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("SKILL.md");
        let mut session = fixture_session(
            "normal",
            dir.path(),
            &path,
            Arc::new(AtomicBool::new(false)),
        );
        let mut result = report(dir.path());
        inspect_session(&mut session, &mut result, &[], true)
            .await
            .unwrap();
        session.close().await;
        assert_eq!(result.version.as_deref(), Some("0.159.2"));
        assert_eq!(result.config_query.status, "success");
        assert_eq!(result.skills[0].status, "disabled");
        assert_eq!(result.skills[0].path, display(&path));
        assert_eq!(result.mcp_servers[0].tool_names, ["lookup"]);
        assert!(result.mcp_servers[0].runtime_status.is_none());
        assert!(!serde_json::to_string(&result).unwrap().contains("SECRET_"));
    }
    #[tokio::test]
    #[ignore = "Manual integration: starts installed Codex and configured MCP servers"]
    async fn installed_codex_read_only_inspection() {
        let result = inspect_codex(
            ScanRequest::default(),
            true,
            Arc::new(AtomicBool::new(false)),
        )
        .await
        .unwrap();
        println!(
            "Codex {:?}: config={}, skills={} ({}), MCP={} ({})",
            result.version,
            result.config_query.status,
            result.skills_query.status,
            result.skills.len(),
            result.mcp_query.status,
            result.mcp_servers.len()
        );
        assert_eq!(result.config_query.status, "success");
        assert_eq!(result.skills_query.status, "success");
        assert_eq!(result.mcp_query.status, "success");
    }
    #[test]
    fn skills_require_matching_context_and_preserve_disabled_overrides() {
        let dir = TempDir::new().unwrap();
        let cwd = dir.path();
        assert!(matches!(
            skill_resources(&json!({"data": []}), cwd),
            Err(RpcError::Protocol)
        ));
        let data = json!({"data": [{"cwd": display(cwd), "skills": [{"path": display(&cwd.join("SKILL.md")), "name": "bundled-override", "scope": "system", "enabled": false}], "errors": []}]});
        let (skills, _) = skill_resources(&data, cwd).unwrap();
        assert_eq!(skills[0].origin, "user");
        assert_eq!(skills[0].status, "disabled");
    }
    #[tokio::test]
    async fn unsupported_and_failed_responses_do_not_expose_error_messages() {
        let dir = TempDir::new().unwrap();
        for (mode, expected) in [("unsupported", "unsupported"), ("error", "error")] {
            let mut session = fixture_session(
                mode,
                dir.path(),
                dir.path(),
                Arc::new(AtomicBool::new(false)),
            );
            let result = session
                .request("config/read", json!({}), 3)
                .await
                .unwrap_err()
                .state();
            session.close().await;
            assert_eq!(result.status, expected);
            assert!(!result.message.contains("SECRET_ERROR"));
        }
    }
    #[tokio::test]
    async fn cancellation_and_deadline_release_the_owned_process() {
        let dir = TempDir::new().unwrap();
        let cancel = Arc::new(AtomicBool::new(false));
        let mut session = fixture_session("hang", dir.path(), dir.path(), cancel.clone());
        session.deadline = Instant::now() + Duration::from_millis(40);
        assert!(matches!(
            session.request("initialize", json!({}), 3).await,
            Err(RpcError::Timeout)
        ));
        session.close().await;
        assert!(session.child.try_wait().unwrap().is_some());
        let mut session = fixture_session("hang", dir.path(), dir.path(), cancel.clone());
        cancel.store(true, Ordering::Relaxed);
        assert!(matches!(
            session.request("initialize", json!({}), 3).await,
            Err(RpcError::Cancelled)
        ));
        session.close().await;
        assert!(session.child.try_wait().unwrap().is_some());
    }
    #[tokio::test]
    async fn oversized_response_is_bounded_before_json_parsing() {
        let dir = TempDir::new().unwrap();
        let mut session = fixture_session(
            "flood",
            dir.path(),
            dir.path(),
            Arc::new(AtomicBool::new(false)),
        );
        assert!(matches!(
            session.request("initialize", json!({}), 3).await,
            Err(RpcError::OutputLimit)
        ));
        assert!(session.bytes <= MAX_MESSAGE + 1);
        session.close().await;
    }
    #[test]
    fn mcp_null_status_and_bundled_plugin_are_not_confused_with_connection() {
        let (servers,cursor)=mcp_page(&json!({"data":[{"name":"runtime","pluginId":"example@openai-bundled","authStatus":"bearerToken","runtimeStatus":null,"tools":{"one":{"token":"SECRET"}},"toolsError":"SECRET_ERROR"}],"nextCursor":"next"}),&[]).unwrap();
        assert_eq!(cursor.as_deref(), Some("next"));
        assert_eq!(servers[0].origin, "bundled");
        assert!(servers[0].runtime_status.is_none());
        assert!(servers[0].tools_error);
        assert!(!serde_json::to_string(&servers).unwrap().contains("SECRET"));
    }
}
