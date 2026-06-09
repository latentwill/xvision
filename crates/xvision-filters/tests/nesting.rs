//! Tests for ConditionItem / ConditionGroup nesting (B-W1).
//!
//! These tests were written BEFORE implementation — they define the
//! required behaviour and should fail until types.rs is updated.

use xvision_filters::{Condition, ConditionGroup, ConditionItem, ConditionTree, Operand, Operator};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_cond() -> Condition {
    Condition {
        lhs: Operand::Numeric(1.0),
        op: Operator::Gt,
        rhs: Operand::Numeric(0.0),
    }
}

fn make_cond_with_rhs(rhs: f64) -> Condition {
    Condition {
        lhs: Operand::Numeric(0.0),
        op: Operator::Gt,
        rhs: Operand::Numeric(rhs),
    }
}

// ---------------------------------------------------------------------------
// ConditionItem serde round-trips
// ---------------------------------------------------------------------------

#[test]
fn condition_item_leaf_round_trips_json() {
    let json = r#"{"lhs":1.0,"op":">","rhs":0.0}"#;
    let item: ConditionItem = serde_json::from_str(json).unwrap();
    assert!(matches!(item, ConditionItem::Leaf(_)), "should be Leaf");
    let re = serde_json::to_string(&item).unwrap();
    assert_eq!(json, re);
}

#[test]
fn nested_group_does_not_misparse_as_leaf() {
    // Guard against untagged serde footgun: a Group key must not be
    // mistakenly deserialized as a Leaf.
    let json = r#"{"all":[{"lhs":25.0,"op":"<","rhs":0.0}]}"#;
    let item: ConditionItem = serde_json::from_str(json).unwrap();
    assert!(
        matches!(item, ConditionItem::Group(_)),
        "should be Group, not Leaf"
    );
}

#[test]
fn condition_item_group_round_trips_json() {
    let json = r#"{"all":[{"lhs":25.0,"op":"<","rhs":0.0}]}"#;
    let item: ConditionItem = serde_json::from_str(json).unwrap();
    let re = serde_json::to_string(&item).unwrap();
    assert_eq!(json, re);
}

// ---------------------------------------------------------------------------
// ConditionTree with nested items
// ---------------------------------------------------------------------------

#[test]
fn nested_filter_parses() {
    // All([leaf_adx, Any([leaf_mfi_lt, leaf_mfi_gt])])
    let json = r#"{"all":[{"lhs":25.0,"op":"<","rhs":0.0},{"any":[{"lhs":20.0,"op":"<","rhs":0.0},{"lhs":80.0,"op":">","rhs":0.0}]}]}"#;
    let tree: ConditionTree = serde_json::from_str(json).unwrap();
    assert_eq!(tree.leaf_count(), 3);
    let leaves = tree.leaves_dfs();
    assert_eq!(leaves.len(), 3);
}

#[test]
fn flat_filter_wire_format_unchanged() {
    let json = r#"{"all":[{"lhs":0.0,"op":">","rhs":0.0}]}"#;
    let tree: ConditionTree = serde_json::from_str(json).unwrap();
    let re = serde_json::to_string(&tree).unwrap();
    assert_eq!(json, re);
}

// ---------------------------------------------------------------------------
// leaf_count
// ---------------------------------------------------------------------------

#[test]
fn leaf_count_flat() {
    let tree = ConditionTree::All(vec![
        ConditionItem::Leaf(make_cond()),
        ConditionItem::Leaf(make_cond()),
    ]);
    assert_eq!(tree.leaf_count(), 2);
}

#[test]
fn leaf_count_nested() {
    let inner = ConditionGroup::Any(vec![make_cond(), make_cond()]);
    let tree = ConditionTree::All(vec![
        ConditionItem::Leaf(make_cond()),
        ConditionItem::Group(inner),
    ]);
    assert_eq!(tree.leaf_count(), 3);
}

// ---------------------------------------------------------------------------
// leaves_dfs order
// ---------------------------------------------------------------------------

#[test]
fn leaves_dfs_order_nested() {
    // All([c0, Any([c1, c2])]) → DFS order: c0 first, then c1, c2
    let c0 = make_cond_with_rhs(0.0);
    let c1 = make_cond_with_rhs(1.0);
    let c2 = make_cond_with_rhs(2.0);
    let inner = ConditionGroup::Any(vec![c1.clone(), c2.clone()]);
    let tree = ConditionTree::All(vec![ConditionItem::Leaf(c0.clone()), ConditionItem::Group(inner)]);
    let leaves = tree.leaves_dfs();
    assert_eq!(leaves.len(), 3);
    // DFS: c0 → c1 → c2 (rhs values 0, 1, 2)
    assert_eq!(leaves[0].rhs, Operand::Numeric(0.0));
    assert_eq!(leaves[1].rhs, Operand::Numeric(1.0));
    assert_eq!(leaves[2].rhs, Operand::Numeric(2.0));
}

// ---------------------------------------------------------------------------
// ConditionGroup helpers
// ---------------------------------------------------------------------------

#[test]
fn condition_group_variant_name() {
    let g_all = ConditionGroup::All(vec![]);
    let g_any = ConditionGroup::Any(vec![]);
    assert_eq!(g_all.variant_name(), "all");
    assert_eq!(g_any.variant_name(), "any");
}

#[test]
fn condition_group_conditions_accessor() {
    let c = make_cond();
    let g = ConditionGroup::All(vec![c.clone()]);
    assert_eq!(g.conditions(), &[c]);
}

#[test]
fn condition_group_is_empty() {
    assert!(ConditionGroup::All(vec![]).is_empty());
    let g = ConditionGroup::Any(vec![make_cond()]);
    assert!(!g.is_empty());
}
