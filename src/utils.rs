use crate::error::Result;
use crate::types::{Benchmark, SystemSpecs};
use crate::wrap;

/// Collect all profiling targets in the given directory and return a map of the target name to the path canonical path.
///
/// # Arguments
///
/// * `path` - A path to the directory containing the profiling targets.
///
/// # Returns
///
/// A `Result` containing a `Vec` of `Benchmark` structs.
///
/// # Errors
///
/// If the path is not a valid directory.
///
pub fn generate_benchmarks<P: AsRef<std::path::Path>>(path: P) -> Result<Vec<Benchmark>> {
    let mut path = path.as_ref();

    if path
        .components()
        .last()
        .ok_or_else(|| wrap!("Failed to get last component from path.".into()))?
        .as_os_str()
        .to_str()
        .ok_or_else(|| wrap!("Failed to get str from os str for last component of path.".into()))?
        == "src"
    {
        path = path
            .parent()
            .ok_or_else(|| wrap!("Failed to get parent of path.".into()))?;
    }

    let mut targets = Vec::new();
    for entry in walkdir::WalkDir::new(path)
        .into_iter()
        .filter_map(std::result::Result::ok)
        .filter(|e| e.file_type().is_file() && e.file_name() == "Forc.toml")
    {
        let entry_path = entry.path();
        let Some(entry_path_parent) = entry_path.parent() else {
            continue;
        };

        let canonical_path =
            std::fs::canonicalize(entry_path_parent).map_err(|e| wrap!(e.into()))?;

        if let Some(name) = canonical_path.file_name().and_then(|n| n.to_str()) {
            let benchmark = Benchmark::new(&name.to_string(), canonical_path.clone());
            if benchmark.verify_path() {
                targets.push(benchmark);
            }
        }
    }

    if targets.is_empty() {
        return Err("No targets found in the directory. Make sure that you are providing a directory or directories that contain sway contracts.".into());
    }

    println!("Found {} targets in the directory.", targets.len());

    Ok(targets)
}

/// Returns the full system specifications as a `SystemSpecs` struct.
///
/// # Returns
///
/// A `Result` containing a `SystemSpecs` struct.
///
/// # Errors
///
/// If the system specifications cannot be collected.
/// If the system specifications cannot be serialized.
///
pub fn system_specs() -> Result<crate::types::SystemSpecs> {
    println!("Collecting system specifications...");
    let mut sys = sysinfo::System::new_all();
    sys.refresh_all();

    let system_specs_string = serde_json::to_string(&sys).map_err(|e| wrap!(e.into()))?;
    let system_specs: SystemSpecs =
        serde_json::from_str(&system_specs_string).map_err(|e| wrap!(e.into()))?;

    Ok(system_specs)
}

/// Setup the benchmarking environment and check for the necessary tools.
///
/// # Errors
///
/// If the creation of the benchmarks output folder fails.
///
/// If the inferno folder does not exist.
///
/// If the perf command is not available.
///
pub fn setup_system(options: &crate::cli::Options) -> Result<()> {
    println!("Setting up the benchmarking environment...");
    // Create the benchmarks output folder if it does not exist
    if !std::path::Path::new(&options.output_folder).exists() {
        std::fs::create_dir(&options.output_folder).map_err(|e| wrap!(e.into()))?;
    }

    if !options
        .output_folder
        .join(crate::BENCHMARKS_RUN_FOLDER)
        .exists()
    {
        std::fs::create_dir(options.output_folder.join(crate::BENCHMARKS_RUN_FOLDER))
            .map_err(|e| wrap!(e.into()))?;
    }

    if !options
        .output_folder
        .join(crate::BENCHMARKS_STATS_FOLDER)
        .exists()
    {
        std::fs::create_dir(options.output_folder.join(crate::BENCHMARKS_STATS_FOLDER))
            .map_err(|e| wrap!(e.into()))?;
    }

    if !options
        .output_folder
        .join(crate::BENCHMARKS_FLAMEGRAPH_FOLDER)
        .exists()
    {
        std::fs::create_dir(
            options
                .output_folder
                .join(crate::BENCHMARKS_FLAMEGRAPH_FOLDER),
        )
        .map_err(|e| wrap!(e.into()))?;
    }

    Ok(())
}

/// Store the item in the output folder.
///
/// # Arguments
///
/// * `item` - A reference to the item to be stored.
///
/// * `options` - A reference to the `Options` struct.
///
/// * `folder` - A string slice containing the folder name.
///
/// # Errors
///
/// If the item cannot be serialized.
///
/// If the item cannot be written to the file.
///
/// If the forc version cannot be retrieved.
///
/// If the binary hash cannot be computed.
///
/// If the file cannot be written to the output folder.
///
pub fn store_item<T: serde::Serialize>(item: &T, path: &str) -> Result<()> {
    let item_json_string = serde_json::to_string_pretty(&item).map_err(|e| wrap!(e.into()))?;

    std::fs::write(path, item_json_string).map_err(|e| wrap!(e.into()))?;

    println!("Stored item in the output folder. File : {path}");

    Ok(())
}

/// Get the current date and time in the format "YYYY-MM-DD--HH:MM:SS"
pub fn get_date_time() -> String {
    let datetime = chrono::Local::now();
    datetime.format("%Y-%m-%d_%H:%M:%S").to_string()
}

pub fn read_latest_file_in_directory(directory: &std::path::Path) -> Result<std::path::PathBuf> {
    // List the files in the directory and filter by .json extension
    let mut entries: Vec<std::path::PathBuf> =
        get_files_in_dir(directory, crate::EXPORT_FILE_TYPE_JSON).map_err(|e| wrap!(e))?;

    // Sort the files by modification time
    entries.sort_by_key(|path| {
        std::fs::metadata(path)
            .and_then(|metadata| metadata.modified())
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
    });

    // Get the latest file
    if let Some(latest_file) = entries.last() {
        // Read the latest file to a string
        return Ok(latest_file.clone());
    }

    Err("No files found in the directory".into())
}

pub fn get_files_in_dir(
    directory: &std::path::Path,
    extension: &str,
) -> Result<Vec<std::path::PathBuf>> {
    Ok(std::fs::read_dir(directory)
        .map_err(|e| wrap!(e.into()))?
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            if path.extension()?.to_str()? == extension {
                Some(path)
            } else {
                None
            }
        })
        .collect())
}

#[inline]
pub fn compute_md5(path: &std::path::Path) -> Result<String> {
    Ok(format!(
        "{:X}",
        md5::compute(std::fs::read(path).map_err(|e| wrap!(e.into()))?)
    ))
}

pub fn get_forc_version(path: &std::path::Path) -> Result<String> {
    let output = std::process::Command::new(path)
        .arg("--version")
        .output()
        .map_err(|e| wrap!(e.into()))?;

    let version = String::from_utf8(output.stdout).map_err(|e| wrap!(e.into()))?;
    let version = version.replace("forc", "").trim().to_string();
    Ok(version)
}

pub fn print_welcome() {
    println!("{}", "=".repeat(100));
    println!(
        "{}Welcome to the Fuel Dyno v{}",
        "     ".repeat(5),
        env!("CARGO_PKG_VERSION")
    );
    println!("{}", "=".repeat(100));
}


use tabled::{Table, Tabled};

#[derive(Tabled)]
struct MetricRow {
    metric: &'static str,
    value_change: String,
    percentage_change: String,
}

#[derive(Tabled)]
struct AsmRow {
    metric: &'static str,
    previous: String,
    current: String,
}

/// Print the performance regression or improvements.
/// 
/// # Arguments
/// 
/// * `stats_result` - A reference to a `stats::Collection`.
/// 
/// * `previous_benchmarks` - A reference to a vector of `types::Benchmark`.
/// 
/// * `current_benchmarks` - A reference to a vector of `types::Benchmark`.
/// 
/// # Errors
/// 
/// If the function is unable to get the asm information.
/// 
pub fn print_stats(
    stats_result: &crate::stats::Collection,
    previous_benchmarks: &[crate::types::Benchmark],
    current_benchmarks: &[crate::types::Benchmark],
) -> Result<()> {
    println!("Printing performance regression or improvements");

    // Create a vector to hold the metric rows
    let mut metric_rows = Vec::new();

    // Add a row for each metric
    for (path, benchmark) in &stats_result.0 {
        metric_rows.push(MetricRow {
            metric: "Path",
            value_change: path.clone(),
            percentage_change: String::new(),
        });
        metric_rows.push(MetricRow {
            metric: "CPU Usage",
            value_change: benchmark.cpu_usage.0.to_string(),
            percentage_change: benchmark.cpu_usage.1.to_string(),
        });
        metric_rows.push(MetricRow {
            metric: "Memory Usage",
            value_change: benchmark.memory_usage.0.to_string(),
            percentage_change: benchmark.memory_usage.1.to_string(),
        });
        metric_rows.push(MetricRow {
            metric: "Virtual Memory Usage",
            value_change: benchmark.virtual_memory_usage.0.to_string(),
            percentage_change: benchmark.virtual_memory_usage.1.to_string(),
        });
        metric_rows.push(MetricRow {
            metric: "Disk Total Written Bytes",
            value_change: benchmark.disk_total_written_bytes.0.to_string(),
            percentage_change: benchmark.disk_total_written_bytes.1.to_string(),
        });
        metric_rows.push(MetricRow {
            metric: "Disk Written Bytes",
            value_change: benchmark.disk_written_bytes.0.to_string(),
            percentage_change: benchmark.disk_written_bytes.1.to_string(),
        });
        metric_rows.push(MetricRow {
            metric: "Disk Total Read Bytes",
            value_change: benchmark.disk_total_read_bytes.0.to_string(),
            percentage_change: benchmark.disk_total_read_bytes.1.to_string(),
        });
        metric_rows.push(MetricRow {
            metric: "Disk Read Bytes",
            value_change: benchmark.disk_read_bytes.0.to_string(),
            percentage_change: benchmark.disk_read_bytes.1.to_string(),
        });
        metric_rows.push(MetricRow {
            metric: "Bytecode Size",
            value_change: benchmark.bytecode_size.0.to_string(),
            percentage_change: benchmark.bytecode_size.1.to_string(),
        });
        metric_rows.push(MetricRow {
            metric: "Data Section Size",
            value_change: benchmark.data_section_size.0.to_string(),
            percentage_change: benchmark.data_section_size.1.to_string(),
        });
        metric_rows.push(MetricRow {
            metric: "Time",
            value_change: benchmark.time.0.to_string(),
            percentage_change: benchmark.time.1.to_string(),
        });
    }

    // Create a vector to hold the ASM rows
    let mut asm_rows = Vec::new();
    asm_rows.push(AsmRow {
        metric: "Asm Information",
        previous: "Previous".to_string(),
        current: "Current".to_string(),
    });

    let benchmarks = previous_benchmarks.iter().zip(current_benchmarks);
    for (previous_benchmark, current_benchmark) in benchmarks {
        asm_rows.push(AsmRow {
            metric: "Bytecode Size",
            previous: previous_benchmark.asm_information.as_ref().ok_or_else(|| {
                wrap!("Failed to get asm information for previous bytecode size".into())
            })?["bytecode_size"]
                .to_string(),
            current: current_benchmark.asm_information.as_ref().ok_or_else(|| {
                wrap!("Failed to get asm information for current bytecode size".into())
            })?["bytecode_size"]
                .to_string(),
        });
        asm_rows.push(AsmRow {
            metric: "Data Section Size",
            previous: previous_benchmark.asm_information.as_ref().ok_or_else(|| {
                wrap!("Failed to get asm information for previous data section size".into())
            })?["data_section"]["size"]
                .to_string(),
            current: current_benchmark.asm_information.as_ref().ok_or_else(|| {
                wrap!("Failed to get asm information for current data section size".into())
            })?["data_section"]["size"]
                .to_string(),
        });
        asm_rows.push(AsmRow {
            metric: "Data Section Used",
            previous: previous_benchmark.asm_information.as_ref().ok_or_else(|| {
                wrap!("Failed to get asm information for previous data section used".into())
            })?["data_section"]["used"]
                .to_string(),
            current: current_benchmark.asm_information.as_ref().ok_or_else(|| {
                wrap!("Failed to get asm information for current data section used".into())
            })?["data_section"]["used"]
                .to_string(),
        });
    }

    // Create and print the metric table
    let metric_table = Table::new(metric_rows);
    println!("{metric_table}");

    // Create and print the ASM table
    let asm_table = Table::new(asm_rows);
    println!("{asm_table}");
    Ok(())
}


#[cfg(test)]
mod tests {

    #[test]
    fn test_get_forc_version() {
        let forc_path = std::path::Path::new("forc");
        let version = super::get_forc_version(forc_path).unwrap();
        assert_eq!("0.63.1", version);
    }
}
