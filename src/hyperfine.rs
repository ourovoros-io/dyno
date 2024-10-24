use crate::wrap;

/// Execute the hyperfine command
pub(crate) fn execute(
    execution_path: &std::path::Path,
    options: &crate::cli::Options,
    date_time: &str,
    forc_version: &str,
    binary_hash: &str,
) -> crate::error::Result<()> {
    // Construct the hyperfine folder path
    let hyperfine_storage_path = options.output_folder.join("hyperfine");

    // First lets check for hyperfine in the system
    let _ = std::process::Command::new("hyperfine")
        .arg("--version")
        .output()
        .map_err(|_| wrap!("Failed to get hyperfine installation. Please install hyperfine via https://github.com/sharkdp/hyperfine.git".into()))?;

    // Create the directory for the hyperfine results
    if !std::path::PathBuf::from(&hyperfine_storage_path).exists() {
        std::fs::create_dir(&hyperfine_storage_path).map_err(|e| wrap!(e.into()))?;
    }

    // Get the forc path from the options or use the default forc path
    let forc_path = std::fs::canonicalize(options.forc_path.clone())
        .map_err(|e| wrap!(e.into()))?
        .display()
        .to_string();

    // Construct the command string for hyperfine
    let command_string = format!("{forc_path} build --log-level 5");

    // Construct the filename for the hyperfine json file
    let filename = format!(
        "{}_hyperfine.json",
        execution_path
            .components()
            .last()
            .ok_or(wrap!(
                "Failed to get last component of the execution path for hyperfine".into()
            ))?
            .as_os_str()
            .to_str()
            .ok_or(wrap!("Failed to convert last component to str".into()))?
    );

    // Spawn the hyperfine command
    let mut hyperfine_command = std::process::Command::new("hyperfine")
        .arg("--warmup")
        .arg("3")
        .arg("-M")
        .arg(options.max_iterations.to_string())
        .arg(command_string)
        .arg("--export-json")
        .arg(filename.clone())
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .current_dir(execution_path)
        .spawn()
        .map_err(|e| wrap!(e.into()))?;

    hyperfine_command.wait().map_err(|e| wrap!(e.into()))?;

    let mut previous_hyperfine_path = std::path::PathBuf::new();

    // Get the items in the hyperfine folder
    let hyperfine_items_count =
        crate::utils::get_files_in_dir(&hyperfine_storage_path, crate::EXPORT_FILE_TYPE_JSON)
            .map_err(|e| wrap!(e))?
            .len();

    // If we have already have a recording in place
    if hyperfine_items_count > 0 {
        previous_hyperfine_path =
            crate::utils::read_latest_file_in_directory(&hyperfine_storage_path)
                .map_err(|e| wrap!(e))?;
    }

    // Construct the path for the current hyperfine output
    let current_hyperfine_path = format!(
        "{}/hyperfine/{forc_version}_{binary_hash}_{date_time}_{filename}",
        options.output_folder.display()
    );

    // Copy the hyperfine json file to the output folder
    let mut copy_file_command = std::process::Command::new("cp")
        .arg(execution_path.join(filename.clone()))
        .arg(&current_hyperfine_path)
        .spawn()
        .map_err(|e| wrap!(e.into()))?;

    copy_file_command.wait().map_err(|e| wrap!(e.into()))?;

    // If we have two or more files compare the last two
    if hyperfine_items_count > 0 {
        let mut hyperfine_compare_command = std::process::Command::new("hyperfine")
            .arg("--warmup")
            .arg("3")
            .arg("-M")
            .arg(options.max_iterations.to_string())
            .arg("-n")
            .arg(&previous_hyperfine_path)
            .arg(previous_hyperfine_path)
            .arg("-n")
            .arg(&current_hyperfine_path)
            .arg(current_hyperfine_path)
            .arg("-i")
            .spawn()
            .map_err(|e| wrap!(e.into()))?;

        hyperfine_compare_command
            .wait()
            .map_err(|e| wrap!(e.into()))?;
    }

    Ok(())
}
