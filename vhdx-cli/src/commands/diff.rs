use std::path::Path;

use crate::cli::DiffCommand;

pub fn cmd_diff(file: &Path, command: &DiffCommand) {
    use vhdx_rs::File;

    match File::open(file).finish() {
        Ok(vhdx_file) => {
            if vhdx_file.has_pending_logs() {
                eprintln!("Warning: File has pending log entries from an interrupted write.");
                eprintln!("         Run 'vhdx-tool repair <file>' to fix the file.");
                eprintln!();
            }

            match command {
                DiffCommand::Parent => {
                    if vhdx_file.has_parent() {
                        if let Ok(metadata) = vhdx_file.sections().metadata()
                            && let Some(locator) = metadata.items().parent_locator()
                        {
                            println!("Parent Locator Entries:");
                            for (i, entry) in locator.entries().iter().enumerate() {
                                if let Some(key) = entry.key(locator.key_value_data())
                                    && let Some(value) = entry.value(locator.key_value_data())
                                {
                                    println!("  [{i}] {key}: {value}");
                                }
                            }
                        }
                    } else {
                        println!("This is not a differencing disk (no parent)");
                    }
                }
                DiffCommand::Chain => {
                    println!("Disk Chain:");
                    println!("  -> {}", file.display());
                    if vhdx_file.has_parent() {
                        println!("     (has parent - chain traversal not yet implemented)");
                    } else {
                        println!("     (base disk)");
                    }
                }
            }
        }
        Err(e) => {
            eprintln!("Error opening VHDX file: {e}");
            std::process::exit(1);
        }
    }
}
