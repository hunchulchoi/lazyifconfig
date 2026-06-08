use std::collections::HashMap;

use crate::model::{InterfaceStats, NetworkInterface};

pub fn parse_stats(input: &str) -> HashMap<String, InterfaceStats> {
    let mut current_name: Option<String> = None;
    let mut stats_by_name: HashMap<String, InterfaceStats> = HashMap::new();

    for line in input.lines() {
        if is_interface_header(line) {
            current_name = Some(line.split(':').next().unwrap_or_default().to_string());
            continue;
        }

        let Some(name) = current_name.as_ref() else {
            continue;
        };

        let trimmed = line.trim();
        let Some((direction, packets, bytes)) = parse_stat_line(trimmed) else {
            continue;
        };

        let stats = stats_by_name.entry(name.clone()).or_default();
        match direction {
            StatDirection::Rx => {
                stats.rx_packets = packets;
                stats.rx_bytes = bytes;
            }
            StatDirection::Tx => {
                stats.tx_packets = packets;
                stats.tx_bytes = bytes;
            }
        }
    }

    stats_by_name
}

pub fn merge_stats(input: &str, mut interfaces: Vec<NetworkInterface>) -> Vec<NetworkInterface> {
    let stats_by_name = if input.contains("Ibytes") && input.contains("Obytes") {
        parse_netstat_ib(input)
    } else {
        parse_stats(input)
    };

    for interface in &mut interfaces {
        if let Some(stats) = stats_by_name.get(&interface.name) {
            interface.stats = Some(stats.clone());
        }
    }

    interfaces
}

fn parse_netstat_ib(input: &str) -> HashMap<String, InterfaceStats> {
    let mut stats_by_name: HashMap<String, InterfaceStats> = HashMap::new();

    for line in input.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 10 {
            continue;
        }

        // We only care about Link-level stats row, which has `<Link#` in the Network column (parts[2])
        if !parts[2].starts_with("<Link#") {
            continue;
        }

        let name = parts[0].trim_end_matches('*').to_string();
        let len = parts.len();

        let rx_packets = match parts[len - 7].parse::<u64>() {
            Ok(v) => v,
            Err(_) => continue,
        };
        let rx_bytes = match parts[len - 5].parse::<u64>() {
            Ok(v) => v,
            Err(_) => continue,
        };
        let tx_packets = match parts[len - 4].parse::<u64>() {
            Ok(v) => v,
            Err(_) => continue,
        };
        let tx_bytes = match parts[len - 2].parse::<u64>() {
            Ok(v) => v,
            Err(_) => continue,
        };

        stats_by_name.insert(
            name,
            InterfaceStats {
                rx_packets,
                rx_bytes,
                tx_packets,
                tx_bytes,
            },
        );
    }

    stats_by_name
}

fn is_interface_header(line: &str) -> bool {
    !line.starts_with(' ') && !line.starts_with('\t') && line.contains(':')
}

fn parse_stat_line(line: &str) -> Option<(StatDirection, u64, u64)> {
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() < 5 || parts[1] != "packets" || parts[3] != "bytes" {
        return None;
    }

    let packets = parts[2].parse().ok()?;
    let bytes = parts[4].parse().ok()?;

    match parts[0] {
        "RX" => Some((StatDirection::Rx, packets, bytes)),
        "TX" => Some((StatDirection::Tx, packets, bytes)),
        _ => None,
    }
}

enum StatDirection {
    Rx,
    Tx,
}
