use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;
use yt_thumb::{ThumbnailDownloader, extract_video_id};

#[derive(Parser)]
#[command(
    name = "yt-thumb",
    about = "Download YouTube thumbnails at the highest available resolution"
)]
struct Args {
    /// YouTube URL or video ID
    url_or_id: String,

    /// Output file path (default: <video_id>.jpg)
    #[arg(short, long)]
    output: Option<PathBuf>,
}

fn main() -> Result<()> {
    let args = Args::parse();

    let video_id = extract_video_id(&args.url_or_id)?;
    let output = args
        .output
        .unwrap_or_else(|| PathBuf::from(format!("{}.jpg", video_id)));

    eprintln!("Fetching thumbnail for: {}", video_id);

    let downloader = ThumbnailDownloader::new();
    let (data, resolution) = downloader.download(&video_id)?;

    std::fs::write(&output, &data)?;
    eprintln!("Saved {} thumbnail → {}", resolution, output.display());

    Ok(())
}
