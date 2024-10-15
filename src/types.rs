use crate::wrap;
use crossbeam_channel::{unbounded, Receiver, Sender};
use inferno::{collapse::Collapse, flamegraph::from_reader};
use serde::{Deserialize, Serialize};
use std::{
    io::{BufRead, BufReader, BufWriter},
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use sysinfo::Pid;

#[cfg(target_os = "linux")]
use inferno::collapse::perf::Folder;
#[cfg(target_os = "macos")]
use inferno::collapse::sample::Folder;

/// A collection of benchmarks and system specifications.
#[derive(Debug, Serialize, Deserialize)]
pub struct Benchmarks {
    /// Total time taken to run all benchmarks
    pub total_time: Duration,
    /// The system specifications of the machine running the benchmarks.
    pub system_specs: SystemSpecs,
    /// The benchmarks data that was collected.
    pub benchmarks: Vec<Benchmark>,
    /// The forc version
    pub forc_version: String,
    /// The compiler hash
    pub compiler_hash: String,
    /// The time that the benchmarks were run
    pub benchmarks_datetime: String,
}

/// A collection of system hardware specifications.
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SystemSpecs {
    /// The global cpu usage of the system.
    #[serde(skip_serializing, skip_deserializing)]
    pub global_cpu_usage: f64,
    /// The cpus of the system.
    pub cpus: Vec<Cpu>,
    /// The physical core count of the system.
    pub physical_core_count: i64,
    /// The total memory of the system.
    pub total_memory: i64,
    /// The free memory of the system.
    pub free_memory: i64,
    /// The available memory of the system.
    pub available_memory: i64,
    /// The used memory of the system.
    pub used_memory: i64,
    /// The total swap of the system.
    pub total_swap: i64,
    /// The free swap of the system.
    pub free_swap: i64,
    /// The used swap of the system.
    pub used_swap: i64,
    /// The uptime of the system.
    pub uptime: i64,
    /// The boot time of the system.
    pub boot_time: i64,
    /// The load average of the system.
    pub load_average: LoadAverage,
    /// The name of the system.
    pub name: String,
    /// The kernel version of the system.
    pub kernel_version: String,
    /// The os version of the system.
    pub os_version: String,
    /// The long os version of the system.
    pub long_os_version: String,
    /// The distribution id of the system.
    pub distribution_id: String,
    /// The host name of the system.
    pub host_name: String,
}

/// A collection of specifications for a single cpu.
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Cpu {
    #[serde(skip_serializing, skip_deserializing)]
    /// The usage of the cpu at the time of querying.
    pub cpu_usage: f64,
    /// The name of the cpu.
    pub name: String,
    /// The vendor id of the cpu.
    pub vendor_id: String,
    /// The brand of the cpu.
    pub brand: String,
    /// The frequency of the cpu.
    pub frequency: i64,
}

/// System load average specifications.
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoadAverage {
    /// The `one` of the load average.
    pub one: f64,
    /// The `five` of the load average.
    pub five: f64,
    /// The `fifteen` of the load average.
    pub fifteen: f64,
}

/// Benchmark metadata and phase-specific performance data.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Benchmark {
    /// The name of the benchmark.
    pub name: String,
    /// The path to the benchmark's project folder.
    pub path: PathBuf,
    /// The start time of the benchmark.
    pub start_time: Option<Duration>,
    /// The end time of the benchmark.
    pub end_time: Option<Duration>,
    /// The phases of the benchmark.
    pub phases: Vec<BenchmarkPhase>,
    /// The performance frames collected from the benchmark.
    pub frames: Arc<Mutex<Vec<BenchmarkFrame>>>,
    /// The bytecode information
    pub asm_information: Option<serde_json::Value>,
    /// The hyperfine information
    pub hyperfine: Option<serde_json::Value>,
}

/// A named collection of performance frames representing a single phase of a benchmark.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BenchmarkPhase {
    /// The name of the benchmark phase.
    pub name: String,
    /// The start time of the benchmark phase.
    pub start_time: Option<Duration>,
    /// The end time of the benchmark phase.
    pub end_time: Option<Duration>,
}

/// A single frame of performance information for a benchmark phase.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BenchmarkFrame {
    /// The time that the frame was captured.
    pub timestamp: Duration,
    /// The relative to the benchmark timestamp.
    pub relative_timestamp: Duration,
    /// The process-specific CPU usage at the time the frame was captured.
    pub cpu_usage: f32,
    /// The total process-specific memory usage (in bytes) at the time the frame was captured.
    pub memory_usage: u64,
    /// The total process-specific virtual memory usage (in bytes) at the time the frame was captured.
    pub virtual_memory_usage: u64,
    /// The total number of bytes the process has written to disk at the time the frame was captured.
    pub disk_total_written_bytes: u64,
    /// The number of bytes the process has written to disk since the last refresh at the time the frame was captured.
    pub disk_written_bytes: u64,
    /// The total number of bytes the process has read from disk at the time the frame was captured.
    pub disk_total_read_bytes: u64,
    /// The number of bytes the process has read from disk since the last refresh at the time the frame was captured.
    pub disk_read_bytes: u64,
}

impl BenchmarkFrame {
    /// The minimum duration of a performance frame.
    pub const MINIMUM_DURATION: Duration = Duration::from_millis(100);
}

impl Benchmark {
    /// Creates a new benchmark using the supplied `name` and `path`.
    #[inline]
    pub(crate) fn new<S: ToString, P: Into<PathBuf>>(name: &S, path: P) -> Self {
        Self {
            name: name.to_string(),
            path: path.into(),
            start_time: None,
            end_time: None,
            phases: vec![],
            frames: Arc::new(Mutex::new(Vec::new())),
            asm_information: None,
            hyperfine: None,
        }
    }

    /// Runs the benchmark.
    ///
    /// # Arguments
    ///
    /// * `epoch` - The epoch time of the benchmark.
    ///
    /// # Errors
    ///
    /// If the benchmark's path is not a directory.
    pub(crate) fn run(
        &mut self,
        epoch: &Instant,
        options: &crate::cli::Options,
        exec_path: &str,
    ) -> crate::error::Result<()> {
        // Ensure the benchmark's path is a directory we can run `forc build` in
        assert!(
            self.verify_path(),
            "Project directory \"{}\" does not contain a Toml file.",
            self.path.display()
        );

        // Set the start time of the benchmark
        self.start_time = Some(epoch.elapsed());

        let forc_path = std::fs::canonicalize(&options.forc_path).map_err(|e| wrap!(e.into()))?;

        // Spawn the `forc build` child command in the benchmark's directory
        // NOTE: stdin and stdout are piped so that we can use them to signal individual phases
        let mut command = Command::new(forc_path)
            .arg("build")
            .arg("--log-level")
            .arg("5")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .current_dir(self.path.clone())
            .spawn()
            .map_err(|e| wrap!(e.into()))?;

        // Create an unbounded channel to send/receive line strings between the readline thread and the main thread
        let (readline_tx, readline_rx) = unbounded();

        // Create an unbounded channel to send/receive STOP signals between the readline thread and the main thread
        let (stop_readline_tx, stop_readline_rx) = unbounded();

        // Get the pid of the spawned child command's process
        let pid = Pid::from_u32(command.id());

        // Create a channel to send/receive STOP signals between the perf thread and the main thread
        let (stop_perf_tx, stop_perf_rx) = unbounded();

        let phase_epoch = Instant::now();
        Self::spawn_perf_thread(
            epoch,
            &phase_epoch,
            pid,
            stop_perf_rx,
            stop_readline_rx.clone(),
            self.frames.clone(),
        );

        // Spawn a thread to read lines from the command's stdout without blocking the main thread
        Self::spawn_readline_thread(&mut command, stop_readline_rx, readline_tx)
            .map_err(|e| wrap!(e))?;

        #[cfg(target_os = "linux")]
        let mut perf_process = if options.flamegraph {
            Some(
                Command::new("perf")
                    .arg("record")
                    .arg("--call-graph")
                    .arg("dwarf")
                    .arg("-p")
                    .arg(pid.to_string())
                    .spawn()
                    .map_err(|e| wrap!(e.into()))?,
            )
        } else {
            None
        };

        #[cfg(target_os = "macos")]
        // Create a channel to signal the sampling thread to stop
        let (sample_stop_tx, sample_stop_rx): (Sender<()>, Receiver<()>) = unbounded();

        #[cfg(target_os = "macos")]
        let sample_output = if options.flamegraph {
            println!("Starting sample command for flamegraph generation");
            Some(std::thread::spawn(move || {
                let sample_output = Self::run_sample(pid.as_u32()).map_err(|e| wrap!(e)).ok();
                sample_stop_rx.recv().ok();
                sample_output
            }))
        } else {
            None
        };

        // Collect frames for each phase of the command
        self.wait(
            epoch,
            &mut command,
            &stop_readline_tx,
            &stop_perf_tx,
            &readline_rx,
        )
        .map_err(|e| wrap!(e))?;

        #[cfg(target_os = "macos")]
        // Signal the sampling thread to stop
        let _ = sample_stop_tx.send(());

        // Set the end time of the benchmark
        self.end_time = Some(epoch.elapsed());

        #[cfg(target_os = "macos")]
        if let Some(sample_output) = sample_output {
            if let Ok(sample_output) = sample_output.join() {
                if let Some(sample_output) = sample_output {
                    // Collapse the sample output
                    let mut collapsed = Vec::new();
                    let mut folder = Folder::default();
                    let reader = BufReader::new(&sample_output[..]);
                    let writer = BufWriter::new(&mut collapsed);

                    folder
                        .collapse(reader, writer)
                        .map_err(|e| wrap!(e.into()))?;

                    let flamegraph_folder = exec_path
                        .strip_suffix(".json")
                        .ok_or_else(|| wrap!("Failed to strip suffix".into()))?;

                    let flamegraph_folder = flamegraph_folder.replace(
                        crate::BENCHMARKS_RUN_FOLDER,
                        crate::BENCHMARKS_FLAMEGRAPH_FOLDER,
                    );

                    let flamegraph_folder = std::path::Path::new(&flamegraph_folder);

                    if !flamegraph_folder.exists() {
                        // Create the flamegraph folder
                        std::fs::create_dir(flamegraph_folder).map_err(|e| wrap!(e.into()))?;
                    }

                    let file_name = format!("{}.svg", self.name,);

                    // Create the flamegraph folder
                    let output_file_path = flamegraph_folder.join(file_name);

                    let output_file =
                        std::fs::File::create(&output_file_path).map_err(|e| wrap!(e.into()))?;

                    let mut writer = BufWriter::new(output_file);
                    let reader = BufReader::new(&collapsed[..]);

                    from_reader(
                        &mut inferno::flamegraph::Options::default(),
                        reader,
                        &mut writer,
                    )
                    .map_err(|e| wrap!(e.into()))?;

                    println!("Flamegraph generated at {}", output_file_path.display());
                }
            }
        }

        #[cfg(target_os = "linux")]
        if let Some(mut perf) = perf_process.take() {
            let _ = perf.wait();

            let perf_script_output = {
                let out = Command::new("perf")
                    .arg("script")
                    .stdout(Stdio::piped())
                    .spawn()
                    .map_err(|e| wrap!(e.into()))?
                    .wait_with_output()
                    .map_err(|e| wrap!(e.into()))?;

                if !out.status.success() {
                    return Err(Box::new(wrap!("perf script failed".into())));
                }
                out
            };

            // Collapse the perf script output
            let mut collapsed = Vec::new();
            let mut folder = Folder::default();
            let reader = BufReader::new(&perf_script_output.stdout[..]);
            let writer = BufWriter::new(&mut collapsed);

            folder
                .collapse(reader, writer)
                .map_err(|e| wrap!(e.into()))?;

            let flamegraph_folder = exec_path
                .strip_suffix(".json")
                .ok_or_else(|| wrap!("Failed to strip suffix".into()))?;

            let flamegraph_folder = flamegraph_folder.replace(
                crate::BENCHMARKS_RUN_FOLDER,
                crate::BENCHMARKS_FLAMEGRAPH_FOLDER,
            );

            let flamegraph_folder = std::path::Path::new(&flamegraph_folder);

            if !flamegraph_folder.exists() {
                // Create the flamegraph folder
                std::fs::create_dir(flamegraph_folder).map_err(|e| wrap!(e.into()))?;
            }

            let file_name = format!("{}.svg", self.name,);

            // Create the flamegraph folder
            let output_file_path = flamegraph_folder.join(file_name);

            let output_file =
                std::fs::File::create(&output_file_path).map_err(|e| wrap!(e.into()))?;

            let mut writer = BufWriter::new(output_file);
            let reader = BufReader::new(&collapsed[..]);

            from_reader(
                &mut inferno::flamegraph::Options::default(),
                reader,
                &mut writer,
            )
            .map_err(|e| wrap!(e.into()))?;
        }

        Ok(())
    }

    #[cfg(target_os = "macos")]
    fn run_sample(pid: u32) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let output = Command::new("sample")
            .arg(pid.to_string())
            .arg("10")
            .arg("-file")
            .arg("/dev/stdout")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .map_err(|e| wrap!(e.into()))?;

        if !output.status.success() {
            // Capture and print the standard error output
            let stderr = String::from_utf8_lossy(&output.stderr);
            eprintln!("sample command failed: {stderr}");
            return Err(Box::new(wrap!("sample command failed".into())));
        }

        // Debugging output to check if any stacks were captured
        let stdout = String::from_utf8_lossy(&output.stdout);
        if stdout.contains("No stack") {
            eprintln!("No stacks found in sample output");
        } else {
            eprintln!("sample output captured successfully");
        }

        Ok(output.stdout)
    }

    /// Verifies that the benchmark's path is valid.
    #[must_use]
    pub(crate) fn verify_path(&self) -> bool {
        // Ensure the benchmark's path exists
        if !self.path.exists() {
            return false;
        }

        // Ensure the benchmark's path is a directory
        if !self.path.is_dir() {
            return false;
        }

        // Ensure the benchmark's directory contains a `Forc.toml` file
        let mut toml_path = self.path.clone();
        toml_path.push("Forc.toml");

        if !toml_path.is_file() {
            return false;
        }
        true
    }

    /// Spawns a thread to read lines from the command's stdout without blocking the main thread.
    fn spawn_readline_thread(
        command: &mut Child,
        stop_readline_rx: Receiver<()>,
        readline_tx: Sender<String>,
    ) -> crate::error::Result<()> {
        let command_stdout = command.stdout.take().ok_or(wrap!(
            "Failed to take stdout for spawn_readline_thread command".into()
        ))?;

        std::thread::spawn(move || {
            // Wrap the stdout of the child command in a BufReader and move it into the readline thread
            let command_stdout = std::io::BufReader::new(command_stdout);

            for line in command_stdout.lines() {
                let line = line.unwrap().trim_end().to_string();

                // Attempt to send the line to the main thread, or stop looping and allow
                // the readline thread to exit if it fails
                if readline_tx.send(line).is_err() {
                    break;
                }

                // If we receive a STOP signal, stop looping and allow the readline thread to exit
                if stop_readline_rx.try_recv().is_ok() {
                    break;
                }
            }
        });
        Ok(())
    }

    /// Collects frames for each phase of the command.
    fn wait(
        &mut self,
        epoch: &Instant,
        command: &mut Child,
        stop_readline_tx: &Sender<()>,
        stop_perf_tx: &Sender<()>,
        readline_rx: &Receiver<String>,
    ) -> crate::error::Result<()> {
        // Loop until the command has exited
        loop {
            // If the command has exited, tell the readline thread to stop and stop looping
            if command.try_wait().map_err(|e| wrap!(e.into()))?.is_some() {
                if stop_readline_tx.send(()).is_err() {
                    break;
                }

                if stop_perf_tx.send(()).is_err() {
                    break;
                }

                break;
            }

            // Attempt to receive a line from the readline thread
            let Ok(line) = readline_rx.try_recv() else {
                continue;
            };

            let line = line.trim_start();

            if line.starts_with("/dyno start ") {
                // Get the name of the phase from the end of the line
                let name = line.trim_start_matches("/dyno start ").trim_end();

                // Add the phase to the current benchmark
                self.phases.push(BenchmarkPhase {
                    name: name.into(),
                    start_time: Some(epoch.elapsed()),
                    end_time: None,
                });
            } else if line.starts_with("/dyno stop ") {
                // Get the name of the phase from the end of the line
                let name = line.trim_start_matches("/dyno stop ").trim_end();

                // Get the current benchmark phase
                let phase = self
                    .phases
                    .iter_mut()
                    .rev()
                    .find(|phase| name == phase.name)
                    .ok_or(wrap!("Failed to find phase".into()))?;

                // Ensure the received name matches the name of the current phase
                assert!(
                    name == phase.name,
                    "Received phase name \"{}\" does not match current phase name \"{}\"",
                    name,
                    phase.name,
                );

                // Set the end time of the benchmark
                phase.end_time = Some(epoch.elapsed());
            } else if line.starts_with("/dyno info ") {
                let asm_information: &str = line.trim_start_matches("/dyno info ").trim_end();
                self.asm_information =
                    Some(serde_json::from_str(asm_information).map_err(|e| wrap!(e.into()))?);
            }
        }

        Ok(())
    }

    /// Spawns a thread to collect performance frames for the command.
    fn spawn_perf_thread(
        epoch: &Instant,
        phase_epoch: &Instant,
        pid: sysinfo::Pid,
        stop_perf_rx: Receiver<()>,
        stop_readline_rx: Receiver<()>,
        frames: Arc<Mutex<Vec<BenchmarkFrame>>>,
    ) {
        let epoch = *epoch;
        let phase_epoch = *phase_epoch;

        let mut system = sysinfo::System::new();

        let num_cpus = {
            system.refresh_cpu_all();
            system.cpus().len()
        };

        let refresh_kind = sysinfo::ProcessRefreshKind::new()
            .with_cpu()
            .with_memory()
            .with_disk_usage();

        std::thread::spawn(move || loop {
            let frame_start = std::time::Instant::now();

            // If we receive a STOP signal, stop looping and allow the perf thread to exit
            if stop_perf_rx.try_recv().is_ok() {
                break;
            }

            if stop_readline_rx.try_recv().is_ok() {
                break;
            }

            // Remove this when this issue [#1315](https://github.com/GuillaumeGomez/sysinfo/issues/1351) has been resolved
            #[cfg(target_os = "linux")]
            system.refresh_all();

            if system.refresh_processes_specifics(
                sysinfo::ProcessesToUpdate::Some(&[pid]),
                true,
                refresh_kind,
            ) != 1
            {
                break;
            }

            let Some(process) = system.process(pid) else {
                panic!("Failed to find process with pid {pid}");
            };

            let cpu_usage = process.cpu_usage() / num_cpus as f32;
            let memory_usage = process.memory();
            let virtual_memory_usage = process.virtual_memory();
            let disk_usage = process.disk_usage();

            frames.lock().unwrap().push(BenchmarkFrame {
                timestamp: frame_start.duration_since(epoch),
                relative_timestamp: frame_start.duration_since(phase_epoch),
                cpu_usage,
                memory_usage,
                virtual_memory_usage,
                disk_total_written_bytes: disk_usage.total_written_bytes,
                disk_written_bytes: disk_usage.written_bytes,
                disk_total_read_bytes: disk_usage.total_read_bytes,
                disk_read_bytes: disk_usage.read_bytes,
            });

            let frame_elapsed = frame_start.elapsed();

            // Ensure that we don't loop any faster than the minimum frame duration
            if frame_elapsed < BenchmarkFrame::MINIMUM_DURATION {
                std::thread::sleep(BenchmarkFrame::MINIMUM_DURATION - frame_elapsed);
            }
        });
    }
}
