// crates/xvision-engine/src/nanochat/label_config.rs

use serde_json::Value;

/// One cycle-like row passed to the evaluator.
#[derive(Debug, Clone)]
pub struct CycleRow {
    pub pnl: Option<f64>,
    pub drawdown_pct: Option<f64>,
    pub win_rate: Option<f64>,
}

/// Parse and validate a `label_config` JSON string against the closed field
/// set {pnl, drawdown_pct, win_rate} and operator set {$gt, $lt, $gte, $lte, $eq}.
/// Returns `Err` if the JSON shape is wrong or an unknown field/op is used.
pub fn validate_label_config(json: &str) -> Result<LabelConfig, String> {
    let v: Value = serde_json::from_str(json)
        .map_err(|e| format!("label_config is not valid JSON: {e}"))?;
    let obj = v.as_object().ok_or("label_config must be a JSON object")?;
    let mut conditions = Vec::new();
    for (field_key, cond_val) in obj {
        let field = match field_key.as_str() {
            "pnl" => Field::Pnl,
            "drawdown_pct" => Field::DrawdownPct,
            "win_rate" => Field::WinRate,
            other => return Err(format!("unknown label_config field: {other:?}; allowed: pnl, drawdown_pct, win_rate")),
        };
        let cond_obj = cond_val.as_object().ok_or_else(|| {
            format!("label_config field {field_key:?} must be an object like {{\"$gt\": 0}}")
        })?;
        for (op_key, operand) in cond_obj {
            let op = match op_key.as_str() {
                "$gt"  => Op::Gt,
                "$lt"  => Op::Lt,
                "$gte" => Op::Gte,
                "$lte" => Op::Lte,
                "$eq"  => Op::Eq,
                other => return Err(format!("unknown operator {other:?}; allowed: $gt, $lt, $gte, $lte, $eq")),
            };
            let threshold = operand.as_f64().ok_or_else(|| {
                format!("operator {op_key:?} operand must be a number, got {operand:?}")
            })?;
            conditions.push((field, op, threshold));
        }
    }
    Ok(LabelConfig(conditions))
}

/// A validated label config ready for evaluation.
#[derive(Debug, Clone)]
pub struct LabelConfig(Vec<(Field, Op, f64)>);

/// Evaluate `config` against a slice of cycle rows.
/// Returns the rows that pass ALL conditions.
pub fn apply_label_config<'a>(config: &LabelConfig, rows: &'a [CycleRow]) -> Vec<&'a CycleRow> {
    rows.iter()
        .filter(|row| {
            config.0.iter().all(|(field, op, threshold)| {
                let val = match field {
                    Field::Pnl => row.pnl,
                    Field::DrawdownPct => row.drawdown_pct,
                    Field::WinRate => row.win_rate,
                };
                match val {
                    None => false, // null field never satisfies any condition
                    Some(v) => match op {
                        Op::Gt  => v > *threshold,
                        Op::Lt  => v < *threshold,
                        Op::Gte => v >= *threshold,
                        Op::Lte => v <= *threshold,
                        Op::Eq  => (v - threshold).abs() < f64::EPSILON,
                    },
                }
            })
        })
        .collect()
}

#[derive(Debug, Clone, Copy)]
pub enum Field { Pnl, DrawdownPct, WinRate }

#[derive(Debug, Clone, Copy)]
pub enum Op { Gt, Lt, Gte, Lte, Eq }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_label_config_parses() {
        let cfg = validate_label_config(r#"{"pnl":{"$gt":0},"drawdown_pct":{"$lt":5}}"#).unwrap();
        // Two conditions parsed.
        assert_eq!(cfg.0.len(), 2);
    }

    #[test]
    fn single_field_valid() {
        assert!(validate_label_config(r#"{"win_rate":{"$gte":0.55}}"#).is_ok());
    }

    #[test]
    fn unknown_field_rejected() {
        assert!(validate_label_config(r#"{"exit_price":{"$gt":0}}"#).is_err());
    }

    #[test]
    fn unknown_operator_rejected() {
        assert!(validate_label_config(r#"{"pnl":{"$ne":0}}"#).is_err());
    }

    #[test]
    fn non_object_top_level_rejected() {
        assert!(validate_label_config(r#"[{"pnl":{"$gt":0}}]"#).is_err());
    }

    #[test]
    fn non_object_field_value_rejected() {
        // Field value must be an object {op: num}, not a bare number.
        assert!(validate_label_config(r#"{"pnl":42}"#).is_err());
    }

    #[test]
    fn non_numeric_operand_rejected() {
        assert!(validate_label_config(r#"{"pnl":{"$gt":"zero"}}"#).is_err());
    }

    #[test]
    fn apply_filters_correctly() {
        let config = validate_label_config(r#"{"pnl":{"$gt":0},"drawdown_pct":{"$lt":5}}"#).unwrap();
        let rows = vec![
            CycleRow { pnl: Some(10.0), drawdown_pct: Some(3.0), win_rate: None },  // PASS
            CycleRow { pnl: Some(-5.0), drawdown_pct: Some(2.0), win_rate: None },  // FAIL pnl
            CycleRow { pnl: Some(20.0), drawdown_pct: Some(8.0), win_rate: None },  // FAIL drawdown
            CycleRow { pnl: None,       drawdown_pct: Some(1.0), win_rate: None },  // FAIL pnl null
        ];
        let passing = apply_label_config(&config, &rows);
        assert_eq!(passing.len(), 1);
        assert_eq!(passing[0].pnl, Some(10.0));
    }

    #[test]
    fn apply_eq_operator() {
        let config = validate_label_config(r#"{"pnl":{"$eq":0.0}}"#).unwrap();
        let rows = vec![
            CycleRow { pnl: Some(0.0), drawdown_pct: None, win_rate: None },   // PASS
            CycleRow { pnl: Some(1.0), drawdown_pct: None, win_rate: None },   // FAIL
        ];
        let passing = apply_label_config(&config, &rows);
        assert_eq!(passing.len(), 1);
    }

    #[test]
    fn null_field_value_does_not_pass_any_op() {
        let config = validate_label_config(r#"{"win_rate":{"$gt":0.5}}"#).unwrap();
        let rows = vec![
            CycleRow { pnl: None, drawdown_pct: None, win_rate: None }, // null → always fails
        ];
        assert!(apply_label_config(&config, &rows).is_empty());
    }
}
