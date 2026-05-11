use xvision_skills::parse;

const FIXTURE: &str = include_str!("fixtures/crypto-trader-base.md");

#[test]
fn parses_valid_skill() {
    let skill = parse(FIXTURE).expect("parse fixture");
    assert_eq!(skill.name, "crypto-trader-base");
    assert_eq!(skill.allowed_tools, vec!["ohlcv", "indicator_panel"]);
    assert!(skill.body.contains("crypto trader"));
    assert_eq!(skill.content_hash.len(), 64);
}

#[test]
fn rejects_missing_frontmatter() {
    let err = parse("just some text").unwrap_err();
    assert!(matches!(err, xvision_skills::SkillError::MissingFrontmatter));
}

#[test]
fn rejects_missing_required_field() {
    let bad = "---\nname: x\n---\nbody\n";
    let err = parse(bad).unwrap_err();
    assert!(matches!(err, xvision_skills::SkillError::MissingField(_)));
}
