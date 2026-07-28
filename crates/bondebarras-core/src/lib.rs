//! bondebarras — audit and cleanup of GitHub organization resources.

pub mod model;
pub mod stale;

use std::process::ExitCode;

/// Entry point shared by the binary. Returns the process exit code.
pub fn run() -> ExitCode {
    println!("bondebarras");
    ExitCode::SUCCESS
}
