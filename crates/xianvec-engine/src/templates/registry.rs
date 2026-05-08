use std::sync::OnceLock;

use crate::templates::Template;

static REGISTRY: OnceLock<Vec<Box<dyn Template>>> = OnceLock::new();

fn registry() -> &'static [Box<dyn Template>] {
    REGISTRY.get_or_init(|| {
        // Templates added via Task 9.
        vec![]
    })
}

pub fn get(name: &str) -> Option<&'static dyn Template> {
    registry().iter().find(|t| t.name() == name).map(|t| t.as_ref())
}

pub fn list_template_names() -> Vec<String> {
    registry().iter().map(|t| t.name().to_string()).collect()
}
