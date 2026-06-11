pub fn is_vpn_interface_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.starts_with("tun")
        || lower.starts_with("tap")
        || lower.starts_with("utun")
        || lower.starts_with("wg")
        || lower.starts_with("tailscale")
        || lower.starts_with("zt")
}
