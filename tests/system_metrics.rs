use lazyifconfig::collector::system::{
    calculate_cpu_usage_percent, parse_linux_cpu_sample, parse_linux_memory,
};
use lazyifconfig::model::CpuSample;

#[test]
fn linux_memory_uses_mem_available_as_free_memory() {
    let meminfo = "\
MemTotal:       16384000 kB
MemFree:         1000000 kB
MemAvailable:    4096000 kB
Buffers:          200000 kB
";

    let metrics = parse_linux_memory(meminfo).unwrap();

    assert_eq!(metrics.memory_total_bytes, Some(16_384_000 * 1024));
    assert_eq!(metrics.memory_used_bytes, Some(12_288_000 * 1024));
}

#[test]
fn linux_cpu_usage_is_calculated_from_consecutive_samples() {
    let previous = CpuSample {
        idle_ticks: 100,
        total_ticks: 1_000,
    };
    let current = CpuSample {
        idle_ticks: 150,
        total_ticks: 1_200,
    };

    assert_eq!(calculate_cpu_usage_percent(previous, current), Some(75));
}

#[test]
fn linux_cpu_sample_reads_idle_and_total_ticks() {
    let stat = "cpu  4705 0 2253 136239 124 0 235 0 0 0\ncpu0 100 0 50 3000\n";

    let sample = parse_linux_cpu_sample(stat).unwrap();

    assert_eq!(sample.idle_ticks, 136_363);
    assert_eq!(sample.total_ticks, 143_556);
}
