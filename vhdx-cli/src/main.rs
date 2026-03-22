use clap::Parser;

#[derive(Parser)]
#[command(name = "vhdx-tool")]
#[command(about = "VHDX (Virtual Hard Disk v2) CLI tool")]
struct Cli {
    /// Path to VHDX file
    #[arg(short, long)]
    file: Option<String>,
}

fn main() {
    let cli = Cli::parse();

    println!("VHDX CLI Tool");

    if let Some(file) = cli.file {
        println!("File: {}", file);
    } else {
        println!("Use --file to specify a VHDX file");
    }
}
