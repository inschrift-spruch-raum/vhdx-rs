use std::path::Path;

pub fn cmd_repair(file: &Path, dry_run: bool) {
    use vhdx_rs::Error;
    use vhdx_rs::File;

    println!("Repairing VHDX file: {}", file.display());

    if dry_run {
        println!("Dry run mode - no changes will be made");
        // Check if log replay would be needed by opening read-only
        match File::open(file).finish() {
            Ok(vhdx_file) => {
                if vhdx_file.has_pending_logs() {
                    println!("\u{2713} File has pending log entries that would be replayed");
                } else {
                    println!("\u{2713} File does not require repair");
                }
                return;
            }
            Err(e) => {
                eprintln!("Error: {e}");
                std::process::exit(1);
            }
        }
    }

    // Open with write access to allow log replay
    match File::open(file).write().finish() {
        Ok(_) => {
            println!("\u{2713} File repaired successfully");
            println!("\u{2713} Log entries replayed");
        }
        Err(Error::LogReplayRequired) => {
            // This shouldn't happen since we opened with write access,
            // but handle it just in case
            eprintln!("Error: Unable to replay log entries");
            std::process::exit(1);
        }
        Err(e) => {
            eprintln!("Error repairing VHDX file: {e}");
            std::process::exit(1);
        }
    }
}
