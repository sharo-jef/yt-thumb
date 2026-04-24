# yt-thumb

A CLI tool that downloads YouTube thumbnails at the highest available resolution.

## Installation

```sh
cargo install --path .
```

## Usage

```sh
yt-thumb <URL_OR_ID> [--output <FILE>]
```

**Arguments**

| Argument | Description |
|---|---|
| `URL_OR_ID` | YouTube URL or video ID |
| `-o, --output <FILE>` | Output file path (default: `<video_id>.jpg`) |

**Supported URL formats**

- Bare video ID: `dQw4w9WgXcQ`
- `https://www.youtube.com/watch?v=<ID>`
- `https://m.youtube.com/watch?v=<ID>`
- `https://youtu.be/<ID>`
- `https://www.youtube.com/embed/<ID>`
- `https://www.youtube.com/v/<ID>`
- `https://www.youtube.com/shorts/<ID>`

**Examples**

```sh
# From a watch URL
yt-thumb https://www.youtube.com/watch?v=dQw4w9WgXcQ

# From a short URL
yt-thumb https://youtu.be/dQw4w9WgXcQ

# Specify output file
yt-thumb dQw4w9WgXcQ --output thumbnail.jpg
```

## Resolution fallback

Thumbnails are fetched in descending quality order until one is available:

1. `maxres` — 1280×720
2. `sd` — 640×480
3. `hq` — 480×360
4. `mq` — 320×180
5. `default` — 120×90

## Development

```sh
cargo build --release
cargo test
```
