//! System statistics for monitoring
//!
//! Provides efficient access to CPU temperature, memory usage, and other system metrics.
//! All reads are from /proc and /sys filesystems which are very fast (no disk I/O).

use serde::Serialize;
use std::fs;
use std::path::Path;

/// System statistics snapshot
#[derive(Debug, Clone, Serialize)]
pub struct SystemStats {
    /// CPU temperature in Celsius (if available)
    pub cpu_temp_c: Option<f32>,
    /// Memory total in bytes
    pub mem_total_bytes: u64,
    /// Memory available in bytes
    pub mem_available_bytes: u64,
    /// Memory used percentage (0-100)
    pub mem_used_percent: f32,
    /// CPU usage percentage (0-100) - instantaneous from /proc/stat
    pub cpu_usage_percent: Option<f32>,
    /// System load average (1 minute)
    pub load_avg_1m: Option<f32>,
    /// System uptime in seconds
    pub uptime_secs: Option<u64>,
    /// Whether the system is throttled (Raspberry Pi specific)
    pub throttled: Option<ThrottleStatus>,
}

/// Raspberry Pi throttle status flags
#[derive(Debug, Clone, Serialize)]
pub struct ThrottleStatus {
    /// Currently under-voltage
    pub under_voltage: bool,
    /// Currently frequency capped
    pub freq_capped: bool,
    /// Currently throttled
    pub throttled: bool,
    /// Soft temperature limit reached
    pub soft_temp_limit: bool,
    /// Under-voltage has occurred since boot
    pub under_voltage_occurred: bool,
    /// Frequency capping has occurred since boot
    pub freq_capped_occurred: bool,
    /// Throttling has occurred since boot
    pub throttled_occurred: bool,
    /// Soft temperature limit occurred since boot
    pub soft_temp_limit_occurred: bool,
}

impl SystemStats {
    /// Gather current system statistics
    /// This is designed to be fast and non-blocking
    pub fn gather() -> Self {
        let (mem_total, mem_available) = read_memory_info();
        let mem_used_percent = if mem_total > 0 {
            ((mem_total - mem_available) as f64 / mem_total as f64 * 100.0) as f32
        } else {
            0.0
        };

        SystemStats {
            cpu_temp_c: read_cpu_temperature(),
            mem_total_bytes: mem_total,
            mem_available_bytes: mem_available,
            mem_used_percent,
            cpu_usage_percent: None, // Would require tracking over time, skip for simplicity
            load_avg_1m: read_load_average(),
            uptime_secs: read_uptime(),
            throttled: read_throttle_status(),
        }
    }
}

/// Read CPU temperature from thermal zone (works on most Linux systems including Pi)
fn read_cpu_temperature() -> Option<f32> {
    // Try Raspberry Pi thermal zone first
    let paths = [
        "/sys/class/thermal/thermal_zone0/temp",
        "/sys/devices/virtual/thermal/thermal_zone0/temp",
    ];

    for path in paths {
        if let Ok(content) = fs::read_to_string(path) {
            if let Ok(millidegrees) = content.trim().parse::<i32>() {
                return Some(millidegrees as f32 / 1000.0);
            }
        }
    }

    // Try vcgencmd on Raspberry Pi (fallback, slightly slower)
    if Path::new("/usr/bin/vcgencmd").exists() {
        if let Ok(output) = std::process::Command::new("vcgencmd")
            .arg("measure_temp")
            .output()
        {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                // Format: temp=42.0'C
                if let Some(temp_str) = stdout.strip_prefix("temp=") {
                    if let Some(temp_str) = temp_str.strip_suffix("'C\n") {
                        if let Ok(temp) = temp_str.parse::<f32>() {
                            return Some(temp);
                        }
                    }
                }
            }
        }
    }

    None
}

/// Read memory info from /proc/meminfo
fn read_memory_info() -> (u64, u64) {
    let mut total: u64 = 0;
    let mut available: u64 = 0;

    if let Ok(content) = fs::read_to_string("/proc/meminfo") {
        for line in content.lines() {
            if line.starts_with("MemTotal:") {
                total = parse_meminfo_kb(line) * 1024;
            } else if line.starts_with("MemAvailable:") {
                available = parse_meminfo_kb(line) * 1024;
            }
            // Early exit once we have both values
            if total > 0 && available > 0 {
                break;
            }
        }
    }

    (total, available)
}

/// Parse a meminfo line like "MemTotal:        4028416 kB"
fn parse_meminfo_kb(line: &str) -> u64 {
    line.split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0)
}

/// Read system load average from /proc/loadavg
fn read_load_average() -> Option<f32> {
    fs::read_to_string("/proc/loadavg")
        .ok()
        .and_then(|content| {
            content
                .split_whitespace()
                .next()
                .and_then(|s| s.parse().ok())
        })
}

/// Read system uptime from /proc/uptime
fn read_uptime() -> Option<u64> {
    fs::read_to_string("/proc/uptime")
        .ok()
        .and_then(|content| {
            content
                .split_whitespace()
                .next()
                .and_then(|s| s.parse::<f64>().ok())
                .map(|f| f as u64)
        })
}

/// Read Raspberry Pi throttle status via vcgencmd
fn read_throttle_status() -> Option<ThrottleStatus> {
    if !Path::new("/usr/bin/vcgencmd").exists() {
        return None;
    }

    let output = std::process::Command::new("vcgencmd")
        .arg("get_throttled")
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    // Format: throttled=0x0
    let hex_str = stdout.trim().strip_prefix("throttled=0x")?;
    let flags = u32::from_str_radix(hex_str, 16).ok()?;

    Some(ThrottleStatus {
        under_voltage: flags & (1 << 0) != 0,
        freq_capped: flags & (1 << 1) != 0,
        throttled: flags & (1 << 2) != 0,
        soft_temp_limit: flags & (1 << 3) != 0,
        under_voltage_occurred: flags & (1 << 16) != 0,
        freq_capped_occurred: flags & (1 << 17) != 0,
        throttled_occurred: flags & (1 << 18) != 0,
        soft_temp_limit_occurred: flags & (1 << 19) != 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_meminfo_kb() {
        assert_eq!(parse_meminfo_kb("MemTotal:        4028416 kB"), 4028416);
        assert_eq!(parse_meminfo_kb("MemAvailable:    3700000 kB"), 3700000);
        assert_eq!(parse_meminfo_kb("Invalid line"), 0);
    }

    #[test]
    fn test_gather_stats() {
        // Just make sure it doesn't panic
        let stats = SystemStats::gather();
        // Memory should be non-zero on any Linux system
        assert!(stats.mem_total_bytes > 0 || cfg!(not(target_os = "linux")));
    }
}
