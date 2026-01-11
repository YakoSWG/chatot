use std::path::PathBuf;

use clap::error::ErrorKind;
use clap::{Args, CommandFactory, Parser, Subcommand};
mod charmap;
mod decode;
mod encode;

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
        #[arg(short = 'm', long)]
        charmap: PathBuf,
        #[command(flatten)]
        source: BinarySource,
        #[command(flatten)]
        destination: TextSource,
        #[command(flatten)]
        settings: Settings,
    },
    /// Encrypt and encode text files to binary text archive
    Encode {
        /// Path to custom character map file
        #[arg(short = 'm', long)]
        charmap: PathBuf,
        #[command(flatten)]
        source: TextSource,
        #[command(flatten)]
        destination: BinarySource,
        #[command(flatten)]
        settings: Settings,
    },
    ///
    Format {
        /// Path to custom character map file
        #[arg(short = 'm', long)]
        charmap: PathBuf,
        #[command(flatten)]
        source: TextSource,
        #[command(flatten)]
        settings: Settings,
    },
}

#[derive(Args, Clone)]
#[group(required = true, multiple = false)]
pub struct BinarySource {
    /// Path(s) to the binary text archive(s)        
    #[arg(short='b', long, num_args = 1.., conflicts_with = "archive_dir")]
    pub archive: Option<Vec<std::path::PathBuf>>,
    /// Directory for archives
    #[arg(short = 'a', long, conflicts_with = "archive")]
    pub archive_dir: Option<std::path::PathBuf>,
}

#[derive(Args, Clone)]
#[group(required = true, multiple = false)]
pub struct TextSource {
    /// Path(s) to the text file(s)
    #[arg(short='t', long, num_args = 1.., conflicts_with = "text_dir")]
    pub txt: Option<Vec<std::path::PathBuf>>,
    /// Directory for text files
    #[arg(short = 'd', long, conflicts_with = "txt")]
    pub text_dir: Option<std::path::PathBuf>,
}

#[derive(Args, Clone)]
pub struct Settings {
    /// Read from JSON format
    #[arg(short = 'j', long, default_value_t = false)]
    json: bool,
    /// Language code for JSON input
    #[arg(short='l', long, default_value_t = String::from("en_US"), requires = "json")]
    lang: String,
    /// Process only files newer than existing outputs, also updates timestamps on source files
    #[arg(short = 'n', long = "newer", default_value_t = false)]
    pub newer_only: bool,
    /// Use same format as tool "msgenc" for encoding messages
    #[arg(long = "msgenc", default_value_t = false, conflicts_with = "json")]
    pub msgenc_format: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match &cli.commands {
        Commands::Decode {
            charmap,
            source,
            destination,
            settings,
        } => {
            // Ensure input isn't a directory when output is files
            if source.archive_dir.is_some() && destination.txt.is_some() {
                let mut cmd = Cli::command();
                cmd.error(
                    ErrorKind::ArgumentConflict,
                    "Cannot use archive directory with text file outputs",
                )
                .exit();
            }

            let charmap = charmap::read_charmap(charmap)?;

            decode::decode_archives(&charmap, source, destination, settings)
        }
        Commands::Encode {
            charmap,
            source,
            destination,
            settings,
        } => {
            // Ensure input isn't a directory when output is files
            if source.text_dir.is_some() && destination.archive.is_some() {
                let mut cmd = Cli::command();
                cmd.error(
                    ErrorKind::ArgumentConflict,
                    "Cannot use text directory with archive file outputs",
                )
                .exit();
            }

            let charmap = charmap::read_charmap(charmap)?;

            encode::encode_texts(&charmap, source, destination, settings)
        }
        Commands::Format {
            charmap: _charmap,
            source: _source,
            settings: _settings,
        } => {
            let mut cmd = Cli::command();
            cmd.error(
                ErrorKind::InvalidSubcommand,
                "Format command is not yet implemented",
            )
            .exit();
        }
    }
}
