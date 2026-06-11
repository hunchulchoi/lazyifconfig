use lazyifconfig::tools::{ToolAvailability, ToolId, ToolRegistry};

#[test]
fn registry_lists_first_slice_tools_in_ui_order() {
    let registry = ToolRegistry::default();
    let ids: Vec<ToolId> = registry.definitions().iter().map(|tool| tool.id).collect();

    assert_eq!(
        ids,
        vec![
            ToolId::DnsLookup,
            ToolId::WhoisLookup,
            ToolId::IpInformation,
            ToolId::PortCheck,
            ToolId::TlsInspector,
            ToolId::Ping,
            ToolId::Traceroute,
        ]
    );
}

#[test]
fn registry_marks_all_tools_runnable() {
    let registry = ToolRegistry::default();

    assert_eq!(
        registry.definition(ToolId::DnsLookup).unwrap().availability,
        ToolAvailability::Runnable
    );
    assert_eq!(
        registry.definition(ToolId::PortCheck).unwrap().availability,
        ToolAvailability::Runnable
    );
    assert_eq!(
        registry.definition(ToolId::Ping).unwrap().availability,
        ToolAvailability::Runnable
    );
    assert_eq!(
        registry
            .definition(ToolId::WhoisLookup)
            .unwrap()
            .availability,
        ToolAvailability::Runnable
    );
    assert_eq!(
        registry
            .definition(ToolId::IpInformation)
            .unwrap()
            .availability,
        ToolAvailability::Runnable
    );
    assert_eq!(
        registry
            .definition(ToolId::TlsInspector)
            .unwrap()
            .availability,
        ToolAvailability::Runnable
    );
    assert_eq!(
        registry
            .definition(ToolId::Traceroute)
            .unwrap()
            .availability,
        ToolAvailability::Runnable
    );
}
