#![warn(clippy::all, clippy::pedantic)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::similar_names)]
#![allow(clippy::struct_field_names)]
#![allow(clippy::struct_excessive_bools)]
#![allow(clippy::too_many_lines)]

mod cli;
mod database;
mod error;
mod hyperfine;
pub mod stats;
pub mod types;
mod utils;

use clap::Parser;
pub use error::Result;

const BENCHMARKS_RUN_FOLDER: &str = "runs";
const BENCHMARKS_STATS_FOLDER: &str = "stats";
const BENCHMARKS_FLAMEGRAPH_FOLDER: &str = "flamegraphs";

const EXPORT_FILE_TYPE_JSON: &str = "json";

#[tokio::main]
pub async fn main() -> Result<()> {
    crate::utils::print_welcome();
    let options = cli::Options::parse();
    execute(&options).await.map_err(|e| wrap!(e))?;

    Ok(())
}

/// Execute the benchmarking process.
///
/// # Arguments
///
/// * `options` - A reference to a `cli::Options`.
///
/// # Errors
///
/// If the setup of the system fails.
///
/// If the generation of the benchmarks fails.
///
/// If the running of the benchmarks fails.
///
/// If the storage of the benchmarks fails.
///
/// If the setup of the database fails.
///
/// If the retrieval of the table count fails.
///
/// If the creation of the schema fails.
///
/// If the retrieval of the latest benchmarks fails.
///
/// If the calculation of the performance regression or improvements fails.
///
/// If the insertion of the new benchmarks into the database fails.
///
/// If the hyperfine analysis fails.
///
pub async fn execute(options: &cli::Options) -> Result<()> {
    // Setup the benchmarking environment
    utils::setup_system(options).map_err(|e| wrap!(e))?;

    let forc_version = utils::get_forc_version(&options.forc_path).map_err(|e| wrap!(e))?;

    let compiler_hash = utils::compute_md5(&options.forc_path).map_err(|e| wrap!(e))?;

    // Get the system specifications
    let system_specs = utils::system_specs().map_err(|e| wrap!(e))?;

    // Get the target path by resolving the canonical path
    let target_path = std::fs::canonicalize(&options.target).map_err(|e| wrap!(e.into()))?;

    // Create a mutable array of new benchmarks to be performed
    let mut current_benchmarks = utils::generate_benchmarks(target_path).map_err(|e| wrap!(e))?;

    let benchmarks_datetime = utils::get_date_time();

    let run_path = format!(
        "{}/{}/{}_{}_{}.json",
        options.output_folder.display(),
        BENCHMARKS_RUN_FOLDER,
        forc_version,
        compiler_hash,
        benchmarks_datetime
    );

    // Get the program-specific epoch
    let epoch = std::time::Instant::now();

    // Run all of the benchmarks
    for benchmark in &mut current_benchmarks {
        println!("Currently profiling : {}", benchmark.path.display());
        benchmark
            .run(&epoch, options, &run_path)
            .map_err(|e| wrap!(e))?;
    }

    // Get the end time of the entire benchmarking process
    let end_time = std::time::Instant::now();

    // Create a new benchmarks struct
    let benchmarks = types::Benchmarks {
        total_time: end_time.duration_since(epoch),
        system_specs,
        benchmarks: current_benchmarks.clone(),
        forc_version: forc_version.clone(),
        compiler_hash: compiler_hash.clone(),
        benchmarks_datetime: benchmarks_datetime.clone(),
    };

    let mut previous_benchmarks = String::new();

    // Get the number of files in the output directory
    let output_dir_file_count = utils::get_files_in_dir(
        &options.output_folder.join(BENCHMARKS_RUN_FOLDER),
        EXPORT_FILE_TYPE_JSON,
    )
    .map_err(|e| wrap!(e))?
    .len();

    // If headless mode is enabled and we have previous benchmarks we need to store the latest one before we create new one
    if output_dir_file_count > 0 {
        let file_path = utils::read_latest_file_in_directory(
            &options.output_folder.join(BENCHMARKS_RUN_FOLDER),
        )
        .map_err(|e| wrap!(e))?;
        previous_benchmarks = std::fs::read_to_string(file_path).map_err(|e| wrap!(e.into()))?;
    }

    // Store the benchmark results
    utils::store_item(&benchmarks, &run_path).map_err(|e| wrap!(e))?;

    if output_dir_file_count > 0 {
        println!("Calculating performance regression or improvements");
        let mut stats_result = stats::Collection::default();

        // Deserialize the previous benchmarks
        let previous_benchmarks: types::Benchmarks =
            serde_json::from_str(&previous_benchmarks).map_err(|e| wrap!(e.into()))?;

        // Calculate the performance regression or improvements
        for (previous, current) in previous_benchmarks
            .benchmarks
            .iter()
            .zip(current_benchmarks.iter())
        {
            let stats = stats::calculate(previous, current)?;
            stats_result
                .0
                .push((previous.path.display().to_string(), stats));
        }

        let stats_path = format!(
            "{}/{}/{}_{}_{}.json",
            options.output_folder.display(),
            BENCHMARKS_STATS_FOLDER,
            forc_version,
            compiler_hash,
            benchmarks_datetime
        );

        utils::store_item(&stats_result, &stats_path).map_err(|e| wrap!(e))?;

        if options.print_output {
            utils::print_stats(
                &stats_result,
                &previous_benchmarks.benchmarks,
                &current_benchmarks,
            )
            .map_err(|e| wrap!(e))?;
        }
    }

    if options.database {
        // Setup the database and get the client
        let client = database::setup().await.map_err(|e| wrap!(e))?;

        // Check if we already have benchmarks in the database
        if database::get_table_count(&client)
            .await
            .map_err(|e| wrap!(e))?
            == 0
        {
            println!("Creating the database schema");

            // Create the schema in the database
            database::create_schema(&client)
                .await
                .map_err(|e| wrap!(e))?;

            // Insert the new benchmarks into the database
            database::insert_benchmarks(&client, &benchmarks)
                .await
                .map_err(|e| wrap!(e))?;
        } else {
            let mut stats_collection = stats::Collection::default();
            // Get the latest benchmarks from the database so we can compare the results
            let previous_benchmarks = database::get_latest_benchmarks(&client)
                .await
                .map_err(|e| wrap!(e))?;

            // Calculate the performance regression or improvements
            for (previous, current) in previous_benchmarks
                .benchmarks
                .iter()
                .zip(current_benchmarks.iter())
            {
                let stats = stats::calculate(previous, current).map_err(|e| wrap!(e))?;
                stats_collection
                    .0
                    .push((previous.path.display().to_string(), stats));
            }
            database::insert_stats(&client, &stats_collection)
                .await
                .map_err(|e| wrap!(e))?;

            // Insert the new benchmarks into the database
            database::insert_benchmarks(&client, &benchmarks)
                .await
                .map_err(|e| wrap!(e))?;
        }
    }

    // If enabled run the hyperfine analysis
    if options.hyperfine {
        for b in &benchmarks.benchmarks {
            println!("Running hyperfine analysis on {}", b.path.display());
            hyperfine::execute(
                &b.path,
                options,
                &benchmarks_datetime,
                &forc_version,
                &compiler_hash,
            )
            .map_err(|e| wrap!(e))?;
        }
    }

    Ok(())
}
