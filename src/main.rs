use std::path::PathBuf;

use clap::{Parser, Subcommand, Args, CommandFactory};
use clap::error::ErrorKind;
mod decode;
mod encode;
mod charmap;

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    commands: Commands,
}
#[derive(Subcommand)]
enum Commands {
    /// Decrypt and decode binary text archive to text files
    Decode {
        /// Path to custom character map file
        #[arg(short='m', long)]
        charmap: PathBuf,
        #[command(flatten)]
        source: BinarySource,
        #[command(flatten)]
        destination: TextSource,
        /// Output in JSON format
        #[arg(short='j', long, default_value_t = false)]
        json: bool,
        /// Language code for JSON output
        #[arg(short='l', long, default_value_t = String::from("en_US"), requires = "json")]
        lang: String,       
    },
    /// Encrypt and encode text files to binary text archive
    Encode {
        /// Path to custom character map file
        #[arg(short='m', long)]
        charmap: PathBuf,
        #[command(flatten)]
        source: TextSource,
        #[command(flatten)]
        destination: BinarySource,
        /// Read from JSON format
        #[arg(short='j', long, default_value_t = false)]
        json: bool,
        /// Language code for JSON input
        #[arg(short='l', long, default_value_t = String::from("en_US"), requires = "json")]
        lang: String,
    },
}

#[derive(Args, Clone)]
#[group(required = true, multiple = false)]
pub struct BinarySource {
    /// Path(s) to the binary text archive(s)        
    #[arg(short='b', long, num_args = 1.., conflicts_with = "archive_dir")]
    pub archive: Option<Vec<std::path::PathBuf>>,
    /// Directory for archives
    #[arg(short='a', long, conflicts_with = "archive")]
    pub archive_dir: Option<std::path::PathBuf>,
    
}

#[derive(Args, Clone)]
#[group(required = true, multiple = false)]
pub struct TextSource {
    /// Path(s) to the text file(s)
    #[arg(short='t', long, num_args = 1.., conflicts_with = "text_dir")]
    pub txt: Option<Vec<std::path::PathBuf>>,
    /// Directory for text files
    #[arg(short='d', long, conflicts_with = "txt")]
    pub text_dir: Option<std::path::PathBuf>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match &cli.commands {
        Commands::Decode {charmap, source, destination, json, lang: _lang } => {
            // Ensure input isn't a directory when output is files
            if source.archive_dir.is_some() && destination.txt.is_some() {
                let mut cmd = Cli::command();
                cmd.error(ErrorKind::ArgumentConflict,
                "Cannot use archive directory with text file outputs",
            )
            .exit();
            }

            let charmap = charmap::read_charmap(charmap)?;

            if *json {
                eprintln!("Warning: JSON input/output is not yet implemented, proceeding with plain text.");
            }


            decode::decode_archives(&charmap, source, destination)
        }
        Commands::Encode { charmap, source, destination, json , lang: _lang ,} => {
            // Ensure input isn't a directory when output is files
            if source.text_dir.is_some() && destination.archive.is_some() {
                let mut cmd = Cli::command();
                cmd.error(ErrorKind::ArgumentConflict,
                "Cannot use text directory with archive file outputs",
            )
            .exit();
            }

            let charmap = charmap::read_charmap(charmap)?;

            if *json {
                eprintln!("Warning: JSON input/output is not yet implemented, proceeding with plain text.");
            }

            encode::encode_texts(&charmap, source, destination)
        }
    }
}