use crate::strategies::Strategy;

pub fn to_markdown(strategy: &Strategy) -> String {
    let m = &strategy.manifest;
    let mut out = String::with_capacity(512);

    out.push_str(&format!("# Strategy: {}\n\n", m.display_name));
    out.push_str(&format!("**Summary**: {}\n", m.plain_summary));
    out.push_str(&format!("**Template**: {}\n", m.template));
    out.push_str(&format!("**Decision cadence**: {} min\n", m.decision_cadence_minutes));

    if !m.asset_universe.is_empty() {
        out.push_str(&format!("**Asset universe**: {}\n", m.asset_universe.join(", ")));
    }

    out.push('\n');
    write_agents_section(strategy, &mut out);
    write_params_section(strategy, &mut out);
    out
}

fn write_agents_section(strategy: &Strategy, out: &mut String) {
    out.push_str("## Agents\n\n");
    if !strategy.agents.is_empty() {
        for a in &strategy.agents {
            out.push_str(&format!("- **{}** (id: {})\n", a.role, a.agent_id));
        }
    } else {
        for (label, slot) in [
            ("regime", strategy.regime_slot.as_ref()),
            ("intern", strategy.intern_slot.as_ref()),
            ("trader", strategy.trader_slot.as_ref()),
        ] {
            if slot.is_some() {
                out.push_str(&format!("- **{}** (legacy slot)\n", label));
            }
        }
        if strategy.regime_slot.is_none()
            && strategy.intern_slot.is_none()
            && strategy.trader_slot.is_none()
        {
            out.push_str("- (none)\n");
        }
    }
    out.push('\n');
}

fn write_params_section(strategy: &Strategy, out: &mut String) {
    if strategy.mechanical_params.is_null() {
        return;
    }
    if let Some(obj) = strategy.mechanical_params.as_object() {
        if obj.is_empty() {
            return;
        }
    } else {
        return;
    }
    out.push_str("## Mechanical Parameters\n\n");
    out.push_str(&strategy.mechanical_params.to_string());
    out.push('\n');
}
