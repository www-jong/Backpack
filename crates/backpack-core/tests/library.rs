use backpack_core::{library::*, scan_at, ScanRequest};
use std::{fs, path::Path};
use tempfile::TempDir;
fn setup() -> (TempDir, ScanRequest, String) {
    let dir = TempDir::new().unwrap();
    let home = dir.path();
    fs::create_dir_all(home.join(".codex/skills/sample/scripts")).unwrap();
    fs::write(
        home.join(".codex/skills/sample/SKILL.md"),
        "# sample\nuse scripts/helper.py\n",
    )
    .unwrap();
    fs::write(
        home.join(".codex/skills/sample/scripts/helper.py"),
        "print('never executed')\n",
    )
    .unwrap();
    fs::write(
        home.join(".codex/skills/sample/.env"),
        "TOKEN=SECRET_LOCAL\n",
    )
    .unwrap();
    fs::create_dir(home.join("shared")).unwrap();
    let shared = home.join("shared").to_string_lossy().into_owned();
    connect(&shared).unwrap();
    (dir, ScanRequest::default(), shared)
}
fn id(home: &Path, req: &ScanRequest, name: &str) -> String {
    scan_at(home, req.clone())
        .unwrap()
        .agents
        .into_iter()
        .flat_map(|a| a.resources)
        .find(|r| r.name == name)
        .unwrap()
        .id
}
#[test]
fn import_skill_bundle_excludes_credentials_and_compares_changed_files() {
    let (dir, req, shared) = setup();
    let resource = id(dir.path(), &req, "sample");
    let preview = prepare_at(dir.path(), req.clone(), &shared, &resource).unwrap();
    assert_eq!(preview.view.files.len(), 2);
    assert_eq!(preview.view.skipped, vec![".env"]);
    assert!(!serde_json::to_string(&preview.view)
        .unwrap()
        .contains("SECRET_LOCAL"));
    assert!(list(&shared).unwrap().is_empty());
    let entry = import(preview).unwrap();
    assert_eq!(list(&shared).unwrap().len(), 1);
    let manifest = fs::read_to_string(
        Path::new(&shared)
            .join(".backpack-library/items")
            .join(&entry.id)
            .join("manifest.json"),
    )
    .unwrap();
    assert!(!manifest.contains(&dir.path().to_string_lossy().to_string()));
    assert!(
        compare_at(dir.path(), &req, &shared, &entry.id, &resource)
            .unwrap()
            .identical
    );
    fs::write(
        dir.path().join(".codex/skills/sample/scripts/helper.py"),
        "# changed",
    )
    .unwrap();
    fs::write(dir.path().join(".codex/skills/sample/new.md"), "new").unwrap();
    let compared = compare_at(dir.path(), &req, &shared, &entry.id, &resource).unwrap();
    assert!(!compared.identical);
    assert!(compared.files.iter().any(|f| f.status == "changed"));
    assert!(compared.files.iter().any(|f| f.status == "localOnly"));
    fs::remove_file(dir.path().join(".codex/skills/sample/scripts/helper.py")).unwrap();
    assert!(compare_at(dir.path(), &req, &shared, &entry.id, &resource)
        .unwrap()
        .files
        .iter()
        .any(|f| f.status == "libraryOnly"));
}
#[test]
fn stale_import_and_token_files_are_rejected_without_publishing() {
    let (dir, req, shared) = setup();
    let resource = id(dir.path(), &req, "sample");
    let preview = prepare_at(dir.path(), req.clone(), &shared, &resource).unwrap();
    fs::write(dir.path().join(".codex/skills/sample/new.md"), "new").unwrap();
    assert!(import(preview).is_err());
    assert!(list(&shared).unwrap().is_empty());
    fs::write(
        dir.path().join(".codex/skills/sample/new.md"),
        "ghp_fake_test_credential",
    )
    .unwrap();
    assert!(prepare_at(dir.path(), req.clone(), &shared, &resource).is_err());
    assert!(prepare_at(dir.path(), req, &shared, "foreign-resource").is_err());
}
#[test]
fn hook_import_extracts_only_selected_definition_and_keeps_original() {
    let (dir, req, shared) = setup();
    fs::create_dir_all(dir.path().join(".gemini/config")).unwrap();
    let original = r#"{"logger":{"Stop":[{"type":"command","command":"python /machine/run.py","timeout":30}]},"other":{"Stop":[{"type":"command","command":"other"}]},"oauth":{"token":"SECRET_UNRELATED"}}"#;
    fs::write(dir.path().join(".gemini/config/hooks.json"), original).unwrap();
    let auth_id = id(dir.path(), &req, "oauth");
    assert!(prepare_at(dir.path(), req.clone(), &shared, &auth_id).is_err());
    let resource = id(dir.path(), &req, "logger");
    let preview = prepare_at(dir.path(), req.clone(), &shared, &resource).unwrap();
    assert_eq!(preview.view.files[0].path, "hook.json");
    let entry = import(preview).unwrap();
    let saved = fs::read_to_string(
        Path::new(&shared)
            .join(".backpack-library/items")
            .join(&entry.id)
            .join("files/hook.json"),
    )
    .unwrap();
    assert!(saved.contains("logger"));
    assert!(!saved.contains("SECRET_UNRELATED"));
    assert!(!saved.contains("other"));
    assert_eq!(
        fs::read_to_string(dir.path().join(".gemini/config/hooks.json")).unwrap(),
        original
    );
    assert!(
        compare_at(dir.path(), &req, &shared, &entry.id, &resource)
            .unwrap()
            .identical
    );
}
#[test]
fn invalid_manifest_and_modified_library_payload_are_rejected() {
    let (dir, req, shared) = setup();
    let resource = id(dir.path(), &req, "sample");
    let entry = import(prepare_at(dir.path(), req.clone(), &shared, &resource).unwrap()).unwrap();
    let item = Path::new(&shared)
        .join(".backpack-library/items")
        .join(&entry.id);
    fs::write(item.join("files/SKILL.md"), "tampered").unwrap();
    assert!(compare_at(dir.path(), &req, &shared, &entry.id, &resource).is_err());
    let mut value: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(item.join("manifest.json")).unwrap()).unwrap();
    value["files"][0]["path"] = serde_json::json!("../outside");
    fs::write(item.join("manifest.json"), value.to_string()).unwrap();
    assert!(list(&shared).is_err());
    assert!(compare_at(dir.path(), &req, &shared, "../outside", &resource).is_err());
}
#[test]
fn binary_assets_and_unconnected_storage_are_rejected() {
    let (dir, req, shared) = setup();
    let resource = id(dir.path(), &req, "sample");
    fs::write(dir.path().join(".codex/skills/sample/image.png"), [0, 1, 2]).unwrap();
    assert!(prepare_at(dir.path(), req.clone(), &shared, &resource).is_err());
    fs::remove_file(dir.path().join(".codex/skills/sample/image.png")).unwrap();
    assert!(prepare_at(
        dir.path(),
        req,
        &dir.path().join(".codex/skills/sample").to_string_lossy(),
        &resource
    )
    .is_err());
}
#[cfg(unix)]
#[test]
fn linked_skill_content_is_not_followed() {
    let (dir, req, shared) = setup();
    let resource = id(dir.path(), &req, "sample");
    std::os::unix::fs::symlink(
        dir.path().join("shared"),
        dir.path().join(".codex/skills/sample/link"),
    )
    .unwrap();
    assert!(prepare_at(dir.path(), req, &shared, &resource).is_err());
}
