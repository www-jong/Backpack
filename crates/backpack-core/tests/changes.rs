use backpack_core::{changes::*, ScanRequest};
use std::{fs, path::Path};
use tempfile::TempDir;
fn setup(id: &str, file: &str, text: &str) -> (TempDir, ScanRequest, McpDraft) {
    let dir = TempDir::new().unwrap();
    let root = dir.path().join("config");
    fs::create_dir(&root).unwrap();
    let path = root.join(file);
    if !text.is_empty() {
        fs::write(&path, text).unwrap();
    }
    let request = ScanRequest {
        roots: [(id.into(), root.to_string_lossy().into_owned())]
            .into_iter()
            .collect(),
        ..Default::default()
    };
    let draft = McpDraft {
        agent_id: id.into(),
        path: path
            .to_string_lossy()
            .replace('/', if cfg!(windows) { "\\" } else { "/" }),
        name: "sample".into(),
        action: "disable".into(),
        ..Default::default()
    };
    (dir, request, draft)
}
#[test]
fn toml_preview_apply_backup_restore_preserves_secrets_and_comments() {
    let original="# keep\nmodel = 'kept'\n[mcp_servers.sample]\ncommand = 'python'\nenabled = true # keep this\n[mcp_servers.sample.env]\nTOKEN = 'SECRET_ORIGINAL'\n";
    let (dir, req, draft) = setup("codex", "config.toml", original);
    let path = Path::new(&draft.path);
    let prepared = prepare_at(dir.path(), &req, draft.clone()).unwrap();
    assert_eq!(fs::read_to_string(path).unwrap(), original);
    assert!(!serde_json::to_string(&prepared.view)
        .unwrap()
        .contains("SECRET"));
    let backups = dir.path().join("backups");
    let receipt = apply(prepared, &backups).unwrap();
    let changed = fs::read_to_string(path).unwrap();
    assert!(changed.contains("enabled = false # keep this"));
    assert!(changed.contains("TOKEN = 'SECRET_ORIGINAL'"));
    assert!(changed.contains("model = 'kept'"));
    assert_eq!(
        fs::read_to_string(backups.join(&receipt.id).join("original")).unwrap(),
        original
    );
    restore_at(dir.path(), &req, "codex", &backups, &receipt.id).unwrap();
    assert_eq!(fs::read_to_string(path).unwrap(), original);
    assert!(!list_backups_at(dir.path(), &req, "codex", &backups).unwrap()[0].restorable);
}
#[test]
fn jsonc_preserves_comments_other_properties_and_restore_exact_bytes() {
    let original="{\r\n  // KEEP\r\n  \"model\": \"kept\",\r\n  \"mcp\": {\r\n    \"sample\": {\"type\":\"local\", \"command\":[\"python\"], \"enabled\": true, /* SECRET_COMMENT */},\r\n  },\r\n}\r\n";
    let (dir, req, draft) = setup("opencode", "opencode.jsonc", original);
    let prepared = prepare_at(dir.path(), &req, draft.clone()).unwrap();
    let backups = dir.path().join("backups");
    let receipt = apply(prepared, &backups).unwrap();
    let changed = fs::read_to_string(&draft.path).unwrap();
    assert!(changed.contains("// KEEP"));
    assert!(changed.contains("/* SECRET_COMMENT */"));
    assert!(changed.contains("\"model\": \"kept\""));
    let value: serde_json::Value = json5::from_str(&changed).unwrap();
    assert_eq!(value["mcp"]["sample"]["enabled"], false);
    restore_at(dir.path(), &req, "opencode", &backups, &receipt.id).unwrap();
    assert_eq!(fs::read_to_string(&draft.path).unwrap(), original);
}
#[test]
fn stale_preview_and_modified_applied_file_are_not_overwritten() {
    let (dir, req, draft) = setup(
        "codex",
        "config.toml",
        "[mcp_servers.sample]\ncommand='unused'\nenabled=true\n",
    );
    let patch = prepare_at(dir.path(), &req, draft.clone()).unwrap();
    fs::write(&draft.path, "# external change").unwrap();
    assert!(apply(patch, &dir.path().join("backups")).is_err());
    assert_eq!(
        fs::read_to_string(&draft.path).unwrap(),
        "# external change"
    );
    let mut draft = draft;
    draft.action = "register".into();
    draft.command = "unused".into();
    let receipt = apply(
        prepare_at(dir.path(), &req, draft.clone()).unwrap(),
        &dir.path().join("backups"),
    )
    .unwrap();
    fs::write(&draft.path, "# later change").unwrap();
    assert!(restore_at(
        dir.path(),
        &req,
        "codex",
        &dir.path().join("backups"),
        &receipt.id
    )
    .is_err());
}
#[test]
fn registration_creates_file_without_running_server_and_can_restore_absence() {
    for (id, file) in [("codex", "config.toml"), ("opencode", "opencode.json")] {
        let (dir, req, mut draft) = setup(id, file, "");
        draft.action = "register".into();
        draft.command = "never-run-this-command".into();
        draft.args = vec!["SECRET_ARG".into()];
        draft.env_names = vec!["MY_TOKEN".into()];
        let patch = prepare_at(dir.path(), &req, draft.clone()).unwrap();
        assert!(patch.view.creates_file);
        assert!(!serde_json::to_string(&patch.view)
            .unwrap()
            .contains("SECRET_ARG"));
        let backup = dir.path().join("backups");
        let receipt = apply(patch, &backup).unwrap();
        let text = fs::read_to_string(&draft.path).unwrap();
        assert!(text.contains("MY_TOKEN"));
        assert!(prepare_at(dir.path(), &req, draft.clone()).is_err());
        restore_at(dir.path(), &req, id, &backup, &receipt.id).unwrap();
        assert!(!Path::new(&draft.path).exists());
    }
}
#[test]
fn rejects_foreign_paths_bad_formats_and_tampered_backup() {
    let (dir, req, mut draft) = setup(
        "opencode",
        "opencode.json",
        "{\"mcp\":{\"sample\":{\"type\":\"local\",\"command\":[\"unused\"],\"enabled\":true}}}",
    );
    let backup = dir.path().join("backups");
    let receipt = apply(
        prepare_at(dir.path(), &req, draft.clone()).unwrap(),
        &backup,
    )
    .unwrap();
    fs::write(backup.join(&receipt.id).join("original"), "tampered").unwrap();
    assert!(restore_at(dir.path(), &req, "opencode", &backup, &receipt.id).is_err());
    assert!(restore_at(dir.path(), &req, "opencode", &backup, "../outside").is_err());
    draft.path = dir
        .path()
        .join("foreign.json")
        .to_string_lossy()
        .into_owned();
    assert!(prepare_at(dir.path(), &req, draft.clone()).is_err());
    for text in [
        "{\"mcp\":{},\"mcp\":{}}",
        "{\"mcp\":{\"sample\":{\"type\":\"local\",\"enabled\":\"true\"}}}",
        "{\"mcp\":{\"servers\":{}}}",
        "invalid",
    ] {
        draft.path = targets_at(dir.path(), &req, "opencode")
            .unwrap()
            .into_iter()
            .find(|p| p.ends_with(".json"))
            .unwrap();
        fs::write(&draft.path, text).unwrap();
        assert!(prepare_at(dir.path(), &req, draft.clone()).is_err());
    }
}
#[cfg(unix)]
#[test]
fn linked_settings_are_not_replaced_and_backups_are_private() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    let (dir, req, draft) = setup(
        "codex",
        "config.toml",
        "[mcp_servers.sample]\ncommand='unused'\nenabled=true\n",
    );
    let backup = dir.path().join("backups");
    let receipt = apply(
        prepare_at(dir.path(), &req, draft.clone()).unwrap(),
        &backup,
    )
    .unwrap();
    assert_eq!(
        fs::metadata(backup.join(&receipt.id).join("original"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    fs::remove_file(&draft.path).unwrap();
    let external = dir.path().join("other.toml");
    fs::write(&external, "# keep").unwrap();
    symlink(external, &draft.path).unwrap();
    assert!(prepare_at(dir.path(), &req, draft).is_err());
}

#[test]
fn remote_registration_and_toggle_chain_preserve_existing_configuration() {
    for (id, file, original) in [
        ("codex", "config.toml", "# keep root\nmodel='kept'\n[mcp_servers.other]\ncommand='unused'\n"),
        ("opencode", "opencode.jsonc", "{ // keep root\n\"model\":\"kept\", \"mcp\":{\"other\":{\"type\":\"local\",\"command\":[\"unused\"]}}}\n"),
    ] {
        let (dir, req, mut draft) = setup(id, file, original);
        draft.action = "register".into();
        draft.url = "https://example.invalid/mcp".into();
        draft.token_env = "MY_REMOTE_TOKEN".into();
        draft.enabled = true;
        let backup = dir.path().join("backups");
        let registered = apply(prepare_at(dir.path(), &req, draft.clone()).unwrap(), &backup).unwrap();
        let text = fs::read_to_string(&draft.path).unwrap();
        assert!(text.contains("keep root"));
        if id == "codex" {
            let config: toml::Value = toml::from_str(&text).unwrap();
            assert_eq!(config["mcp_servers"]["sample"]["enabled"].as_bool(), Some(true));
            assert_eq!(config["mcp_servers"]["sample"]["bearer_token_env_var"].as_str(), Some("MY_REMOTE_TOKEN"));
            assert_eq!(config["mcp_servers"]["other"]["command"].as_str(), Some("unused"));
        } else {
            let config: serde_json::Value = json5::from_str(&text).unwrap();
            assert_eq!(config["mcp"]["sample"]["enabled"], true);
            assert_eq!(config["mcp"]["sample"]["headers"]["Authorization"], "Bearer {env:MY_REMOTE_TOKEN}");
            assert_eq!(config["mcp"]["other"]["command"][0], "unused");
        }
        draft.action = "disable".into();
        let disabled = apply(prepare_at(dir.path(), &req, draft.clone()).unwrap(), &backup).unwrap();
        draft.action = "enable".into();
        let enabled = apply(prepare_at(dir.path(), &req, draft.clone()).unwrap(), &backup).unwrap();
        assert_eq!(fs::read_to_string(&draft.path).unwrap(), text);
        restore_at(dir.path(), &req, id, &backup, &enabled.id).unwrap();
        restore_at(dir.path(), &req, id, &backup, &disabled.id).unwrap();
        restore_at(dir.path(), &req, id, &backup, &registered.id).unwrap();
        assert_eq!(fs::read_to_string(&draft.path).unwrap(), original);
        draft.action = "register".into();
        draft.url = "https://user:password@example.invalid/mcp".into();
        assert!(prepare_at(dir.path(), &req, draft).is_err());
    }
}
