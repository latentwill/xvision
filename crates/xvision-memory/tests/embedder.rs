use xvision_memory::embedder::{Embedder, StaticEmbedder};

#[tokio::test]
async fn static_embedder_returns_configured_vector() {
    let embedder = StaticEmbedder::new("test-embedder", vec![0.5, 0.5, 0.0]);
    let v = embedder.embed("anything").await.unwrap();
    assert_eq!(v, vec![0.5, 0.5, 0.0]);
    assert_eq!(embedder.id(), "test-embedder");
}
