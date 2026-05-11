//! r2-ui — UI utility function tests
//!
//! Tests for formatting functions used in the UI layer.
//! These tests do not require GTK4 initialization.

#[cfg(test)]
mod tests {
    /// Format bytes in human-readable form (KB, MB, GB, TB) using 1024-base
    fn format_bytes(bytes: u64) -> String {
        if bytes >= 1_099_511_627_776 {
            format!("{:.1} TB", bytes as f64 / 1_099_511_627_776.0)
        } else if bytes >= 1_073_741_824 {
            format!("{:.1} GB", bytes as f64 / 1_073_741_824.0)
        } else if bytes >= 1_048_576 {
            format!("{:.1} MB", bytes as f64 / 1_048_576.0)
        } else if bytes >= 1_024 {
            format!("{:.1} KB", bytes as f64 / 1_024.0)
        } else {
            format!("{} B", bytes)
        }
    }

    /// Format speed in human-readable form using 1024-base
    fn format_speed(bytes_per_sec: f64) -> String {
        if bytes_per_sec >= 1_073_741_824.0 {
            format!("{:.1} GB/s", bytes_per_sec / 1_073_741_824.0)
        } else if bytes_per_sec >= 1_048_576.0 {
            format!("{:.1} MB/s", bytes_per_sec / 1_048_576.0)
        } else if bytes_per_sec >= 1_024.0 {
            format!("{:.1} KB/s", bytes_per_sec / 1_024.0)
        } else {
            format!("{:.0} B/s", bytes_per_sec)
        }
    }

    /// Format ETA in human-readable form
    fn format_eta(secs: f64) -> String {
        if secs.is_nan() || secs.is_infinite() || secs <= 0.0 {
            return "—".to_string();
        }
        let total_secs = secs as u64;
        let hours = total_secs / 3600;
        let minutes = (total_secs % 3600) / 60;
        let seconds = total_secs % 60;

        if hours > 0 {
            format!("{}h {}min", hours, minutes)
        } else if minutes > 0 {
            format!("{}min {}s", minutes, seconds)
        } else {
            format!("{}s", seconds)
        }
    }

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(1024), "1.0 KB");
        assert_eq!(format_bytes(1_048_576), "1.0 MB");
        assert_eq!(format_bytes(1_073_741_824), "1.0 GB");
        assert_eq!(format_bytes(1_099_511_627_776), "1.0 TB");
    }

    #[test]
    fn test_format_speed() {
        assert_eq!(format_speed(0.0), "0 B/s");
        assert_eq!(format_speed(500.0), "500 B/s");
        assert_eq!(format_speed(1024.0), "1.0 KB/s");
        assert_eq!(format_speed(1024.0 * 1024.0 * 5.0), "5.0 MB/s");
        assert_eq!(format_speed(1_073_741_824.0), "1.0 GB/s");
    }

    #[test]
    fn test_format_eta() {
        assert_eq!(format_eta(10.0), "10s");
        assert_eq!(format_eta(120.0), "2min 0s");
        assert_eq!(format_eta(3600.0), "1h 0min");
        assert_eq!(format_eta(3661.0), "1h 1min");
        assert_eq!(format_eta(0.0), "—");
        assert_eq!(format_eta(f64::NAN), "—");
        assert_eq!(format_eta(f64::INFINITY), "—");
    }

    #[test]
    fn test_format_bytes_edge_cases() {
        assert_eq!(format_bytes(1), "1 B");
        assert_eq!(format_bytes(999), "999 B");
        assert_eq!(format_bytes(1000), "1000 B");
        assert_eq!(format_bytes(1023), "1023 B");
        assert_eq!(format_bytes(1024), "1.0 KB");
        assert_eq!(format_bytes(1025), "1.0 KB");
        assert_eq!(format_bytes(1536), "1.5 KB");
    }

    #[test]
    fn test_format_speed_edge_cases() {
        assert_eq!(format_speed(1.0), "1 B/s");
        assert_eq!(format_speed(999.0), "999 B/s");
        assert_eq!(format_speed(1024.0), "1.0 KB/s");
        assert_eq!(format_speed(1_048_576.0), "1.0 MB/s");
    }
}
