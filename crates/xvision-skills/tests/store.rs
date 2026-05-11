use tempfile::tempdir;
use xvision_skills::store::{FilesystemSkillStore, SkillStore};

const FIXTURE: &str = include_str!("fixtures/crypto-trader-base.md");

#[tokio::test]
async fn save_and_load_roundtrip() {
    let dir = tempdir().unwrap();
    let store = FilesystemSkillStore::new(dir.path().to_path_buf());
    store.save("crypto-trader-base", FIXTURE).await.unwrap();
    let loaded = store.load("crypto-trader-base").await.unwrap();
    assert_eq!(loaded.name, "crypto-trader-base");
    assert!(loaded.body.contains("crypto trader"));
}

#[tokio::test]
async fn list_returns_saved_skills() {
    let dir = tempdir().unwrap();
    let store = FilesystemSkillStore::new(dir.path().to_path_buf());
    let skill_a = "---\nname: a\ndisplay_name: A\ndescription: x\nversion: \"1.0\"\nmodel_requirement: anthropic.claude-sonnet-4.6\n---\nbody";
    let skill_b = "---\nname: b\ndisplay_name: B\ndescription: x\nversion: \"1.0\"\nmodel_requirement: anthropic.claude-sonnet-4.6\n---\nbody";
    store.save("a", skill_a).await.unwrap();
    store.save("b", skill_b).await.unwrap();
    let names = store.list().await.unwrap();
    assert_eq!(names, vec!["a".to_string(), "b".to_string()]);
}

#[tokio::test]
async fn list_returns_empty_for_missing_dir() {
    let dir = tempdir().unwrap();
    let store = FilesystemSkillStore::new(dir.path().join("nope"));
    let names = store.list().await.unwrap();
    assert!(names.is_empty());
}
