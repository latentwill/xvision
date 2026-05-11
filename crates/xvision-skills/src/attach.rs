use crate::Skill;
use xvision_engine::bundle::StrategyBundle;

/// Attach a skill to a named slot of a strategy bundle. Replaces the slot's
/// prompt with the skill body, sets `model_requirement`, and unions the
/// skill's `allowed_tools` into the slot's existing tool set.
///
/// Errors if `slot_role` is unknown or if the slot is currently empty —
/// authors must fill the slot before they can layer a skill onto it.
pub fn attach_skill_to_agent(
    bundle: &mut StrategyBundle,
    slot_role: &str,
    skill: &Skill,
) -> anyhow::Result<()> {
    let slot = match slot_role {
        "regime" => bundle.regime_slot.as_mut(),
        "intern" => bundle.intern_slot.as_mut(),
        "trader" => bundle.trader_slot.as_mut(),
        other => anyhow::bail!("unknown slot role: {other} (must be regime, intern, or trader)"),
    };
    let slot =
        slot.ok_or_else(|| anyhow::anyhow!("slot '{slot_role}' is empty — fill it before attaching"))?;
    slot.prompt = skill.body.clone();
    slot.model_requirement = skill.model_requirement.clone();
    let mut tools = slot.allowed_tools.clone();
    for t in &skill.allowed_tools {
        if !tools.contains(t) {
            tools.push(t.clone());
        }
    }
    slot.allowed_tools = tools;
    Ok(())
}
