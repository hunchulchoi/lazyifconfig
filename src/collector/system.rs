use std::process::Command;

use crate::model::{CpuSample, SystemMetrics};

pub fn collect_system_metrics(
    previous_cpu_sample: Option<CpuSample>,
) -> (SystemMetrics, Option<CpuSample>) {
    if cfg!(target_os = "linux") {
        collect_linux_system_metrics(previous_cpu_sample)
    } else if cfg!(target_os = "macos") {
        (collect_macos_system_metrics(), previous_cpu_sample)
    } else {
        (SystemMetrics::default(), previous_cpu_sample)
    }
}

fn collect_linux_system_metrics(
    previous_cpu_sample: Option<CpuSample>,
) -> (SystemMetrics, Option<CpuSample>) {
    let cpu_sample = std::fs::read_to_string("/proc/stat")
        .ok()
        .and_then(|contents| parse_linux_cpu_sample(&contents));
    let memory = std::fs::read_to_string("/proc/meminfo")
        .ok()
        .and_then(|contents| parse_linux_memory(&contents))
        .unwrap_or_default();

    let cpu_usage_percent = previous_cpu_sample
        .zip(cpu_sample)
        .and_then(|(previous, current)| calculate_cpu_usage_percent(previous, current));

    (
        SystemMetrics {
            cpu_usage_percent,
            ..memory
        },
        cpu_sample,
    )
}

fn collect_macos_system_metrics() -> SystemMetrics {
    let cpu_usage_percent = command_stdout("top", &["-l", "1", "-n", "0"])
        .ok()
        .and_then(|output| parse_macos_cpu_usage(&output));
    let memory = collect_macos_memory().unwrap_or_default();

    SystemMetrics {
        cpu_usage_percent,
        ..memory
    }
}

pub fn parse_linux_memory(input: &str) -> Option<SystemMetrics> {
    let total_kib = linux_meminfo_value(input, "MemTotal")?;
    let available_kib = linux_meminfo_value(input, "MemAvailable")?;
    let used_kib = total_kib.saturating_sub(available_kib);

    Some(SystemMetrics {
        memory_used_bytes: Some(used_kib * 1024),
        memory_total_bytes: Some(total_kib * 1024),
        ..SystemMetrics::default()
    })
}

fn linux_meminfo_value(input: &str, key: &str) -> Option<u64> {
    input.lines().find_map(|line| {
        let rest = line.strip_prefix(key)?.trim_start();
        let value = rest.strip_prefix(':')?.trim_start();
        value.split_whitespace().next()?.parse().ok()
    })
}

pub fn parse_linux_cpu_sample(input: &str) -> Option<CpuSample> {
    let line = input.lines().find(|line| line.starts_with("cpu "))?;
    let values: Vec<u64> = line
        .split_whitespace()
        .skip(1)
        .filter_map(|part| part.parse().ok())
        .collect();

    if values.len() < 4 {
        return None;
    }

    let idle_ticks = values[3] + values.get(4).copied().unwrap_or(0);
    let total_ticks = values.iter().sum();
    Some(CpuSample {
        idle_ticks,
        total_ticks,
    })
}

pub fn calculate_cpu_usage_percent(previous: CpuSample, current: CpuSample) -> Option<u8> {
    let total_delta = current.total_ticks.checked_sub(previous.total_ticks)?;
    if total_delta == 0 {
        return None;
    }

    let idle_delta = current.idle_ticks.saturating_sub(previous.idle_ticks);
    let busy_delta = total_delta.saturating_sub(idle_delta);
    Some(((busy_delta * 100 + total_delta / 2) / total_delta).min(100) as u8)
}

fn collect_macos_memory() -> Option<SystemMetrics> {
    let total = command_stdout("sysctl", &["-n", "hw.memsize"])
        .ok()?
        .trim()
        .parse::<u64>()
        .ok()?;
    let vm_stat = command_stdout("vm_stat", &[]).ok()?;
    let used = parse_macos_memory_used(&vm_stat)?;

    Some(SystemMetrics {
        memory_used_bytes: Some(used.min(total)),
        memory_total_bytes: Some(total),
        ..SystemMetrics::default()
    })
}

fn parse_macos_cpu_usage(input: &str) -> Option<u8> {
    let marker = "CPU usage:";
    let usage = input.lines().find_map(|line| line.split_once(marker))?.1;
    let idle = usage.split(',').find_map(|part| {
        let trimmed = part.trim();
        let value = trimmed.strip_suffix("% idle")?;
        value.trim().parse::<f64>().ok()
    })?;
    Some((100.0 - idle).round().clamp(0.0, 100.0) as u8)
}

fn parse_macos_memory_used(input: &str) -> Option<u64> {
    let page_size = input.lines().find_map(|line| {
        let start = line.find("page size of ")? + "page size of ".len();
        let end = line[start..].find(" bytes")? + start;
        line[start..end].parse::<u64>().ok()
    })?;

    let active = macos_vm_stat_pages(input, "Pages active").unwrap_or(0);
    let wired = macos_vm_stat_pages(input, "Pages wired down").unwrap_or(0);
    let compressed = macos_vm_stat_pages(input, "Pages occupied by compressor").unwrap_or(0);

    Some((active + wired + compressed) * page_size)
}

fn macos_vm_stat_pages(input: &str, key: &str) -> Option<u64> {
    input.lines().find_map(|line| {
        let value = line.trim().strip_prefix(key)?.trim_start();
        let value = value.strip_prefix(':')?.trim();
        value.trim_end_matches('.').replace('.', "").parse().ok()
    })
}

fn command_stdout(program: &str, args: &[&str]) -> Result<String, String> {
    let output = Command::new(program)
        .args(args)
        .output()
        .map_err(|err| err.to_string())?;
    if output.status.success() {
        String::from_utf8(output.stdout).map_err(|err| err.to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}
