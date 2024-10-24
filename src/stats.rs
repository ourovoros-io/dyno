use serde::{Deserialize, Serialize};

use crate::types::{Benchmark, BenchmarkFrame};

use crate::wrap;

use std::sync::MutexGuard;

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Collection(pub Vec<(String, Stats)>);

/// [`Stats`] struct that contains the regression information for each metric
/// The tuple contains the change and the percentage change for each metric
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Stats {
    pub cpu_usage: (f64, f64),
    pub memory_usage: (f64, f64),
    pub virtual_memory_usage: (f64, f64),
    pub disk_total_written_bytes: (f64, f64),
    pub disk_written_bytes: (f64, f64),
    pub disk_total_read_bytes: (f64, f64),
    pub disk_read_bytes: (f64, f64),
    pub bytecode_size: (f64, f64),
    pub data_section_size: (f64, f64),
    pub time: (f64, f64),
}

/// Aggregate the values of a metric from all the frames
fn aggregate_values(frames: &[BenchmarkFrame], metric_fn: fn(&BenchmarkFrame) -> f64) -> f64 {
    frames.iter().map(metric_fn).sum()
}

/// Calculate the change and the percentage change between two values
fn calculate_change(previous: f64, current: f64) -> (f64, f64) {
    #[allow(clippy::float_cmp)]
    if previous == current {
        return (0.0, 0.0);
    } else if previous > current {
        if current == 0.0 {
            return (previous, -100.0);
        }
        return (
            -(previous - current),
            -(100.0 - ((current / previous) * 100.0)),
        );
    }

    if previous == 0.0 {
        (current, 100.0)
    } else {
        (
            current - previous,
            -(100.0 - ((current / previous) * 100.0)),
        )
    }
}

/// Check if the change in a metric is greater than the threshold
fn check(previous: f64, current: f64) -> (f64, f64) {
    let (change, percentage_change) = calculate_change(previous, current);
    (change, percentage_change)
}

/// Calculate the regression between two benchmarks
///
/// # Arguments
///
/// * `previous_benchmark` - The previous benchmark
///
/// * `current_benchmark` - The current benchmark
///
/// # Returns
///
/// A `Stats` struct containing the regression information for each metric
///
/// # Errors
///
/// If the lock on the frames fails
///
/// If the asm information is missing
///
/// If the bytecode size is missing
///
/// If the data section is missing
///
/// If the data section size is missing
///
/// If the time is missing
///
/// # Panics
///
/// If the metric name is unknown
#[allow(clippy::too_many_lines)]
pub(crate) fn calculate(
    previous_benchmark: &Benchmark,
    current_benchmark: &Benchmark,
) -> crate::error::Result<Stats> {
    let previous_frames: MutexGuard<_> = previous_benchmark
        .frames
        .lock()
        .expect("Failed to get the previous benchmark frames lock");

    let current_frames: MutexGuard<_> = current_benchmark
        .frames
        .lock()
        .expect("Failed to get the current benchmark frames lock");

    #[allow(clippy::type_complexity)]
    let metrics: Vec<(&str, fn(&BenchmarkFrame) -> f64)> = vec![
        ("cpu_usage", |f| f64::from(f.cpu_usage)),
        ("memory_usage", |f| f.memory_usage as f64),
        ("virtual_memory_usage", |f| f.virtual_memory_usage as f64),
        ("disk_total_written_bytes", |f| {
            f.disk_total_written_bytes as f64
        }),
        ("disk_written_bytes", |f| f.disk_written_bytes as f64),
        ("disk_total_read_bytes", |f| f.disk_total_read_bytes as f64),
        ("disk_read_bytes", |f| f.disk_read_bytes as f64),
    ];

    let mut regression = Stats::default();

    for (metric_name, metric_fn) in metrics {
        let previous_aggregated_value = aggregate_values(&previous_frames, metric_fn);
        let current_aggregated_value = aggregate_values(&current_frames, metric_fn);

        let metric = match metric_name {
            "cpu_usage" => &mut regression.cpu_usage,
            "memory_usage" => &mut regression.memory_usage,
            "virtual_memory_usage" => &mut regression.virtual_memory_usage,
            "disk_total_written_bytes" => &mut regression.disk_total_written_bytes,
            "disk_written_bytes" => &mut regression.disk_written_bytes,
            "disk_total_read_bytes" => &mut regression.disk_total_read_bytes,
            "disk_read_bytes" => &mut regression.disk_read_bytes,
            _ => panic!("Unknown metric"),
        };

        *metric = check(previous_aggregated_value, current_aggregated_value);
    }

    let previous_bytecode_size = previous_benchmark
        .asm_information
        .as_ref()
        .ok_or(wrap!(
            "Failed to get previous asm information for bytecode size".into()
        ))?
        .get("bytecode_size")
        .ok_or(wrap!("Failed to get the previous bytecode size".into()))?
        .as_u64()
        .ok_or(wrap!("Failed to parse previous bytecode size as u64".into()))?
        as f64;

    let current_bytecode_size = current_benchmark
        .asm_information
        .as_ref()
        .ok_or(wrap!(
            "Failed to get current asm information for bytecode size".into()
        ))?
        .get("bytecode_size")
        .ok_or(wrap!("Failed to get the current bytecode size".into()))?
        .as_u64()
        .ok_or(wrap!("Failed to parse current bytecode size as u64".into()))?
        as f64;

    regression.bytecode_size = check(previous_bytecode_size, current_bytecode_size);

    let previous_datasection_size = previous_benchmark
        .asm_information
        .as_ref()
        .ok_or(wrap!(
            "Failed to get previous asm information for data section".into()
        ))?
        .get("data_section")
        .ok_or(wrap!("Failed to get previous data section".into()))?
        .get("size")
        .ok_or(wrap!("Failed to get previous size of data section".into()))?
        .as_u64()
        .ok_or(wrap!(
            "Failed to parse previous size for data section as u64".into()
        ))? as f64;

    let current_datasection_size = current_benchmark
        .asm_information
        .as_ref()
        .ok_or(wrap!(
            "Failed to get current asm information for data section".into()
        ))?
        .get("data_section")
        .ok_or(wrap!("Failed to get current data section".into()))?
        .get("size")
        .ok_or(wrap!("Failed to get current size of data section".into()))?
        .as_u64()
        .ok_or(wrap!(
            "Failed to parse current size for data section as u64".into()
        ))? as f64;

    regression.data_section_size = check(previous_datasection_size, current_datasection_size);

    let previous_time = previous_benchmark
        .end_time
        .as_ref()
        .ok_or(wrap!("Failed to get previous end time of benchmarks".into()))?
        .as_millis()
        - previous_benchmark
            .start_time
            .as_ref()
            .ok_or(wrap!(
                "Failed to get previous start time of benchmarks".into()
            ))?
            .as_millis();
    let current_time = current_benchmark
        .end_time
        .as_ref()
        .ok_or(wrap!("Failed to get current end time of benchmarks".into()))?
        .as_millis()
        - current_benchmark
            .start_time
            .as_ref()
            .ok_or(wrap!(
                "Failed to get current start time of benchmarks".into()
            ))?
            .as_millis();

    regression.time = check(previous_time as f64, current_time as f64);

    Ok(regression)
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_regression() -> crate::error::Result<()> {
        let bench1 = std::fs::read_to_string("test_data/bench.json")?;
        let bench1 = serde_json::from_str::<crate::types::Benchmarks>(&bench1)?;

        let bench2 = std::fs::read_to_string("test_data/bench_regression.json")?;
        let bench2 = serde_json::from_str::<crate::types::Benchmarks>(&bench2)?;

        let regression = crate::stats::calculate(&bench1.benchmarks[0], &bench2.benchmarks[0]);
        assert!(regression.is_ok());
        println!("{:#?}", regression);
        Ok(())
    }
}
