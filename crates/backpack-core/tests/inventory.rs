use backpack_core::{scan_at, ScanRequest};
use std::{collections::BTreeMap, fs, path::Path};
use tempfile::TempDir;

fn write(home: &Path, file: &str, content: &str) {
    let path = home.join(file);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}
fn request(home: &Path) -> ScanRequest {
    let roots: BTreeMap<String, String> = [
        ("codex", ".codex"),
        ("claude", ".claude"),
        ("antigravity", ".gemini"),
        ("opencode", ".config/opencode"),
    ]
    .into_iter()
    .map(|(id, path)| {
        let root = home.join(path);
        fs::create_dir_all(&root).unwrap();
        (id.into(), root.to_string_lossy().into_owned())
    })
    .collect();
    ScanRequest {
        roots,
        project_path: None,
        custom_agents: vec![],
    }
}

#[test]
fn empty_configuration_is_not_a_scan_failure() {
    let home = TempDir::new().unwrap();
    let snapshot = scan_at(home.path(), request(home.path())).unwrap();
    assert_eq!(snapshot.agents.len(), 8);
    assert!(snapshot
        .agents
        .iter()
        .all(|a| a.resources.is_empty() && a.warnings.is_empty()));
}

#[test]
fn secrets_do_not_leave_native_configuration() {
    let home = TempDir::new().unwrap();
    write(
        home.path(),
        ".codex/config.toml",
        r#"
[mcp_servers.private]
command = "python SECRET_IN_COMMAND"
args = ["SECRET_IN_ARGS"]
enabled = false
[mcp_servers.private.env]
API_KEY = "SECRET_IN_ENV"
[plugins."example@local"]
enabled = true
"#,
    );
    write(
        home.path(),
        ".claude/settings.json",
        r#"{"env":{"TOKEN":"SECRET_IN_SETTINGS"},"hooks":{"Stop":[{"hooks":[{"type":"command","command":"echo SECRET_IN_HOOK"}]}]}}"#,
    );
    let snapshot = scan_at(home.path(), request(home.path())).unwrap();
    let json = serde_json::to_string(&snapshot).unwrap();
    for secret in [
        "SECRET_IN_COMMAND",
        "SECRET_IN_ARGS",
        "SECRET_IN_ENV",
        "SECRET_IN_SETTINGS",
        "SECRET_IN_HOOK",
    ] {
        assert!(!json.contains(secret), "{secret} escaped into inventory");
    }
    let mcp = snapshot.agents[0]
        .resources
        .iter()
        .find(|r| r.kind == "mcp")
        .unwrap();
    assert_eq!(mcp.status, "disabled");
    assert!(mcp.details.iter().any(|d| d.value == "API_KEY"));
    assert!(snapshot.agents[1]
        .resources
        .iter()
        .any(|r| r.kind == "hook" && r.name == "Stop"));
}

#[test]
fn detects_hook_and_installed_plugin_without_executing_code() {
    let home = TempDir::new().unwrap();
    write(
        home.path(),
        ".gemini/config/hooks.json",
        r#"{"notion-auto-logger":{"Stop":[{"type":"command","command":"python /external/run.py antigravity","timeout":30}]}}"#,
    );
    write(
        home.path(),
        ".config/opencode/plugins/session-logger.js",
        "// session.idle run.py\nthrow new Error('must not execute');",
    );
    write(
        home.path(),
        ".config/opencode/tools/custom.ts",
        "throw new Error('must not import');",
    );
    let before = fs::read(home.path().join(".gemini/config/hooks.json")).unwrap();
    let snapshot = scan_at(home.path(), request(home.path())).unwrap();
    let hook = snapshot.agents[2]
        .resources
        .iter()
        .find(|r| r.kind == "hook")
        .unwrap();
    assert_eq!(hook.name, "notion-auto-logger");
    assert!(hook.details.iter().any(|d| d.value == "30초"));
    let plugin = snapshot.agents[3]
        .resources
        .iter()
        .find(|r| r.kind == "plugin")
        .unwrap();
    assert!(plugin
        .details
        .iter()
        .any(|d| d.value.contains("session.idle")));
    assert!(snapshot.agents[3]
        .resources
        .iter()
        .any(|r| r.kind == "tool"));
    assert_eq!(
        fs::read(home.path().join(".gemini/config/hooks.json")).unwrap(),
        before
    );
}

#[test]
fn malformed_settings_do_not_hide_other_agents() {
    let home = TempDir::new().unwrap();
    write(home.path(), ".codex/config.toml", "invalid = [");
    write(home.path(),".config/opencode/opencode.jsonc","{\n// comment\nmcp: { external: { type: 'remote', url: 'https://example.test/?token=SECRET_URL', enabled: true }, },\n}");
    let snapshot = scan_at(home.path(), request(home.path())).unwrap();
    assert_eq!(snapshot.agents[0].warnings.len(), 1);
    assert!(snapshot.agents[3]
        .resources
        .iter()
        .any(|r| r.kind == "mcp" && r.status == "enabled"));
    assert!(!serde_json::to_string(&snapshot)
        .unwrap()
        .contains("SECRET_URL"));
}

#[test]
fn project_resources_and_common_skills_are_scoped_and_deduplicated() {
    let home = TempDir::new().unwrap();
    write(home.path(), ".agents/skills/example/SKILL.md", "# example");
    write(
        home.path(),
        "project/.agents/skills/shared/SKILL.md",
        "# shared",
    );
    write(
        home.path(),
        "project/.claude/skills/review/SKILL.md",
        "# review",
    );
    write(
        home.path(),
        "project/.mcp.json",
        r#"{"mcpServers":{"local":{"command":"python"}}}"#,
    );
    let mut request = request(home.path());
    request.project_path = Some(home.path().join("project").to_string_lossy().into_owned());
    let snapshot = scan_at(home.path(), request).unwrap();
    assert!(snapshot.agents[0]
        .resources
        .iter()
        .any(|r| r.name == "example" && r.scope == "공통 사용자"));
    assert!(snapshot.agents[0]
        .resources
        .iter()
        .any(|r| r.name == "shared" && r.scope == "프로젝트 공통"));
    assert!(snapshot.agents[1]
        .resources
        .iter()
        .any(|r| r.name == "review" && r.scope == "프로젝트"));
    assert!(snapshot.agents[1]
        .resources
        .iter()
        .any(|r| r.kind == "mcp" && r.scope == "프로젝트"));
}

#[test]
fn rejects_invalid_scan_paths_and_limits_config_size() {
    let home = TempDir::new().unwrap();
    let mut invalid = request(home.path());
    invalid.project_path = Some("relative/path".into());
    assert!(scan_at(home.path(), invalid).is_err());
    write(
        home.path(),
        ".codex/config.toml",
        &"x".repeat(2 * 1024 * 1024 + 1),
    );
    let snapshot = scan_at(home.path(), request(home.path())).unwrap();
    assert!(snapshot.agents[0]
        .warnings
        .iter()
        .any(|w| w.contains("크기 제한")));
}

#[test]
fn bundled_provenance_preserves_user_skills_and_plugin_overrides() {
    let home = TempDir::new().unwrap();
    write(
        home.path(),
        ".codex/skills/.system/review/SKILL.md",
        "# system",
    );
    write(home.path(), ".codex/skills/review/SKILL.md", "# user copy");
    write(
        home.path(),
        ".agents/skills/openai-docs/SKILL.md",
        "# user skill",
    );
    write(
        home.path(),
        ".codex/config.toml",
        r#"
[plugins."browser@openai-bundled"]
enabled = true
[plugins."pdf@openai-primary-runtime"]
enabled = true
[plugins."computer-use@openai-bundled"]
enabled = false
[plugins."notion@openai-curated-remote"]
enabled = true
[plugins."browser@my-marketplace"]
enabled = true
"#,
    );
    let snapshot = scan_at(home.path(), request(home.path())).unwrap();
    let resources = &snapshot.agents[0].resources;
    let bundled = resources
        .iter()
        .filter(|r| r.origin == "bundled")
        .collect::<Vec<_>>();
    assert_eq!(bundled.len(), 3);
    assert!(bundled
        .iter()
        .any(|r| r.name == "review" && r.scope == "기본 제공"));
    let managed = resources
        .iter()
        .filter(|r| r.origin != "bundled")
        .collect::<Vec<_>>();
    for name in [
        "review",
        "openai-docs",
        "notion@openai-curated-remote",
        "browser@my-marketplace",
        "computer-use@openai-bundled",
    ] {
        assert!(
            managed.iter().any(|r| r.name == name),
            "User configuration disappeared: {name}"
        );
    }
    assert!(managed
        .iter()
        .any(|r| r.name == "computer-use@openai-bundled" && r.status == "disabled"));
}

#[test]
#[cfg(target_os = "windows")]
fn windows_inventory_formats_all_reported_paths_consistently() {
    let home = TempDir::new().unwrap();
    write(home.path(), ".agents/skills/shared/SKILL.md", "# shared");
    write(
        home.path(),
        ".gemini/antigravity/skills/example/SKILL.md",
        "# example",
    );
    write(home.path(), ".codex/config.toml", "invalid = [");
    fs::create_dir_all(home.path().join("project")).unwrap();
    let mut request = request(home.path());
    for root in request.roots.values_mut() {
        *root = root.replace('\\', "/");
    }
    request.project_path = Some(
        home.path()
            .join("project")
            .to_string_lossy()
            .replace('\\', "/"),
    );
    let input_home = home.path().to_string_lossy().replace('\\', "/");
    let snapshot = scan_at(Path::new(&input_home), request).unwrap();
    assert!(!snapshot.home.contains('/'));
    assert!(!snapshot.project_path.unwrap().contains('/'));
    for agent in snapshot.agents {
        assert!(agent.config_roots.iter().all(|p| !p.contains('/')));
        assert!(agent.executable.iter().all(|p| !p.contains('/')));
        assert!(agent.warnings.iter().all(|w| !w.contains('/')));
        for resource in agent.resources {
            assert!(!resource.path.contains('/'), "{}", resource.path);
            assert!(!resource.id.contains('/'));
            for group in resource.details.iter().filter(|d| d.label == "파일 묶음") {
                assert!(!group.value.contains('/'));
            }
        }
    }
}

#[test]
#[cfg(not(target_os = "windows"))]
fn unix_inventory_preserves_backslashes_in_file_names() {
    let home = TempDir::new().unwrap();
    write(
        home.path(),
        ".agents/skills/custom\\name/SKILL.md",
        "# custom",
    );
    let snapshot = scan_at(home.path(), request(home.path())).unwrap();
    let resource = snapshot.agents[0]
        .resources
        .iter()
        .find(|r| r.kind == "skill")
        .unwrap();
    assert!(resource.path.contains("custom\\name/SKILL.md"));
}

#[test]
#[cfg(target_os = "windows")]
fn runtime_provenance_uses_executable_location_not_mcp_name() {
    let home = TempDir::new().unwrap();
    let runtime = "AppData/Local/OpenAI/Codex/runtimes/cua_node/version/bin/node_repl.exe";
    write(home.path(), runtime, "test fixture, never execute");
    write(
        home.path(),
        "custom/node_repl.exe",
        "test fixture, never execute",
    );
    let supplied = home
        .path()
        .join(runtime)
        .to_string_lossy()
        .replace('\\', "/");
    let custom = home
        .path()
        .join("custom/node_repl.exe")
        .to_string_lossy()
        .replace('\\', "/");
    write(
        home.path(),
        ".codex/config.toml",
        &format!(
            r#"
[mcp_servers.runtime_alias]
command = '{supplied}'
[mcp_servers.node_repl]
command = '{custom}'
[mcp_servers.disabled_runtime]
command = '{supplied}'
enabled = false
"#
        ),
    );
    let snapshot = scan_at(home.path(), request(home.path())).unwrap();
    let resources = &snapshot.agents[0].resources;
    for (name, origin) in [
        ("runtime_alias", "bundled"),
        ("node_repl", "user"),
        ("disabled_runtime", "user"),
    ] {
        assert_eq!(
            resources.iter().find(|r| r.name == name).unwrap().origin,
            origin
        );
    }
}

#[test]
fn discovers_additional_agents_and_separates_shared_gemini_directory() {
    let home = TempDir::new().unwrap();
    write(
        home.path(),
        ".gemini/settings.json",
        r#"{"mcpServers":{"gemini-tool":{"command":"unused","env":{"TOKEN":"SECRET"}}}}"#,
    );
    write(
        home.path(),
        ".cursor/mcp.json",
        r#"{"mcpServers":{"cursor-tool":{"url":"https://SECRET.example"}}}"#,
    );
    write(home.path(), ".copilot/settings.json", "{}");
    write(home.path(), ".codeium/windsurf/mcp_config.json", "{}");
    let snapshot = scan_at(home.path(), request(home.path())).unwrap();
    let by_id = |id: &str| snapshot.agents.iter().find(|a| a.id == id).unwrap();
    assert!(by_id("gemini")
        .resources
        .iter()
        .any(|r| r.name == "gemini-tool"));
    assert!(!by_id("antigravity")
        .resources
        .iter()
        .any(|r| r.name == "gemini-tool"));
    assert!(by_id("cursor")
        .resources
        .iter()
        .any(|r| r.name == "cursor-tool"));
    assert!(!by_id("copilot").resources.is_empty());
    assert!(!by_id("windsurf").resources.is_empty());
    assert_eq!(by_id("codex").inspection, "app-server");
    assert_eq!(by_id("cursor").inspection, "file");
    assert!(!serde_json::to_string(&snapshot).unwrap().contains("SECRET"));
}
#[test]
fn custom_agents_are_bounded_read_only_and_preserve_missing_registrations() {
    use backpack_core::CustomAgent;
    let home = TempDir::new().unwrap();
    write(
        home.path(),
        "custom/options.json",
        r#"{"mcpServers":{"my-tool":{"command":"do-not-run"}}}"#,
    );
    write(home.path(), "custom/skills/custom-skill/SKILL.md", "custom");
    write(home.path(), "custom/tool.exe", "not an executable");
    let custom = CustomAgent {
        id: "custom-example".into(),
        name: "Other AI".into(),
        config_root: home.path().join("custom").to_string_lossy().into_owned(),
        executable: Some(
            home.path()
                .join("custom/tool.exe")
                .to_string_lossy()
                .into_owned(),
        ),
        config_files: vec!["options.json".into()],
    };
    let mut req = request(home.path());
    req.custom_agents.push(custom.clone());
    let result = scan_at(home.path(), req.clone()).unwrap();
    let agent = result.agents.last().unwrap();
    assert!(agent.custom);
    assert!(agent.executable.is_some());
    assert!(agent.resources.iter().any(|r| r.name == "my-tool"));
    assert!(agent.resources.iter().any(|r| r.name == "custom-skill"));
    req.custom_agents[0].config_root = home.path().join("missing").to_string_lossy().into_owned();
    assert!(!scan_at(home.path(), req.clone())
        .unwrap()
        .agents
        .last()
        .unwrap()
        .warnings
        .is_empty());
    req.custom_agents.push(custom);
    assert!(scan_at(home.path(), req.clone()).is_err());
    req.custom_agents.pop();
    req.custom_agents[0].config_files = vec!["../auth.json".into()];
    assert!(scan_at(home.path(), req.clone()).is_err());
    req.custom_agents[0].id = "codex".into();
    assert!(scan_at(home.path(), req).is_err());
}
