mod cli;
mod commands;
mod utils;

use clap::Parser;

use cli::{Cli, Commands};

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Info { file, format } => {
            commands::cmd_info(&file, &format);
        }
        Commands::Create {
            path,
            size,
            disk_type,
            block_size,
            parent,
        } => {
            commands::cmd_create(&path, size, &disk_type, block_size, parent.as_deref());
        }
        Commands::Check {
            file,
            repair,
            log_replay,
        } => {
            commands::cmd_check(&file, repair, log_replay);
        }
        Commands::Repair { file, dry_run } => {
            commands::cmd_repair(&file, dry_run);
        }
        Commands::Sections { file, section } => {
            commands::cmd_sections(&file, &section);
        }
        Commands::Diff { file, command } => {
            commands::cmd_diff(&file, &command);
        }
    }
}
