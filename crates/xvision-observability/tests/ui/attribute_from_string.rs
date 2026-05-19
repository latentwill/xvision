use xvision_observability::recorder::Attribute;

fn main() {
    let owned = String::from("raw-payload-string-must-not-compile");
    let _: Attribute = owned.into();
}
