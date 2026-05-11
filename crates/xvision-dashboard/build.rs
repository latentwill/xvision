// Ensures cargo treats `static/` (where Vite writes the SPA build) as part of
// the crate's fingerprint. Without this, rust-embed's derive macro can be
// compiled once against an empty `static/` (e.g. during cargo-chef's `cook`
// stage in the Docker build) and cargo won't re-derive it when the real
// assets arrive later, producing a binary that knows about `index.html` but
// 404s every `/assets/*` request.
fn main() {
    println!("cargo:rerun-if-changed=static");
}
