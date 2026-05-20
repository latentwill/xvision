use xvision_memory::types::{MemoryMode, Namespace};

#[test]
fn memory_mode_serde_round_trip() {
    for mode in [MemoryMode::Off, MemoryMode::Global, MemoryMode::AgentScoped] {
        let json = serde_json::to_string(&mode).unwrap();
        let back: MemoryMode = serde_json::from_str(&json).unwrap();
        assert_eq!(mode, back);
    }
    assert_eq!(serde_json::to_string(&MemoryMode::Off).unwrap(), "\"off\"");
    assert_eq!(serde_json::to_string(&MemoryMode::AgentScoped).unwrap(), "\"agent_scoped\"");
}

#[test]
fn namespace_for_mode_uses_agent_id() {
    assert_eq!(Namespace::for_mode(MemoryMode::Off, "01HZTEST").as_str(), None::<&str>.unwrap_or_default());
    assert_eq!(Namespace::for_mode(MemoryMode::Global, "01HZTEST").as_str(), "global");
    assert_eq!(Namespace::for_mode(MemoryMode::AgentScoped, "01HZTEST").as_str(), "agent:01HZTEST");
}
