//! Semantic validation of `Filter`. Implements the 10 rules from
//! `docs/superpowers/specs/2026-05-21-filter-v1.md` §Validation rules.
//!
//! Each error carries a JSON-pointer field path (e.g.
//! `/conditions/all/2/rhs`) and a stable `E_FILTER_*` code via
//! `ValidationError::code()` / `field_path()`.
//!
//! ## Per-indicator numeric bounds (rule 5)
//!
//! | Indicator | Numeric operand constraint |
//! |---|---|
//! | `rsi_n` | value ∈ [0.0, 100.0] |
//! | `atr_pct_n` | value > 0.0 |
//! | `ema_n`, `sma_n`, `atr_n`, `close` | no upper-bound check in v1; finite required |
//!
//! Range bounds carry the same constraint per element when compared
//! against the corresponding indicator (e.g. `rsi_14 between [50, 70]`
//! — both endpoints must lie in `[0, 100]`).

use crate::errors::ValidationError;
use crate::types::{Condition, ConditionTree, Filter, IndicatorName, IndicatorRef, Operand, Operator};

/// Validate a `Filter`. Returns on the **first** rule violation; rules
/// are checked in spec-table order so the most semantically-meaningful
/// failure surfaces first.
pub fn validate(filter: &Filter) -> Result<(), ValidationError> {
    // Rule 9 — asset scope must name at least one symbol. Multi-asset filters
    // are valid; the executor evaluates this same predicate per active asset.
    validate_asset_scope(filter)?;

    // Rule 8 — max wakeups per day.
    validate_wakeup_cap(filter)?;

    // Rule 7 — cooldown is non-negative (enforced at type level; this
    // is a belt-and-braces check kept here so the rule code is
    // exercisable from in-memory tests if a future refactor relaxes the
    // type).
    //
    // (u32 cannot represent negatives, so this is a no-op today.)
    validate_cooldown(filter)?;

    // Rule 10 — non-empty condition tree.
    validate_condition_tree_non_empty(&filter.conditions)?;

    validate_fire_metadata(filter)?;

    // Per-condition rules: 1, 2, 3, 4, 5, 6.
    let variant = filter.conditions.variant_name();
    for (idx, cond) in filter.conditions.conditions().iter().enumerate() {
        let base = format!("/conditions/{}/{}", variant, idx);
        validate_condition(cond, &base)?;
    }

    Ok(())
}

fn validate_fire_metadata(filter: &Filter) -> Result<(), ValidationError> {
    let Some(fire) = &filter.fire else {
        return Ok(());
    };
    if fire.reason.trim().is_empty() {
        return Err(ValidationError::EmptyTree {
            path: "/fire/reason".to_string(),
            detail: "fire.reason must not be empty".to_string(),
        });
    }
    if !fire.priority.is_finite() || !(0.0..=1.0).contains(&fire.priority) {
        return Err(ValidationError::NumericBounds {
            path: "/fire/priority".to_string(),
            detail: format!(
                "fire.priority must be finite and in [0, 1]; got {}",
                fire.priority
            ),
        });
    }
    for (idx, indicator) in fire.context.iter().enumerate() {
        validate_indicator_ref(indicator, &format!("/fire/context/{}", idx))?;
    }
    Ok(())
}

fn validate_asset_scope(filter: &Filter) -> Result<(), ValidationError> {
    if filter.asset_scope.is_empty() {
        return Err(ValidationError::AssetScope {
            path: "/asset_scope".to_string(),
            detail: "asset_scope must include at least one symbol".to_string(),
        });
    }
    Ok(())
}

fn validate_wakeup_cap(filter: &Filter) -> Result<(), ValidationError> {
    if let Some(n) = filter.max_wakeups_per_day {
        if !(1..=1440).contains(&n) {
            return Err(ValidationError::WakeupCap {
                path: "/max_wakeups_per_day".to_string(),
                detail: format!("max_wakeups_per_day must be in [1, 1440]; got {}", n),
            });
        }
    }
    Ok(())
}

fn validate_cooldown(_filter: &Filter) -> Result<(), ValidationError> {
    // u32 prevents negative values at the type level. If a future
    // refactor relaxes the type, return:
    //
    // ValidationError::CooldownNeg {
    //     path: "/cooldown_bars".to_string(),
    //     detail: format!("cooldown_bars must be >= 0; got {}", n),
    // }
    Ok(())
}

fn validate_condition_tree_non_empty(tree: &ConditionTree) -> Result<(), ValidationError> {
    let variant = tree.variant_name();
    if tree.conditions().is_empty() {
        return Err(ValidationError::EmptyTree {
            path: format!("/conditions/{}", variant),
            detail: format!("condition tree '{}' must contain at least one condition", variant),
        });
    }
    Ok(())
}

fn validate_condition(cond: &Condition, base: &str) -> Result<(), ValidationError> {
    // Rule 1 + 6: indicators inside operands must be in catalog and
    // not reference future bars.
    validate_operand_indicators(&cond.lhs, &format!("{}/lhs", base))?;
    validate_operand_indicators(&cond.rhs, &format!("{}/rhs", base))?;

    // Rule 2: unknown operator. The closed `Operator` enum means a
    // value can only be one of the 8 cataloged variants; we still
    // expose this rule via a function (returning Ok) so the validator's
    // public contract carries the rule. Frontend matchers can rely on
    // the code existing.
    validate_operator_in_catalog(&cond.op, &format!("{}/op", base))?;

    // Rule 3: operand-type contract per operator.
    validate_operand_types(cond, base)?;

    // Rule 4: range ordering.
    if let Operand::Range(lo, hi) = &cond.lhs {
        validate_range(*lo, *hi, &format!("{}/lhs", base))?;
    }
    if let Operand::Range(lo, hi) = &cond.rhs {
        validate_range(*lo, *hi, &format!("{}/rhs", base))?;
    }

    // Rule 5: per-indicator numeric bounds. Run after type-contract so
    // we know operands sit in their expected slots.
    validate_numeric_bounds(cond, base)?;

    Ok(())
}

fn validate_operand_indicators(operand: &Operand, path: &str) -> Result<(), ValidationError> {
    if let Operand::Indicator(ind) = operand {
        validate_indicator_ref(ind, path)?;
    }
    Ok(())
}

fn validate_indicator_ref(ind: &IndicatorRef, path: &str) -> Result<(), ValidationError> {
    // Rule 6: future-bar leak. `bar_offset > 0` references a future
    // bar. The DSL parser already rejects `+N` syntax; this catches
    // in-memory constructions used by v1.5 plugins or hand-built
    // structs in tests.
    if let Some(off) = ind.bar_offset {
        if off > 0 {
            return Err(ValidationError::FutureLeak {
                path: path.to_string(),
                detail: format!(
                    "indicator references future bar (+{}); v1 has no future-bar syntax",
                    off
                ),
            });
        }
    }

    // Rule 1: indicator name + period in catalog.
    match ind.name {
        name if !name.has_period() => {
            if ind.period.is_some() {
                return Err(ValidationError::UnknownIndicator {
                    path: path.to_string(),
                    detail: format!("indicator '{}' must not carry a period", name.dsl_prefix()),
                });
            }
        }
        name => {
            let period = match ind.period {
                Some(p) => p,
                None => {
                    return Err(ValidationError::UnknownIndicator {
                        path: path.to_string(),
                        detail: format!("indicator '{}' requires a period", name.dsl_prefix()),
                    });
                }
            };
            let (lo, hi) = name.period_bounds().expect("non-close has bounds");
            if !(lo..=hi).contains(&period) {
                return Err(ValidationError::UnknownIndicator {
                    path: path.to_string(),
                    detail: format!(
                        "indicator '{}_{}' period out of range [{}, {}]",
                        name.dsl_prefix(),
                        period,
                        lo,
                        hi
                    ),
                });
            }
        }
    }
    Ok(())
}

fn validate_operator_in_catalog(_op: &Operator, _path: &str) -> Result<(), ValidationError> {
    // The `Operator` enum is closed; deserialization already rejects
    // anything outside the catalog. This function exists so the rule
    // is reachable from the validator surface — see the
    // `unknown_operator` test in `tests/validate_codes.rs`, which
    // exercises the parser-layer rejection that maps to the same
    // wire code via `ParseError::UnknownOperator`.
    Ok(())
}

fn validate_operand_types(cond: &Condition, base: &str) -> Result<(), ValidationError> {
    let lhs_path = format!("{}/lhs", base);
    let rhs_path = format!("{}/rhs", base);

    match cond.op {
        // Comparison operators: lhs Indicator, rhs Indicator | Numeric.
        // Range disallowed on either side.
        Operator::Gt
        | Operator::Lt
        | Operator::Gte
        | Operator::Lte
        | Operator::Eq
        | Operator::AboveFor(_)
        | Operator::BelowFor(_)
        | Operator::WithinPct(_) => {
            require_indicator(&cond.lhs, &lhs_path, cond.op)?;
            match &cond.rhs {
                Operand::Indicator(_) | Operand::Numeric(_) => {}
                Operand::Range(_, _) => {
                    return Err(ValidationError::OperandType {
                        path: rhs_path,
                        detail: format!(
                            "operator '{}' rhs must be indicator or numeric, got range",
                            cond.op.dsl_token()
                        ),
                    });
                }
            }
        }
        // `crosses_*`: both sides indicator.
        Operator::CrossesAbove
        | Operator::CrossesBelow
        | Operator::CrossedAbove(_)
        | Operator::CrossedBelow(_) => {
            require_indicator(&cond.lhs, &lhs_path, cond.op)?;
            require_indicator(&cond.rhs, &rhs_path, cond.op)?;
        }
        // Transform operators compare the transformed LHS against a
        // numeric threshold.
        Operator::SlopeGt(_) | Operator::SlopeLt(_) | Operator::ZscoreGt(_) | Operator::ZscoreLt(_) => {
            require_indicator(&cond.lhs, &lhs_path, cond.op)?;
            if !matches!(cond.rhs, Operand::Numeric(_)) {
                return Err(ValidationError::OperandType {
                    path: rhs_path,
                    detail: format!(
                        "operator '{}' requires rhs to be numeric, got {}",
                        cond.op.dsl_token(),
                        cond.rhs.kind_name()
                    ),
                });
            }
        }
        // `between`: lhs indicator, rhs range.
        Operator::Between => {
            require_indicator(&cond.lhs, &lhs_path, cond.op)?;
            if !matches!(cond.rhs, Operand::Range(_, _)) {
                return Err(ValidationError::OperandType {
                    path: rhs_path,
                    detail: format!(
                        "operator 'between' requires rhs to be a range, got {}",
                        cond.rhs.kind_name()
                    ),
                });
            }
        }
    }
    Ok(())
}

fn require_indicator(operand: &Operand, path: &str, op: Operator) -> Result<(), ValidationError> {
    if matches!(operand, Operand::Indicator(_)) {
        Ok(())
    } else {
        Err(ValidationError::OperandType {
            path: path.to_string(),
            detail: format!(
                "operator '{}' requires this operand to be an indicator, got {}",
                op.dsl_token(),
                operand.kind_name()
            ),
        })
    }
}

fn validate_range(lo: f64, hi: f64, path: &str) -> Result<(), ValidationError> {
    if !lo.is_finite() || !hi.is_finite() {
        return Err(ValidationError::RangeOrder {
            path: path.to_string(),
            detail: format!("range endpoints must be finite; got [{}, {}]", lo, hi),
        });
    }
    if lo >= hi {
        return Err(ValidationError::RangeOrder {
            path: path.to_string(),
            detail: format!("range requires lo < hi; got [{}, {}]", lo, hi),
        });
    }
    Ok(())
}

fn validate_numeric_bounds(cond: &Condition, base: &str) -> Result<(), ValidationError> {
    if matches!(
        cond.op,
        Operator::SlopeGt(_) | Operator::SlopeLt(_) | Operator::ZscoreGt(_) | Operator::ZscoreLt(_)
    ) {
        return Ok(());
    }
    // Determine the indicator the numeric value is being compared
    // against, then enforce per-indicator bounds.
    let indicator = match &cond.lhs {
        Operand::Indicator(ind) => Some(ind),
        _ => None,
    };
    if let Some(ind) = indicator {
        match &cond.rhs {
            Operand::Numeric(v) => {
                check_numeric_for_indicator(ind, *v, &format!("{}/rhs", base))?;
            }
            Operand::Range(lo, hi) => {
                check_numeric_for_indicator(ind, *lo, &format!("{}/rhs", base))?;
                check_numeric_for_indicator(ind, *hi, &format!("{}/rhs", base))?;
            }
            Operand::Indicator(_) => {}
        }
    }
    Ok(())
}

fn check_numeric_for_indicator(ind: &IndicatorRef, value: f64, path: &str) -> Result<(), ValidationError> {
    if !value.is_finite() {
        return Err(ValidationError::NumericBounds {
            path: path.to_string(),
            detail: format!(
                "numeric operand must be finite when compared against '{}'; got {}",
                ind.to_dsl(),
                value
            ),
        });
    }
    match ind.name {
        IndicatorName::Rsi
        | IndicatorName::Adx
        | IndicatorName::DiPlus
        | IndicatorName::DiMinus
        | IndicatorName::StochK
        | IndicatorName::StochD
        | IndicatorName::StochRsiK
        | IndicatorName::StochRsiD
        | IndicatorName::Mfi
            if !(0.0..=100.0).contains(&value) =>
        {
            Err(ValidationError::NumericBounds {
                path: path.to_string(),
                detail: format!(
                    "{} threshold must be in [0, 100]; got {} for '{}'",
                    ind.name.dsl_prefix(),
                    value,
                    ind.to_dsl()
                ),
            })
        }
        IndicatorName::BbPercentB if !(-5.0..=5.0).contains(&value) => Err(ValidationError::NumericBounds {
            path: path.to_string(),
            detail: format!(
                "bb_pct_b threshold must be in [-5, 5]; got {} for '{}'",
                value,
                ind.to_dsl()
            ),
        }),
        IndicatorName::AtrPct if value <= 0.0 => Err(ValidationError::NumericBounds {
            path: path.to_string(),
            detail: format!(
                "atr_pct threshold must be > 0; got {} for '{}'",
                value,
                ind.to_dsl()
            ),
        }),
        IndicatorName::WilliamsR if !(-100.0..=0.0).contains(&value) => Err(ValidationError::NumericBounds {
            path: path.to_string(),
            detail: format!(
                "williams_r threshold must be in [-100, 0]; got {} for '{}'",
                value,
                ind.to_dsl()
            ),
        }),
        IndicatorName::GapUp | IndicatorName::GapDown if !(0.0..=1.0).contains(&value) => {
            Err(ValidationError::NumericBounds {
                path: path.to_string(),
                detail: format!(
                    "{} threshold must be in [0, 1]; got {} for '{}'",
                    ind.name.dsl_prefix(),
                    value,
                    ind.to_dsl()
                ),
            })
        }
        // No upper-bound check in v1 for ema/sma/atr/close (or for
        // values that satisfy the rsi/atr_pct bound).
        _ => Ok(()),
    }
}
