use xvision_observability::{otel_add_attribute, otel_attr};

fn main() {
    let payload = String::from("raw-string-not-allowed");
    otel_add_attribute(todo!(), otel_attr::RUN_ID, payload);
}
