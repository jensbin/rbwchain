#![allow(clippy::needless_return)] // Style preference for clarity in this case

use clap::Parser;
use std::collections::HashMap;
use std::env;
use std::error::Error;
use std::ffi::OsString;
use std::io::Write;
use std::os::unix::process::ExitStatusExt;
use std::process::{Command, ExitStatus, Stdio};
use std::str;
use tempfile::NamedTempFile;
use which::which;

// --- Constants ---
const RBWCHAIN_PREFIX: &str = "[rbwchain]";

// --- Logging Abstraction ---

/// Prints a debug message to stderr if debug mode is enabled.
#[inline]
fn debug_eprintln(debug_enabled: bool, args: std::fmt::Arguments) {
    if debug_enabled {
        eprintln!("{} {}", RBWCHAIN_PREFIX, args);
    }
}

/// Prints a warning message to stderr if debug mode is enabled.
/// Warnings are treated like debug messages based on the requirement
/// to be quiet by default.
#[inline]
fn warn_eprintln(debug_enabled: bool, args: std::fmt::Arguments) {
    if debug_enabled {
        // Prefix warnings clearly, even in debug mode
        eprintln!("{} Warning: {}", RBWCHAIN_PREFIX, args);
    }
}

/// Prints an error message to stderr. Always printed.
#[inline]
fn error_eprintln(args: std::fmt::Arguments) {
    eprintln!("{} Error: {}", RBWCHAIN_PREFIX, args);
}

// --- Core Logic ---

/// Executes `rbw <secret_note>` and parses its stdout for environment variables.
/// Expects stdout to contain lines in the format "KEY=VALUE".
/// Returns a HashMap of the parsed variables.
fn get_secret_content_from_rbw(secret_note: &str) -> Result<String, Box<dyn Error>> {
    let rbw_cmd_display = format!("rbw get {}", secret_note); // For error messages
    let output = Command::new("rbw")
        .arg("get")
        .arg(secret_note)
        .stdout(Stdio::piped()) // Capture stdout
        .stderr(Stdio::piped()) // Capture stderr for error reporting
        .output() // Execute and wait
        .map_err(|e| format!("Failed to execute '{}': {}", rbw_cmd_display, e))?;

    // Check if the command executed successfully
    if !output.status.success() {
        let stderr_output = String::from_utf8_lossy(&output.stderr);
        // Use the dedicated error printer
        // Note: We return the error here, so the message might be printed again by the caller,
        // but having it here gives immediate context if rbw fails.
        // Consider if double printing is acceptable or if the error message should be
        // constructed and returned for the main handler to print once.
        // For now, let's keep it simple and potentially print twice in case of error.
        error_eprintln(format_args!(
            "Command '{}' failed with status {}: {}",
            rbw_cmd_display,
            output.status,
            stderr_output.trim()
        ));
        return Err(format!(
            "Command '{}' failed with status {}: {}",
            rbw_cmd_display,
            output.status,
            stderr_output.trim()
        )
        .into()); // Convert String to Box<dyn Error>
    }

    // Parse the standard output as a UTF-8 string
    let stdout_str = String::from_utf8(output.stdout)
        .map_err(|e| format!("Output of '{}' is not valid UTF-8: {}", rbw_cmd_display, e))?;

    Ok(stdout_str)
}

/// Parses a string containing lines in "KEY=VALUE" format into a HashMap.
/// Skips empty lines, comments (#), and lines without '='.
/// Uses `warn_eprintln` for skippable lines, controlled by the `debug_enabled` flag.
fn parse_env_vars(
    content: &str,
    debug_enabled: bool,
) -> Result<HashMap<String, String>, Box<dyn Error>> {
    let mut env_vars = HashMap::new();
    for line in content.lines() {
        // Skip empty lines or lines potentially starting with # (comments)
        let trimmed_line = line.trim();
        if trimmed_line.is_empty() || trimmed_line.starts_with('#') {
            continue;
        }

        // Split the line at the first '='
        if let Some((key, value)) = trimmed_line.split_once('=') {
            // Basic trimming, consider more robust parsing if needed (e.g., handling quotes)
            let key = key.trim();
            let value = value.trim();
            if !key.is_empty() {
                // Ensure the key is not empty after trimming
                env_vars.insert(key.to_string(), value.to_string());
            } else {
                // Use the conditional warning printer
                warn_eprintln(
                    debug_enabled,
                    format_args!("Skipping line with empty key: '{}'", line),
                );
            }
        } else {
            // Use the conditional warning printer
            warn_eprintln(
                debug_enabled,
                format_args!("Skipping invalid line in env file content: '{}'", line),
            );
        }
    }
    Ok(env_vars)
}

// --- Command Line Argument Parsing ---
#[derive(Parser, Debug)]
#[command(
    name = "rbwchain",
    about = "Executes a command with secrets from rbw, either as environment variables or via a temporary file.",
    long_about = "This program reads secrets from a specified rbw note. \
By default, it parses the secrets as KEY=VALUE pairs and sets them as environment variables for the child command. \
If -f/--file is used, it writes the raw secret content to a temporary file and sets the specified environment variable to its path. \
Error messages are always printed to stderr. Use --debug for verbose output.",
    // Allows arguments like `-v` to be passed to the child command
    allow_hyphen_values = true,
    // Capture all trailing arguments for the child command
    trailing_var_arg = true
)]
struct Cli {
    /// The secret_note to read (using `rbw`)
    #[arg(required = true, value_name = "SECRET_NOTE")]
    secret_note: String,

    /// Provide secrets via a temporary file path set in an environment variable.
    /// Writes the raw secret content to a temp file and sets ENV_VAR_NAME=</path/to/tempfile>
    /// for the child command.
    #[arg(short = 'f', long = "file", value_name = "ENV_VAR_NAME")]
    file_env_var: Option<OsString>,

    /// Enable debug logging to stderr.
    #[arg(long, short = 'd', action = clap::ArgAction::SetTrue)]
    debug: bool,

    /// The command to execute
    #[arg(required = true)]
    command: OsString,

    /// Arguments to pass to the command
    #[arg(required = false)]
    args: Vec<OsString>,
}

// --- Main Logic ---
fn main() -> Result<(), Box<dyn Error>> {
    // --- Pre-flight Check: Ensure rbw exists ---
    if which("rbw").is_err() {
        // Use the dedicated error printer
        error_eprintln(format_args!(
            "The 'rbw' command was not found in your system's PATH."
        ));
        eprintln!(
            "{} Please ensure rbw (https://github.com/doy/rbw) is installed and accessible.",
            RBWCHAIN_PREFIX
        ); // Keep informational part separate
        std::process::exit(1);
    }

    // 1. Parse Command Line Arguments
    let cli = Cli::parse();
    let debug_enabled = cli.debug; // Store flag for easy access

    debug_eprintln(debug_enabled, format_args!("Debug mode enabled."));
    debug_eprintln(debug_enabled, format_args!("Parsed arguments: {:?}", cli));

    // 2. Fetch Secret Content (always needed)
    debug_eprintln(
        debug_enabled,
        format_args!("Fetching secret content for note: '{}'", cli.secret_note),
    );
    let secret_content = get_secret_content_from_rbw(&cli.secret_note).map_err(|e| {
        // Ensure the specific error is printed by the main error handler
        format!(
            "Error getting secret content from rbw for note '{}': {}",
            cli.secret_note, e
        )
    })?;
    debug_eprintln(
        debug_enabled,
        format_args!(
            "Successfully fetched {} bytes of secret content.",
            secret_content.len()
        ),
    );

    // 3. Set up the Command
    let mut command_to_run = Command::new(&cli.command);

    // Set the arguments for the command
    command_to_run.args(&cli.args);

    // Prepare environment variables map to be passed to the command
    let mut final_env_vars = HashMap::new();

    // Add standard wrapper variables first. These are always set.
    final_env_vars.insert(
        "RBWCHAIN_VERSION".into(), // Use .into() for OsString conversion if needed later
        OsString::from(env!("CARGO_PKG_VERSION")),
    );
    final_env_vars.insert(
        "RBWCHAIN_SECRET_NOTE".into(),
        OsString::from(&cli.secret_note),
    );
    if debug_enabled {
        // Only add RBWCHAIN_DEBUG if debug mode is active
        final_env_vars.insert("RBWCHAIN_DEBUG".into(), OsString::from("1"));
    }


    // Keep temp file alive until command finishes if using file mode
    let mut temp_file_guard: Option<NamedTempFile> = None;

    if let Some(env_var_name_os) = &cli.file_env_var {
        // --- File Mode ---
        // Convert OsString to String for validation and logging. Use lossy for robustness.
        let env_var_name = env_var_name_os.to_string_lossy();
        if env_var_name.is_empty() {
            // Use the dedicated error printer
            error_eprintln(format_args!(
                "Specified environment variable name for -f/--file option is empty."
            ));
            return Err("Empty environment variable name for file mode.".into());
        }

        debug_eprintln(
            debug_enabled,
            format_args!(
                "Using file mode. Setting environment variable '{}'.",
                env_var_name
            ),
        );

        // Create temp file
        let mut temp_file = NamedTempFile::new()
            .map_err(|e| format!("Failed to create temporary file: {}", e))?;
        debug_eprintln(
            debug_enabled,
            format_args!(
                "Created temporary file container at: {}",
                temp_file.path().display()
            ),
        );

        // Write content to temp file
        temp_file
            .write_all(secret_content.as_bytes())
            .map_err(|e| format!("Failed to write secret content to temporary file: {}", e))?;
        debug_eprintln(
            debug_enabled,
            format_args!("Wrote secret content to temporary file."),
        );

        // Ensure data is flushed to the OS buffer, making it readable by the child.
        temp_file
            .flush()
            .map_err(|e| format!("Failed to flush temporary file: {}", e))?;
        debug_eprintln(debug_enabled, format_args!("Flushed temporary file."));

        // Get the path as an OsString (needed for .env)
        let temp_file_path_os = temp_file.path().as_os_str().to_os_string();

        // Add the *specified* environment variable to the *path* of the temp file to the map.
        // Use the original OsString key provided by the user.
        final_env_vars.insert(env_var_name_os.clone(), temp_file_path_os.clone());

        // Move the temp_file into the guard to keep it alive.
        temp_file_guard = Some(temp_file);

        debug_eprintln(
            debug_enabled,
            format_args!(
                "Prepared environment variable: {}={}",
                env_var_name, // Log user-provided name lossily
                temp_file_path_os.to_string_lossy() // Log path lossily
            ),
        );
    } else {
        // --- Environment Variable Mode (Default Behavior) ---
        debug_eprintln(
            debug_enabled,
            format_args!("Using environment variable mode."),
        );

        // Parse the fetched content into environment variables
        // Pass the debug flag to control warnings during parsing
        let parsed_vars = parse_env_vars(&secret_content, debug_enabled)?;

        if parsed_vars.is_empty() && !secret_content.trim().is_empty() {
            // Only warn if the secret content wasn't empty but we didn't parse anything.
            // Use the conditional warning printer
            warn_eprintln(
                debug_enabled,
                format_args!(
                    "No valid 'KEY=VALUE' pairs found in secret note '{}'.",
                    cli.secret_note
                ),
            );
        }

        // Merge parsed vars into final_env_vars. Parsed vars take precedence if keys conflict.
        // Need to convert String key/value from parsed_vars to OsString for the final map.
        for (key, value) in parsed_vars {
            final_env_vars.insert(OsString::from(key), OsString::from(value));
        }

        // Calculate counts *after* merging
        let parsed_count = final_env_vars.len().saturating_sub(
              2 + if debug_enabled {1} else {0} // Subtract base vars + conditional debug var
            );
        let default_count = final_env_vars.len() - parsed_count;

        debug_eprintln(
            debug_enabled,
            format_args!(
                "Injecting {} environment variable(s) ({} parsed + {} standard).",
                final_env_vars.len(),
                parsed_count,
                default_count,
            ),
        );
         if debug_enabled {
            // Optionally log the keys being set (but not the values for security)
            let keys_str = final_env_vars
                .keys()
                .map(|k| k.to_string_lossy())
                .collect::<Vec<_>>()
                .join(", ");
            debug_eprintln(debug_enabled, format_args!("Variables set: [{}]", keys_str));
         }
    }

    // Set the environment variables for the command
    command_to_run.envs(&final_env_vars);

    // Ensure the child process inherits stdin, stdout, and stderr from the wrapper.
    command_to_run.stdin(Stdio::inherit());
    command_to_run.stdout(Stdio::inherit());
    command_to_run.stderr(Stdio::inherit());

    debug_eprintln(
        debug_enabled,
        format_args!(
            "Executing command: {} {}",
            cli.command.to_string_lossy(),
            cli.args
                .iter()
                .map(|a| a.to_string_lossy())
                .collect::<Vec<_>>()
                .join(" ")
        ),
    );

    // 4. Execute the Command and Handle Exit Status
    let status = command_to_run
        .status()
        .map_err(|e| format!("Failed to execute command '{}': {}", cli.command.to_string_lossy(), e))?;

    debug_eprintln(
        debug_enabled,
        format_args!("Command finished with status: {}", status),
    );

    // Explicitly drop the guard *after* the child process has finished.
    drop(temp_file_guard);
    if debug_enabled && cli.file_env_var.is_some() {
         debug_eprintln(debug_enabled, format_args!("Temporary file guard dropped."));
    }


    // Forward the exit code or signal termination status from the child process.
    // Pass the debug flag to control the "terminated by signal" message.
    handle_exit_status(status, debug_enabled);

    // Note: handle_exit_status never returns (it exits).
}

// --- Exit Status Handling ---
/// Handles the ExitStatus of the child process, exiting the wrapper
/// with the appropriate code or signal status.
/// Uses `debug_eprintln` for the signal termination message.
///
/// This function diverges (never returns).
fn handle_exit_status(status: ExitStatus, debug_enabled: bool) -> ! {
    // Check if the process exited normally
    if let Some(code) = status.code() {
        // Exit the wrapper with the same code as the child process
        std::process::exit(code);
    } else {
        // The process was terminated by a signal (Unix-specific)
        #[cfg(unix)]
        {
            if let Some(signal) = status.signal() {
                // As per convention (e.g., bash), exit code for signal termination
                // is 128 + signal number.
                let exit_code = 128 + signal;
                // Use the conditional debug printer for this message
                debug_eprintln(
                    debug_enabled,
                    format_args!(
                        "Child process terminated by signal {} (Exiting with code {})",
                        signal, exit_code
                    ),
                );
                std::process::exit(exit_code);
            } else {
                // Should not happen if code() is None on Unix, but handle defensively.
                // This is unexpected, treat as an error message.
                error_eprintln(format_args!(
                    "Child process terminated abnormally (unknown Unix reason)."
                ));
                std::process::exit(1); // General error
            }
        }
        #[cfg(not(unix))]
        {
            // On non-Unix platforms, if code() is None, it's an abnormal termination.
            // Treat as an error message.
            error_eprintln(format_args!("Child process terminated abnormally."));
            std::process::exit(1); // General error
        }
    }
}
