use clap::Parser;
use std::path::PathBuf;

#[derive(Parser)]
#[clap(name = "Forc Performance Tool")]
#[clap(
    author = "Georgios Delkos <georgios@tenbeo.io>, Camden Smallwood <camden-smallwood@gmail.com>"
)]
#[clap(version = "1.0")]
#[clap(about = "Fuel Orchestrator Performance Profiling & Benchmarking", long_about = None)]
pub struct Options {
    #[clap(short, long)]
    /// A path to a target folder or file to compile
    pub target: PathBuf,

    #[clap(short, long)]
    /// The path to the forc binary compiled with --features profiler
    pub forc_path: PathBuf,

    #[clap(short, long, default_value = "./benchmarks")]
    pub output_folder: PathBuf,

    #[clap[short, long, default_value = "false"]]
    /// Enable printing output (Optional)
    pub print_output: bool,

    #[clap(long)]
    /// Flamegraph support (Optional)
    pub flamegraph: bool,

    #[clap(long, requires = "flamegraph")]
    /// Only data for flamegraph (Optional)
    pub data_only: bool,

    #[clap(long)]
    /// Enable hyperfine analysis (Optional)
    pub hyperfine: bool,

    #[clap(long, requires = "hyperfine", default_value = "2")]
    /// Maximum iterations for hyperfine (Optional)
    pub max_iterations: u32,

    #[clap(short, long)]
    /// Database support (Optional)
    pub database: bool,
}
