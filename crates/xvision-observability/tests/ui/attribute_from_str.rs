use xvision_observability::recorder::Attribute;

fn main() {
    let _: Attribute = "raw-payload-string-must-not-compile".into();
}
