use xvision_engine::autooptimizer::mutator::{
    empty_mutation, MutationDiff, MutationKind, ParamChange, ProseEdit, ToolDiff,
};

#[test]
fn mutation_diff_round_trips_through_serde_json() {
    let diff = MutationDiff {
        kind: MutationKind::Param,
        prose: vec![ProseEdit {
            agent_role: "trader".into(),
            before: "old prompt".into(),
            after: "new prompt".into(),
        }],
        params: vec![ParamChange {
            key: "temperature".into(),
            before: serde_json::json!(0.7),
            after: serde_json::json!(0.9),
        }],
        tools: ToolDiff {
            added: vec!["tool_a".into()],
            removed: vec!["tool_b".into()],
        },
        filter: Vec::new(),
        create_filter: None,
        rationale: "improve accuracy".into(),
    };

    let json = serde_json::to_string(&diff).expect("serialize");
    let restored: MutationDiff = serde_json::from_str(&json).expect("deserialize");

    assert_eq!(restored.kind, MutationKind::Param);
    assert_eq!(restored.rationale, diff.rationale);
    assert_eq!(restored.prose.len(), 1);
    assert_eq!(restored.prose[0].agent_role, "trader");
    assert_eq!(restored.prose[0].before, "old prompt");
    assert_eq!(restored.prose[0].after, "new prompt");
    assert_eq!(restored.params.len(), 1);
    assert_eq!(restored.params[0].key, "temperature");
    assert_eq!(restored.params[0].before, serde_json::json!(0.7));
    assert_eq!(restored.params[0].after, serde_json::json!(0.9));
    assert_eq!(restored.tools.added, vec!["tool_a"]);
    assert_eq!(restored.tools.removed, vec!["tool_b"]);
}

#[test]
fn is_empty_reflects_content() {
    let empty = empty_mutation();
    assert!(empty.is_empty());

    let with_prose = MutationDiff {
        kind: MutationKind::Prose,
        prose: vec![ProseEdit {
            agent_role: "trader".into(),
            before: "a".into(),
            after: "b".into(),
        }],
        params: Vec::new(),
        tools: ToolDiff {
            added: Vec::new(),
            removed: Vec::new(),
        },
        filter: Vec::new(),
        create_filter: None,
        rationale: String::new(),
    };
    assert!(!with_prose.is_empty());

    let with_params = MutationDiff {
        kind: MutationKind::Param,
        prose: Vec::new(),
        params: vec![ParamChange {
            key: "k".into(),
            before: serde_json::json!(1),
            after: serde_json::json!(2),
        }],
        tools: ToolDiff {
            added: Vec::new(),
            removed: Vec::new(),
        },
        filter: Vec::new(),
        create_filter: None,
        rationale: String::new(),
    };
    assert!(!with_params.is_empty());

    let with_tools_added = MutationDiff {
        kind: MutationKind::Tool,
        prose: Vec::new(),
        params: Vec::new(),
        tools: ToolDiff {
            added: vec!["new_tool".into()],
            removed: Vec::new(),
        },
        filter: Vec::new(),
        create_filter: None,
        rationale: String::new(),
    };
    assert!(!with_tools_added.is_empty());

    let with_tools_removed = MutationDiff {
        kind: MutationKind::Tool,
        prose: Vec::new(),
        params: Vec::new(),
        tools: ToolDiff {
            added: Vec::new(),
            removed: vec!["old_tool".into()],
        },
        filter: Vec::new(),
        create_filter: None,
        rationale: String::new(),
    };
    assert!(!with_tools_removed.is_empty());
}

#[test]
fn empty_mutation_kind_discriminant() {
    let diff = empty_mutation();
    assert_eq!(diff.kind, MutationKind::Prose);
}

#[test]
fn mutation_kind_serializes_as_snake_case() {
    assert_eq!(serde_json::to_string(&MutationKind::Prose).unwrap(), "\"prose\"");
    assert_eq!(serde_json::to_string(&MutationKind::Param).unwrap(), "\"param\"");
    assert_eq!(serde_json::to_string(&MutationKind::Tool).unwrap(), "\"tool\"");
}
