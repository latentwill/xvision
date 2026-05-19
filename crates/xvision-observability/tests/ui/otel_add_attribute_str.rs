use xvision_observability::{otel_add_attribute, otel_attr};

fn main() {
    otel_add_attribute(todo!(), otel_attr::RUN_ID, "raw-string-not-allowed");
}
