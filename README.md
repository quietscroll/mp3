# mp3

[![Cargo Test](https://github.com/quietscroll/mp3/actions/workflows/cargo-test.yml/badge.svg)](https://github.com/quietscroll/mp3/actions/workflows/cargo-test.yml)

A lightweight Rust crate for managing and encoding MP3 audio in the QuietScroll audio pipeline.

It wraps MP3 encoded bytes in a type-safe `MP3` struct and provides an easy-to-use encoding function from 16-bit Mono PCM to MP3 using the LAME encoder (CBR 64 kbps, 24 kHz mono).

## Features

- **PCM to MP3 Encoding**: Encode raw Mono 16-bit LE PCM data (via the `pcm::PCM` type) into compliant MP3 audio.
- **ID3 Metadata Support**: Re-exports `Id3Tag` to attach metadata (title, artist, album, cover art, year, comment) to the generated MP3 file during the encoding process.
- **Serde integration**: Optional `serde` feature support to serialize/deserialize the `MP3` byte wrapper.
- **Timing Diagnostics**: Tracking and reporting of encoding and encoder builder instantiation duration.

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
mp3 = { path = "path/to/mp3" }
```

### Optional Features

- `serde`: Enables `Serialize` and `Deserialize` implementations for the `MP3` type.
  ```toml
  [dependencies]
  mp3 = { path = "path/to/mp3", features = ["serde"] }
  ```

## Usage

### Simple Encoding Example

```rust
use mp3::{MP3, Id3Tag};
use pcm::PCM;

fn main() -> Result<(), mp3::Error> {
    // 1. Prepare your raw Mono PCM (L16) data (2 bytes per sample)
    // 24 kHz Mono PCM has 24000 samples per second = 48000 bytes.
    let pcm_data = vec![0u8; 48000];
    let pcm = PCM::from(pcm_data);

    // 2. Define ID3 metadata tags
    let tag = Id3Tag {
        title: b"QuietScroll Podcast",
        artist: b"QuietScroll AI",
        album: b"Episodes",
        album_art: &[],
        year: b"2026",
        comment: b"Encoded using mp3 crate",
    };

    // 3. Encode PCM to MP3
    let (mp3, encoding_duration, builder_duration) = MP3::encode(&pcm, tag)?;

    println!("Successfully encoded {} bytes of MP3!", mp3.len());
    println!("Encoding took: {:?}", encoding_duration);

    // 4. Access underlying bytes
    let raw_bytes: Vec<u8> = mp3.into_inner();

    Ok(())
}
```

## Running Tests

To run the unit tests for this crate, execute:

```bash
CARGO_TARGET_DIR=target/gemini cargo nextest run -p mp3 --all-features
```
