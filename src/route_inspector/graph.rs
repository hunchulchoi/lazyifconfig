use petgraph::graph::Graph;

use crate::model::{RouteGraph, RouteGraphNode, RouteGraphNodeKind, RoutePathResult};

use super::vpn::is_vpn_interface_name;

pub fn build_route_graph(result: &RoutePathResult) -> RouteGraph {
    let mut graph = Graph::<RouteGraphNode, ()>::new();
    let mut ordered_nodes = Vec::new();

    let host = RouteGraphNode {
        kind: RouteGraphNodeKind::Host,
        label: "This Host".to_string(),
        detail: result.source_ip.clone(),
    };
    let mut previous = graph.add_node(host.clone());
    ordered_nodes.push(host);

    if let Some(interface) = result.interface.as_deref() {
        let node = RouteGraphNode {
            kind: RouteGraphNodeKind::Interface,
            label: interface.to_string(),
            detail: Some("Interface".to_string()),
        };
        let current = graph.add_node(node.clone());
        graph.add_edge(previous, current, ());
        previous = current;
        ordered_nodes.push(node);
    }

    if let Some(gateway) = result
        .gateway
        .as_deref()
        .filter(|gateway| should_show_gateway(gateway))
    {
        let node = RouteGraphNode {
            kind: RouteGraphNodeKind::Gateway,
            label: "Gateway".to_string(),
            detail: Some(gateway.to_string()),
        };
        let current = graph.add_node(node.clone());
        graph.add_edge(previous, current, ());
        previous = current;
        ordered_nodes.push(node);
    }

    let is_vpn = result.is_vpn
        || result
            .interface
            .as_deref()
            .is_some_and(is_vpn_interface_name);
    let transit = RouteGraphNode {
        kind: if is_vpn {
            RouteGraphNodeKind::VpnTunnel
        } else {
            RouteGraphNodeKind::Internet
        },
        label: if is_vpn {
            "VPN Tunnel".to_string()
        } else {
            "Internet".to_string()
        },
        detail: None,
    };
    let current = graph.add_node(transit.clone());
    graph.add_edge(previous, current, ());
    previous = current;
    ordered_nodes.push(transit);

    let destination = RouteGraphNode {
        kind: RouteGraphNodeKind::Destination,
        label: result
            .resolved_destination
            .clone()
            .unwrap_or_else(|| result.destination.clone()),
        detail: Some("Destination".to_string()),
    };
    let current = graph.add_node(destination.clone());
    graph.add_edge(previous, current, ());
    ordered_nodes.push(destination);

    let _node_count = graph.node_count();

    RouteGraph {
        nodes: ordered_nodes,
    }
}

pub fn render_route_graph_lines(graph: &RouteGraph) -> Vec<String> {
    if graph.nodes.is_empty() {
        return Vec::new();
    }

    let boxes: Vec<Vec<String>> = graph.nodes.iter().map(render_node).collect();
    let mut lines = Vec::new();

    for (index, node_lines) in boxes.iter().enumerate() {
        lines.extend(node_lines.iter().cloned());
        if index + 1 < boxes.len() {
            lines.push("   │".to_string());
            lines.push("   ▼".to_string());
        }
    }

    lines
}

fn render_node(node: &RouteGraphNode) -> Vec<String> {
    let mut content = truncate(&node.label, 32);
    if let Some(detail) = node.detail.as_deref().filter(|detail| !detail.is_empty()) {
        content.push_str(" · ");
        content.push_str(&truncate(detail, 32));
    }
    let width = content.chars().count() + 2;

    vec![
        format!("┌{}┐", "─".repeat(width)),
        format!("│ {content} │"),
        format!("└{}┘", "─".repeat(width)),
    ]
}

fn should_show_gateway(gateway: &str) -> bool {
    let lower = gateway.to_ascii_lowercase();
    lower != "link" && lower != "local" && !lower.starts_with("link#")
}

fn truncate(value: &str, max_chars: usize) -> String {
    let mut chars = value.chars();
    let truncated: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        format!("{truncated}...")
    } else {
        truncated
    }
}
