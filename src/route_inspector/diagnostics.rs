use std::collections::{HashMap, HashSet};

use crate::model::{
    InterfaceStatus, NetworkInterface, RouteDiagnostic, RouteDiagnosticSeverity, RouteEntry,
};

use super::vpn::is_vpn_interface_name;

pub fn build_route_diagnostics(
    routes: &[RouteEntry],
    interfaces: &[NetworkInterface],
) -> Vec<RouteDiagnostic> {
    let mut diagnostics = Vec::new();
    let default_routes: Vec<&RouteEntry> = routes
        .iter()
        .filter(|route| is_default_route(route))
        .collect();

    if default_routes.is_empty() {
        diagnostics.push(diagnostic(
            RouteDiagnosticSeverity::Warning,
            "No default route",
            "No default route was found in the routing table.",
            None,
            "Add or restore a default gateway so traffic can reach external networks.",
        ));
    }

    if default_routes.len() > 1 {
        diagnostics.push(diagnostic(
            RouteDiagnosticSeverity::Warning,
            "Multiple default routes",
            "More than one default route is present.",
            default_routes.first().map(|route| (*route).clone()),
            "Review route metrics and remove unintended default routes.",
        ));
    }

    for route in &default_routes {
        if is_vpn_interface_name(&route.interface) {
            diagnostics.push(diagnostic(
                RouteDiagnosticSeverity::Info,
                "VPN overrides default route",
                "The default route is using a VPN-like interface.",
                Some((*route).clone()),
                "This is expected for full-tunnel VPNs; verify it matches your intended routing policy.",
            ));
        }
    }

    let interface_by_name: HashMap<&str, &NetworkInterface> = interfaces
        .iter()
        .map(|interface| (interface.name.as_str(), interface))
        .collect();
    let mut reported_missing = HashSet::new();

    for route in routes {
        if let Some(interface) = interface_by_name.get(route.interface.as_str()) {
            if interface.status == InterfaceStatus::Down {
                diagnostics.push(diagnostic(
                    RouteDiagnosticSeverity::Warning,
                    "Route interface is down",
                    "A route points to an interface that is currently down.",
                    Some(route.clone()),
                    "Bring the interface up or remove the stale route.",
                ));
            }
        } else if !interfaces.is_empty() && reported_missing.insert(route.interface.clone()) {
            diagnostics.push(diagnostic(
                RouteDiagnosticSeverity::Warning,
                "Route references missing interface",
                "A route points to an interface that was not found in the interface list.",
                Some(route.clone()),
                "Refresh network state or remove routes that reference missing interfaces.",
            ));
        }
    }

    let mut metric_keys: HashSet<(&str, u32)> = HashSet::new();
    let mut reported_metric_conflicts: HashSet<(&str, u32)> = HashSet::new();
    for route in routes {
        let Some(metric) = route.metric else {
            continue;
        };
        let key = (route.destination.as_str(), metric);
        if !metric_keys.insert(key) && reported_metric_conflicts.insert(key) {
            diagnostics.push(diagnostic(
                RouteDiagnosticSeverity::Warning,
                "Route metric conflict",
                "Multiple routes share the same destination and metric.",
                Some(route.clone()),
                "Adjust route metrics so route selection is deterministic.",
            ));
        }
    }

    diagnostics
}

pub fn is_default_route(route: &RouteEntry) -> bool {
    matches!(
        route.destination.trim().to_ascii_lowercase().as_str(),
        "default" | "0.0.0.0/0"
    )
}

fn diagnostic(
    severity: RouteDiagnosticSeverity,
    title: &str,
    description: &str,
    affected_route: Option<RouteEntry>,
    recommendation: &str,
) -> RouteDiagnostic {
    RouteDiagnostic {
        severity,
        title: title.to_string(),
        description: description.to_string(),
        affected_route,
        recommendation: recommendation.to_string(),
    }
}
