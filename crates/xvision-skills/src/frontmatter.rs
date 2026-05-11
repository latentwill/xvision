use crate::SkillError;

pub fn split(markdown: &str) -> Result<(&str, &str), SkillError> {
    let trimmed = markdown.trim_start();
    let after_open = trimmed
        .strip_prefix("---\n")
        .or_else(|| trimmed.strip_prefix("---\r\n"))
        .ok_or(SkillError::MissingFrontmatter)?;
    let close_idx = after_open
        .find("\n---\n")
        .or_else(|| after_open.find("\r\n---\r\n"))
        .ok_or(SkillError::MissingFrontmatter)?;
    let yaml = &after_open[..close_idx];
    let body_start = close_idx + "\n---\n".len();
    let body = if body_start < after_open.len() {
        &after_open[body_start..]
    } else {
        ""
    };
    Ok((yaml, body.trim_start_matches('\n')))
}
