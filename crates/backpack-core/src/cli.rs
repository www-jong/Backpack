use crate::{
    codex::{cancelled, ProcessOwnership, QueryState},
    display, scan, ScanRequest,
};
use serde::Serialize;
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::{
    path::{Path, PathBuf},
    process::Stdio,
    sync::{atomic::AtomicBool, Arc},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::{
    io::{AsyncRead, AsyncReadExt},
    process::Command,
};
const MAX_OUTPUT: u64 = 1024 * 1024;
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CliInspection {
    pub agent_id: String,
    pub observed_at: u64,
    pub version: Option<String>,
    pub executable: String,
    pub cwd: String,
    pub config_root: String,
    pub query: QueryState,
    pub servers: Vec<CliServer>,
}
#[derive(Serialize)]
pub struct CliServer {
    pub name: String,
    pub status: String,
    pub origin: String,
}
fn state(status: &str, message: &str) -> QueryState {
    QueryState {
        status: status.into(),
        message: message.into(),
    }
}
struct Launcher {
    program: PathBuf,
    entry: Option<PathBuf>,
}
fn launcher(executable: &Path, id: &str) -> Result<Launcher, String> {
    if !cfg!(windows)
        || executable
            .extension()
            .is_some_and(|s| s.eq_ignore_ascii_case("exe"))
    {
        return Ok(Launcher {
            program: executable.into(),
            entry: None,
        });
    }
    // Resolve only known npm package entry points. Never interpret a shell wrapper.
    let parent = executable
        .parent()
        .ok_or("실행 파일 경로가 올바르지 않습니다.")?;
    let entry = match id {
        "opencode" => parent.join("node_modules/opencode-ai/bin/opencode.exe"),
        "claude" => parent.join("node_modules/@anthropic-ai/claude-code/cli.js"),
        "gemini" => parent.join("node_modules/@google/gemini-cli/dist/index.js"),
        _ => return Err("지원하지 않는 직접 조회입니다.".into()),
    };
    if !entry.is_file() {
        return Err(
            "이 설치 형태의 조회 실행 파일을 확인하지 못했습니다. 셸 래퍼는 실행하지 않습니다."
                .into(),
        );
    }
    if id == "opencode" {
        return Ok(Launcher {
            program: entry,
            entry: None,
        });
    }
    let node = std::iter::once(parent.to_path_buf())
        .chain(
            std::env::var_os("PATH")
                .map(|p| std::env::split_paths(&p).collect::<Vec<_>>())
                .unwrap_or_default(),
        )
        .map(|p| p.join("node.exe"))
        .find(|p| p.is_file())
        .ok_or("npm 설치의 Node.js 실행 파일을 찾지 못했습니다.")?;
    Ok(Launcher {
        program: node,
        entry: Some(entry),
    })
}
#[derive(Debug)]
enum RunError {
    Failed,
    Timeout,
    Cancelled,
    Limit,
}
impl RunError {
    fn state(&self) -> QueryState {
        match self {
            Self::Failed => state(
                "error",
                "CLI 실행에 실패했습니다. 비밀 값 보호를 위해 원문은 표시하지 않습니다.",
            ),
            Self::Timeout => state("error", "조회 제한 시간을 초과했습니다."),
            Self::Cancelled => state("error", "조회를 취소했습니다."),
            Self::Limit => state("error", "조회 결과 크기 제한을 초과했습니다."),
        }
    }
}
async fn read_bounded(stream: impl AsyncRead + Unpin) -> Result<Vec<u8>, RunError> {
    let mut bytes = Vec::new();
    stream
        .take(MAX_OUTPUT + 1)
        .read_to_end(&mut bytes)
        .await
        .map_err(|_| RunError::Failed)?;
    if bytes.len() as u64 > MAX_OUTPUT {
        return Err(RunError::Limit);
    }
    Ok(bytes)
}
async fn run(
    mut command: Command,
    cancel: Arc<AtomicBool>,
    duration: Duration,
) -> Result<String, RunError> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(0x08000000);
    #[cfg(unix)]
    command.as_std_mut().process_group(0);
    let mut child = command.spawn().map_err(|_| RunError::Failed)?;
    let mut owned = ProcessOwnership::attach(&child).map_err(|_| RunError::Failed)?;
    let stdout = child.stdout.take().ok_or(RunError::Failed)?;
    let stderr = child.stderr.take().ok_or(RunError::Failed)?;
    let result = tokio::select! {
        result=tokio::time::timeout(duration, async { tokio::try_join!(read_bounded(stdout), read_bounded(stderr), async {child.wait().await.map_err(|_|RunError::Failed)}) }) => result.map_err(|_|RunError::Timeout).and_then(|r|r),
        _=cancelled(cancel)=>Err(RunError::Cancelled),
    };
    owned.terminate();
    if result.is_err() {
        let _ = child.kill().await;
    }
    match result {
        Ok((out, err, exit)) if exit.success() => Ok(format!(
            "{}\n{}",
            String::from_utf8_lossy(&out),
            String::from_utf8_lossy(&err)
        )),
        Ok(_) => Err(RunError::Failed),
        Err(e) => Err(e),
    }
}
fn strip_ansi(text: &str) -> String {
    let mut out = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            match chars.next() {
                Some('[') => {
                    for c in chars.by_ref() {
                        if ('@'..='~').contains(&c) {
                            break;
                        }
                    }
                }
                Some(']') => {
                    while let Some(c) = chars.next() {
                        if c == '\u{7}' {
                            break;
                        }
                        if c == '\u{1b}' && chars.peek() == Some(&'\\') {
                            chars.next();
                            break;
                        }
                    }
                }
                _ => {}
            }
        } else if !c.is_control() || c == '\n' || c == '\t' {
            out.push(c);
        }
    }
    out
}
fn version(text: &str) -> Option<String> {
    strip_ansi(text)
        .split_whitespace()
        .map(|s| s.trim_start_matches('v'))
        .find(|s| {
            let parts = s.split('.').collect::<Vec<_>>();
            parts.len() == 3
                && parts
                    .iter()
                    .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
        })
        .map(str::to_owned)
}
fn safe_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || " -_.".contains(c))
}
fn reported_status(text: &str) -> Option<&'static str> {
    let s = text
        .trim_start_matches(|c: char| !c.is_ascii_alphabetic())
        .to_ascii_lowercase();
    if s.starts_with("disconnected") || s.starts_with("failed") {
        Some("failed")
    } else if s.starts_with("connected") {
        Some("connected")
    } else if s.starts_with("needs authentication") {
        Some("authenticationRequired")
    } else if s.starts_with("needs client registration") {
        Some("clientRegistrationRequired")
    } else if s.starts_with("disabled") || s.starts_with("rejected") {
        Some("disabled")
    } else if s.starts_with("pending approval") {
        Some("pendingApproval")
    } else if s.starts_with("not initialized") {
        Some("notStarted")
    } else if s.starts_with("not configured") {
        Some("notConfigured")
    } else {
        None
    }
}
fn parse_servers(id: &str, text: &str) -> Result<(Vec<CliServer>, bool), String> {
    let text = strip_ansi(text);
    let mut servers = Vec::new();
    let mut unparsed = false;
    let empty = text
        .to_ascii_lowercase()
        .contains("no mcp servers configured")
        || text
            .to_ascii_lowercase()
            .contains("no configured mcp servers");
    for line in text.lines() {
        let line = line
            .trim()
            .trim_start_matches(|c: char| c.is_whitespace() || "│┃┌└●◇".contains(c))
            .trim();
        let row = if id == "opencode" {
            let line = line.trim_start_matches(['✓', '○', '⚠', '✗', '✘']).trim();
            line.split_once(char::is_whitespace)
                .and_then(|(name, status)| reported_status(status).map(|s| (name, s)))
        } else {
            line.split_once(": ").and_then(|(name, rest)| {
                rest.rsplit_once(" - ").and_then(|(_, status)| {
                    reported_status(status)
                        .map(|s| (name.trim_start_matches(['✓', '✗', '✘']).trim(), s))
                })
            })
        };
        if row.is_none()
            && (line.contains(": ") && line.contains(" - ")
                || id == "opencode" && line.starts_with(['✓', '○', '⚠', '✗', '✘']))
        {
            unparsed = true;
        }
        if let Some((name, status)) = row {
            if !safe_name(name) {
                unparsed = true;
                continue;
            }
            if !servers.iter().any(|s: &CliServer| s.name == name) {
                servers.push(CliServer {
                    name: name.into(),
                    status: status.into(),
                    origin: "unknown".into(),
                });
            }
        }
    }
    if servers.len() > 2000 {
        return Err("조회 서버 개수 제한을 초과했습니다.".into());
    }
    if servers.is_empty() && !empty {
        return Err("CLI 응답 형식을 해석하지 못했습니다. 서버가 없다고 판단하지 않습니다.".into());
    }
    Ok((servers, unparsed))
}
fn command(launcher: &Launcher, id: &str, root: &Path, cwd: &Path, args: &[&str]) -> Command {
    let mut command = Command::new(&launcher.program);
    if let Some(entry) = &launcher.entry {
        command.arg(entry);
    }
    command
        .args(args)
        .current_dir(cwd)
        .env("NO_COLOR", "1")
        .env("CI", "1");
    match id {
        "claude" => {
            command.env("CLAUDE_CONFIG_DIR", root);
        }
        "opencode" => {
            command.env("OPENCODE_CONFIG_DIR", root);
        }
        _ => {}
    }
    command
}
pub async fn inspect_cli(
    request: ScanRequest,
    id: String,
    cancel: Arc<AtomicBool>,
) -> Result<CliInspection, String> {
    if !["claude", "opencode", "gemini"].contains(&id.as_str()) {
        return Err("이 에이전트의 CLI 직접 조회는 지원하지 않습니다.".into());
    }
    if id == "gemini" && request.roots.contains_key(&id) {
        return Err("Gemini CLI 직접 조회는 기본 설정 경로에서만 지원합니다. 사용자 지정 루트는 파일 탐지로 확인하세요.".into());
    }
    let snapshot = scan(request)?;
    let agent = snapshot
        .agents
        .iter()
        .find(|a| a.id == id)
        .ok_or("에이전트를 찾지 못했습니다.")?;
    let exe = agent
        .executable
        .as_ref()
        .ok_or("설치된 CLI 실행 파일을 찾지 못했습니다.")?;
    let launch = launcher(Path::new(exe), &id)?;
    let root = Path::new(&agent.config_roots[0]);
    let cwd = Path::new(snapshot.project_path.as_deref().unwrap_or(&snapshot.home));
    let deadline = Instant::now() + Duration::from_secs(45);
    let mut result = CliInspection {
        agent_id: id.clone(),
        observed_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        version: None,
        executable: display(&launch.program),
        cwd: display(cwd),
        config_root: display(root),
        query: state("skipped", ""),
        servers: vec![],
    };
    match run(
        command(&launch, &id, root, cwd, &["--version"]),
        cancel.clone(),
        Duration::from_secs(8),
    )
    .await
    {
        Ok(text) => result.version = version(&text),
        Err(RunError::Cancelled) => return Err("조회를 취소했습니다.".into()),
        _ => {}
    }
    let output = run(
        command(&launch, &id, root, cwd, &["mcp", "list"]),
        cancel,
        deadline.saturating_duration_since(Instant::now()),
    )
    .await;
    match output {
        Ok(text) => match parse_servers(&id, &text) {
            Ok((mut servers, partial)) => {
                for server in &mut servers {
                    if let Some(file) = agent
                        .resources
                        .iter()
                        .find(|r| r.kind == "mcp" && r.name == server.name)
                    {
                        server.origin = file.origin.clone();
                    }
                }
                result.servers = servers;
                result.query=state(if partial {"partial"} else {"success"},"CLI가 보고한 MCP 상태입니다. 현재 대화 세션과 별개이며 도구 목록은 조회하지 않습니다.");
            }
            Err(message) => result.query = state("unsupported", &message),
        },
        Err(e) => result.query = e.state(),
    }
    Ok(result)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cli_parsers_return_only_server_names_and_statuses() {
        for (id,text) in [
            ("claude","safe: https://SECRET.example - ✔ Connected\nauth: cmd SECRET - ! Needs authentication\npending: hidden - ⏸ Pending approval (run claude)"),
            ("gemini","✓ safe: command: SECRET (stdio) - Connected\n✗ bad: https://SECRET (http) - Disconnected"),
            ("opencode","│ ● ✓ safe connected\n│     https://SECRET\n│ ● ○ off disabled\n│ ● ✗ bad failed\n│     SECRET error"),
        ] {let (servers,partial)=parse_servers(id,text).unwrap(); assert!(!partial); assert_eq!(servers[0].status,"connected"); assert!(!serde_json::to_string(&servers).unwrap().contains("SECRET"));}
    }
    #[test]
    fn unknown_and_empty_outputs_are_distinct() {
        assert!(parse_servers("claude", "unexpected SECRET").is_err());
        assert!(parse_servers("opencode", "No MCP servers configured")
            .unwrap()
            .0
            .is_empty());
        assert_eq!(
            version("\u{1b}[32m1.2.3\u{1b}[0m SECRET"),
            Some("1.2.3".into())
        );
        assert_eq!(reported_status("Disconnected"), Some("failed"));
        let (_, partial) = parse_servers(
            "claude",
            "safe: hidden - Connected\nnew: SECRET - Novel status",
        )
        .unwrap();
        assert!(partial);
    }
    #[tokio::test]
    async fn cli_process_limits_cancellation_and_failures_are_bounded() {
        let temp = tempfile::TempDir::new().unwrap();
        let exe = temp.path().join(if cfg!(windows) {
            "fixture.exe"
        } else {
            "fixture"
        });
        assert!(std::process::Command::new("rustc")
            .arg("--edition=2021")
            .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cli_server.rs"))
            .arg("-o")
            .arg(&exe)
            .status()
            .unwrap()
            .success());
        let execute = |mode: &str| {
            let mut command = Command::new(&exe);
            command.arg(mode);
            command
        };
        let ok = run(
            execute("normal"),
            Arc::new(AtomicBool::new(false)),
            Duration::from_secs(3),
        )
        .await
        .unwrap();
        let (servers, _) = parse_servers("claude", &ok).unwrap();
        assert!(!serde_json::to_string(&servers).unwrap().contains("SECRET"));
        assert!(matches!(
            run(
                execute("fail"),
                Arc::new(AtomicBool::new(false)),
                Duration::from_secs(3)
            )
            .await,
            Err(RunError::Failed)
        ));
        assert!(matches!(
            run(
                execute("flood"),
                Arc::new(AtomicBool::new(false)),
                Duration::from_secs(3)
            )
            .await,
            Err(RunError::Limit)
        ));
        assert!(matches!(
            run(
                execute("hang"),
                Arc::new(AtomicBool::new(false)),
                Duration::from_millis(50)
            )
            .await,
            Err(RunError::Timeout)
        ));
        assert!(matches!(
            run(
                execute("hang"),
                Arc::new(AtomicBool::new(true)),
                Duration::from_secs(3)
            )
            .await,
            Err(RunError::Cancelled)
        ));
    }
    #[tokio::test]
    #[ignore = "Manual integration: executes installed OpenCode and MCP health checks"]
    async fn installed_opencode_query_returns_safe_failure_or_report() {
        let result = inspect_cli(
            ScanRequest::default(),
            "opencode".into(),
            Arc::new(AtomicBool::new(false)),
        )
        .await
        .unwrap();
        println!(
            "OpenCode version={:?}, query={}, servers={}",
            result.version,
            result.query.status,
            result.servers.len()
        );
        assert!(
            result.query.status == "success"
                || result.query.status == "error"
                || result.query.status == "unsupported"
        );
    }
}
