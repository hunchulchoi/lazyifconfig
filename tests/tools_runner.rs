use std::collections::BTreeMap;
use std::time::Duration;

use lazyifconfig::tools::{port_check, ToolInput};
use lazyifconfig::tools::{ToolId, ToolResult, ToolResultSection};
use tokio::net::TcpListener;

fn input(values: &[(&str, &str)]) -> ToolInput {
    ToolInput {
        values: values
            .iter()
            .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
            .collect::<BTreeMap<_, _>>(),
    }
}

#[tokio::test]
async fn port_check_reports_open_local_listener() {
    let listener = match TcpListener::bind("127.0.0.1:0").await {
        Ok(listener) => listener,
        Err(err) if err.kind() == std::io::ErrorKind::PermissionDenied => return,
        Err(err) => panic!("failed to bind local listener: {err}"),
    };
    let port = listener.local_addr().unwrap().port().to_string();
    let accept_task = tokio::spawn(async move {
        let _ = listener.accept().await;
    });

    let result = port_check::run(
        input(&[("host", "127.0.0.1"), ("port", &port)]),
        Duration::from_secs(1),
    )
    .await
    .unwrap();

    assert_eq!(result.title, "Port Check");
    assert!(result.sections.iter().any(|section| {
        section.label == "Status" && section.lines.iter().any(|line| line.contains("OPEN"))
    }));
    assert!(result
        .raw_output
        .contains("lazyifconfig tools port-check 127.0.0.1"));

    let _ = accept_task.await;
}

#[tokio::test]
async fn port_check_rejects_invalid_port() {
    let err = port_check::run(
        input(&[("host", "127.0.0.1"), ("port", "not-a-port")]),
        Duration::from_millis(50),
    )
    .await
    .unwrap_err();

    assert!(err.contains("Port must be a number"));
}

#[test]
fn dns_command_candidates_prefer_dig() {
    let candidates = lazyifconfig::tools::dns::command_candidates("example.com");

    assert_eq!(candidates[0].program, "dig");
    assert_eq!(candidates[0].args, vec!["example.com"]);
    assert_eq!(candidates[1].program, "host");
    assert_eq!(candidates[2].program, "nslookup");
}

#[test]
fn ping_command_uses_small_count_per_platform() {
    let mac = lazyifconfig::tools::ping::command_spec_for_os("macos", "8.8.8.8");
    let linux = lazyifconfig::tools::ping::command_spec_for_os("linux", "8.8.8.8");
    let windows = lazyifconfig::tools::ping::command_spec_for_os("windows", "8.8.8.8");

    assert_eq!(mac.program, "ping");
    assert_eq!(mac.args, vec!["-c", "4", "8.8.8.8"]);
    assert_eq!(linux.program, "ping");
    assert_eq!(linux.args, vec!["-c", "4", "8.8.8.8"]);
    assert_eq!(windows.program, "ping");
    assert_eq!(windows.args, vec!["-n", "4", "8.8.8.8"]);
}

#[test]
fn whois_command_uses_standard_target_lookup() {
    let spec = lazyifconfig::tools::whois::command_spec("github.com");

    assert_eq!(spec.program, "whois");
    assert_eq!(spec.args, vec!["github.com"]);
}

#[test]
fn ip_info_reverse_dns_candidates_prefer_dig_ptr() {
    let candidates = lazyifconfig::tools::ip_info::reverse_dns_command_candidates("8.8.8.8");

    assert_eq!(candidates[0].program, "dig");
    assert_eq!(candidates[0].args, vec!["-x", "8.8.8.8", "+short"]);
    assert_eq!(candidates[1].program, "host");
}

#[test]
fn tls_command_uses_sni_for_host_and_port() {
    let spec = lazyifconfig::tools::tls::command_spec("github.com", 443);

    assert_eq!(spec.program, "openssl");
    assert_eq!(
        spec.args,
        vec![
            "s_client",
            "-connect",
            "github.com:443",
            "-servername",
            "github.com",
            "-showcerts",
        ]
    );
}

#[test]
fn traceroute_command_uses_platform_specific_flags() {
    let mac = lazyifconfig::tools::traceroute::command_spec_for_os("macos", "8.8.8.8");
    let linux = lazyifconfig::tools::traceroute::command_spec_for_os("linux", "8.8.8.8");
    let windows = lazyifconfig::tools::traceroute::command_spec_for_os("windows", "8.8.8.8");

    assert_eq!(mac.program, "traceroute");
    assert_eq!(mac.args, vec!["-m", "8", "8.8.8.8"]);
    assert_eq!(linux.program, "traceroute");
    assert_eq!(linux.args, vec!["-m", "8", "-w", "1", "8.8.8.8"]);
    assert_eq!(windows.program, "tracert");
    assert_eq!(windows.args, vec!["-h", "8", "8.8.8.8"]);
}

#[test]
fn cli_tool_ids_cover_all_runnable_tools() {
    assert_eq!(
        lazyifconfig::tools::tool_id_from_cli_name("dns"),
        Some(ToolId::DnsLookup)
    );
    assert_eq!(
        lazyifconfig::tools::tool_id_from_cli_name("whois"),
        Some(ToolId::WhoisLookup)
    );
    assert_eq!(
        lazyifconfig::tools::tool_id_from_cli_name("ip-info"),
        Some(ToolId::IpInformation)
    );
    assert_eq!(
        lazyifconfig::tools::tool_id_from_cli_name("port-check"),
        Some(ToolId::PortCheck)
    );
    assert_eq!(
        lazyifconfig::tools::tool_id_from_cli_name("tls"),
        Some(ToolId::TlsInspector)
    );
    assert_eq!(
        lazyifconfig::tools::tool_id_from_cli_name("ping"),
        Some(ToolId::Ping)
    );
    assert_eq!(
        lazyifconfig::tools::tool_id_from_cli_name("traceroute"),
        Some(ToolId::Traceroute)
    );
    assert_eq!(lazyifconfig::tools::tool_id_from_cli_name("unknown"), None);
}

#[test]
fn cli_argument_mapping_matches_tool_fields() {
    let dns =
        lazyifconfig::tools::tool_input_from_cli_args(ToolId::DnsLookup, &["example.com"]).unwrap();
    assert_eq!(dns.get("target"), Some("example.com"));

    let port =
        lazyifconfig::tools::tool_input_from_cli_args(ToolId::PortCheck, &["github.com", "443"])
            .unwrap();
    assert_eq!(port.get("host"), Some("github.com"));
    assert_eq!(port.get("port"), Some("443"));

    let tls =
        lazyifconfig::tools::tool_input_from_cli_args(ToolId::TlsInspector, &["github.com:443"])
            .unwrap();
    assert_eq!(tls.get("target"), Some("github.com:443"));
}

#[test]
fn cli_argument_mapping_rejects_wrong_arity() {
    let err = lazyifconfig::tools::tool_input_from_cli_args(ToolId::PortCheck, &["github.com"])
        .unwrap_err();
    assert!(err.contains("Usage"));
    assert!(err.contains("port-check <host> <port>"));
}

#[test]
fn plain_text_formatter_includes_sections_and_raw_output() {
    let rendered = lazyifconfig::tools::format_tool_result_plaintext(&ToolResult {
        title: "Ping".to_string(),
        sections: vec![
            ToolResultSection {
                label: "Summary".to_string(),
                lines: vec!["Target: 8.8.8.8".to_string()],
            },
            ToolResultSection {
                label: "Diagnostics".to_string(),
                lines: vec!["ok".to_string()],
            },
        ],
        raw_output: "$ ping -c 4 8.8.8.8\n...".to_string(),
    });

    assert!(rendered.contains("Ping"));
    assert!(rendered.contains("[Summary]"));
    assert!(rendered.contains("Target: 8.8.8.8"));
    assert!(rendered.contains("[Raw Output]"));
}
