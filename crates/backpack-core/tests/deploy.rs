use backpack_core::{deploy, library, scan_at, ScanRequest};
use std::{
    fs,
    path::{Path, PathBuf},
};
use tempfile::TempDir;
struct Fixture {
    dir: TempDir,
    request: ScanRequest,
    library: String,
    item: String,
    records: PathBuf,
}
fn setup() -> Fixture {
    let dir = TempDir::new().unwrap();
    let home = dir.path();
    let source = home.join(".codex/skills/sample/scripts");
    fs::create_dir_all(&source).unwrap();
    fs::write(
        source.parent().unwrap().join("SKILL.md"),
        "---\nname: sample\ndescription: Example skill\n---\n# Example\n",
    )
    .unwrap();
    fs::write(
        source.join("helper.py"),
        "raise Exception('must not execute')\n",
    )
    .unwrap();
    let shared = home.join("shared");
    let project = home.join("project");
    fs::create_dir(&shared).unwrap();
    fs::create_dir(&project).unwrap();
    let library = shared.to_string_lossy().into_owned();
    library::connect(&library).unwrap();
    let request = ScanRequest {
        project_path: Some(project.to_string_lossy().into_owned()),
        ..Default::default()
    };
    let resource = scan_at(home, request.clone())
        .unwrap()
        .agents
        .into_iter()
        .flat_map(|a| a.resources)
        .find(|r| r.name == "sample")
        .unwrap()
        .id;
    let item =
        library::import(library::prepare_at(home, request.clone(), &library, &resource).unwrap())
            .unwrap()
            .id;
    let records = home.join("records");
    Fixture {
        dir,
        request,
        library,
        item,
        records,
    }
}
impl Fixture {
    fn prepare(&self, target: &str) -> deploy::PreparedDeployment {
        deploy::prepare_install_at(
            self.dir.path(),
            &self.request,
            &self.library,
            &self.item,
            target,
            &self.records,
        )
        .unwrap()
    }
    fn existing(&self, id: &str, action: &str) -> Result<deploy::PreparedDeployment, String> {
        deploy::prepare_existing_at(self.dir.path(), &self.request, id, action, &self.records)
    }
    fn data(&self) -> deploy::DeploymentData {
        deploy::data_at(
            self.dir.path(),
            &self.request,
            &self.library,
            &self.item,
            &self.records,
        )
        .unwrap()
    }
}
#[test]
fn skill_install_remove_restore_and_discovery() {
    let f = setup();
    let initial = f.data();
    assert_eq!(initial.targets.len(), 10);
    let result = deploy::apply(f.prepare("claude:project")).unwrap();
    assert_eq!(result.state, "installed");
    let installed = Path::new(&result.path);
    assert!(installed.join("SKILL.md").is_file());
    assert!(f.dir.path().join(".codex/skills/sample/SKILL.md").is_file());
    assert!(scan_at(f.dir.path(), f.request.clone())
        .unwrap()
        .agents
        .iter()
        .find(|a| a.id == "claude")
        .unwrap()
        .resources
        .iter()
        .any(|r| r.name == "sample"));
    assert_eq!(f.data().installations.len(), 1);
    let result = deploy::apply(f.existing(&result.id, "remove").unwrap()).unwrap();
    assert_eq!(result.state, "disabled");
    assert!(!installed.exists());
    let stored = f
        .dir
        .path()
        .join("project/.claude/.backpack-disabled")
        .join(&result.id);
    assert!(stored.join("SKILL.md").is_file());
    assert!(!scan_at(f.dir.path(), f.request.clone())
        .unwrap()
        .agents
        .iter()
        .find(|a| a.id == "claude")
        .unwrap()
        .resources
        .iter()
        .any(|r| r.name == "sample"));
    let result = deploy::apply(f.existing(&result.id, "restore").unwrap()).unwrap();
    assert_eq!(result.state, "installed");
    assert!(!stored.exists());
}
#[test]
fn existing_and_late_conflicts_are_not_overwritten() {
    let f = setup();
    let patch = f.prepare("codex:project");
    let target = PathBuf::from(&patch.view.path);
    fs::create_dir_all(&target).unwrap();
    fs::write(target.join("keep.md"), "keep").unwrap();
    assert!(deploy::apply(patch).is_err());
    assert_eq!(fs::read_to_string(target.join("keep.md")).unwrap(), "keep");
    assert!(deploy::prepare_install_at(
        f.dir.path(),
        &f.request,
        &f.library,
        &f.item,
        "codex:project",
        &f.records
    )
    .is_err());
}
#[test]
fn external_edits_and_extra_empty_folders_block_removal() {
    let f = setup();
    let result = deploy::apply(f.prepare("gemini:project")).unwrap();
    let target = PathBuf::from(&result.path);
    let patch = f.existing(&result.id, "remove").unwrap();
    fs::write(target.join("extra.md"), "keep").unwrap();
    assert!(deploy::apply(patch).is_err());
    assert!(target.exists());
    assert_eq!(f.data().installations[0].state, "changed");
    assert!(f.existing(&result.id, "remove").is_err());
    fs::remove_file(target.join("extra.md")).unwrap();
    fs::create_dir(target.join("extra-empty")).unwrap();
    assert!(f.existing(&result.id, "remove").is_err());
    fs::remove_dir(target.join("extra-empty")).unwrap();
    fs::write(target.join("SKILL.md"), "edited").unwrap();
    assert!(f.existing(&result.id, "remove").is_err());
}
#[test]
fn restore_conflicts_and_modified_quarantine_are_preserved() {
    let f = setup();
    let installed = deploy::apply(f.prepare("opencode:project")).unwrap();
    let result = deploy::apply(f.existing(&installed.id, "remove").unwrap()).unwrap();
    let patch = f.existing(&result.id, "restore").unwrap();
    let target = PathBuf::from(&result.path);
    fs::create_dir(&target).unwrap();
    assert!(deploy::apply(patch).is_err());
    fs::remove_dir(&target).unwrap();
    let stored = f
        .dir
        .path()
        .join("project/.opencode/.backpack-disabled")
        .join(&result.id);
    fs::write(stored.join("SKILL.md"), "edited").unwrap();
    assert!(f.existing(&result.id, "restore").is_err());
    assert!(stored.exists());
}
#[test]
fn changed_library_and_foreign_records_are_rejected() {
    let f = setup();
    let patch = f.prepare("antigravity:project");
    let file = Path::new(&f.library)
        .join(".backpack-library/items")
        .join(&f.item)
        .join("files/SKILL.md");
    fs::write(file, "modified").unwrap();
    assert!(deploy::apply(patch).is_err());
    let f = setup();
    let result = deploy::apply(f.prepare("claude:project")).unwrap();
    let file = f.records.join(&result.id).join("record.json");
    let mut record: serde_json::Value = serde_json::from_slice(&fs::read(&file).unwrap()).unwrap();
    record["target"]["path"] = serde_json::json!(f.dir.path().join("unrelated").to_string_lossy());
    fs::write(file, serde_json::to_vec(&record).unwrap()).unwrap();
    assert!(f.existing(&result.id, "remove").is_err());
    assert!(Path::new(&result.path).exists());
}
#[test]
fn malformed_skill_header_and_unsupported_target_are_rejected() {
    let f = setup();
    assert!(deploy::prepare_install_at(
        f.dir.path(),
        &f.request,
        &f.library,
        &f.item,
        "custom:project",
        &f.records
    )
    .is_err());
    let home = f.dir.path();
    fs::write(home.join(".codex/skills/sample/SKILL.md"), "# no metadata").unwrap();
    let resource = scan_at(home, f.request.clone())
        .unwrap()
        .agents
        .into_iter()
        .flat_map(|a| a.resources)
        .find(|r| r.name == "sample")
        .unwrap()
        .id;
    let item = library::import(
        library::prepare_at(home, f.request.clone(), &f.library, &resource).unwrap(),
    )
    .unwrap();
    assert!(deploy::prepare_install_at(
        home,
        &f.request,
        &f.library,
        &item.id,
        "claude:project",
        &f.records
    )
    .is_err());
}
#[test]
fn opencode_tool_is_copied_without_execution_and_cannot_install_to_claude() {
    let f = setup();
    let source = f.dir.path().join("project/.opencode/tools");
    fs::create_dir_all(&source).unwrap();
    fs::write(
        source.join("custom.ts"),
        "throw new Error('do not execute');",
    )
    .unwrap();
    let resource = scan_at(f.dir.path(), f.request.clone())
        .unwrap()
        .agents
        .into_iter()
        .flat_map(|a| a.resources)
        .find(|r| r.kind == "tool" && r.name == "custom.ts")
        .unwrap()
        .id;
    let item = library::import(
        library::prepare_at(f.dir.path(), f.request.clone(), &f.library, &resource).unwrap(),
    )
    .unwrap();
    let data = deploy::data_at(f.dir.path(), &f.request, &f.library, &item.id, &f.records).unwrap();
    assert!(data.targets.iter().all(|t| t.agent_id == "opencode"));
    let patch = deploy::prepare_install_at(
        f.dir.path(),
        &f.request,
        &f.library,
        &item.id,
        "opencode:user",
        &f.records,
    )
    .unwrap();
    let result = deploy::apply(patch).unwrap();
    assert_eq!(
        fs::read_to_string(&result.path).unwrap(),
        "throw new Error('do not execute');"
    );
}

#[test]
fn library_overlap_and_project_context_changes_are_blocked() {
    let mut f = setup();
    f.request.roots.insert("claude".into(), f.library.clone());
    assert!(deploy::prepare_install_at(
        f.dir.path(),
        &f.request,
        &f.library,
        &f.item,
        "claude:user",
        &f.records
    )
    .is_err());
    let result = deploy::apply(f.prepare("claude:project")).unwrap();
    f.request.project_path = None;
    assert!(f.existing(&result.id, "remove").is_err());
    assert!(Path::new(&result.path).exists());
    assert!(f.data().installations.is_empty());
}
#[test]
fn claude_rule_install_remove_restore_keeps_exact_bytes() {
    let f = setup();
    let source = f.dir.path().join(".claude/rules");
    fs::create_dir_all(&source).unwrap();
    let contents = "# Rule\nKeep source bytes.\n";
    fs::write(source.join("sample.md"), contents).unwrap();
    let resource = scan_at(f.dir.path(), f.request.clone())
        .unwrap()
        .agents
        .into_iter()
        .flat_map(|a| a.resources)
        .find(|r| r.kind == "rule" && r.agent_id == "claude")
        .unwrap()
        .id;
    let item = library::import(
        library::prepare_at(f.dir.path(), f.request.clone(), &f.library, &resource).unwrap(),
    )
    .unwrap();
    let patch = deploy::prepare_install_at(
        f.dir.path(),
        &f.request,
        &f.library,
        &item.id,
        "claude:project",
        &f.records,
    )
    .unwrap();
    let result = deploy::apply(patch).unwrap();
    assert_eq!(fs::read_to_string(&result.path).unwrap(), contents);
    let result = deploy::apply(f.existing(&result.id, "remove").unwrap()).unwrap();
    assert_eq!(result.state, "disabled");
    let result = deploy::apply(f.existing(&result.id, "restore").unwrap()).unwrap();
    assert_eq!(fs::read_to_string(&result.path).unwrap(), contents);
}
#[cfg(unix)]
#[test]
fn symlink_parent_and_installed_link_prevent_mutation() {
    use std::os::unix::fs::symlink;
    let f = setup();
    let outside = f.dir.path().join("outside");
    fs::create_dir(&outside).unwrap();
    symlink(&outside, f.dir.path().join("project/.claude")).unwrap();
    assert!(deploy::prepare_install_at(
        f.dir.path(),
        &f.request,
        &f.library,
        &f.item,
        "claude:project",
        &f.records
    )
    .is_err());
    let result = deploy::apply(f.prepare("gemini:project")).unwrap();
    symlink(&outside, Path::new(&result.path).join("link")).unwrap();
    assert!(f.existing(&result.id, "remove").is_err());
    assert!(outside.exists());
}
