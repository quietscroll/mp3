//! MP3 audio type and encoding for the audio pipeline.
//!
//! Provides the [`MP3`] newtype wrapping encoded MP3 bytes and [`encode`] for
//! converting L16 mono PCM to MP3 via LAME (CBR 64 kbps, 24 kHz mono).

#![deny(missing_docs, unreachable_pub)]

use std::ops::Deref;
use std::time::Instant;

use mp3lame_encoder::{Bitrate, Builder, DualPcm, FlushNoGap, Mode, Quality, VbrMode};
use pcm::PCM;
use time::Duration;
use tracing::info;

pub use mp3lame_encoder::Id3Tag;

/// Sample rate assumed by all [`MP3`] operations, in Hz (24 000).
pub const MP3_SAMPLE_RATE_HZ: u16 = 24_000;

/// Errors that can arise from MP3 encoding.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The PCM byte buffer has an odd length. L16 mono uses 2 bytes per sample.
    #[error("PCM byte length must be even (L16 mono: 2 bytes per sample)")]
    ByteLengthNotEven,
    /// LAME encoder builder failed.
    #[error("MP3 LAME builder error: {0:?}")]
    Build(#[from] mp3lame_encoder::BuildError),
    /// LAME encoder encode step failed.
    #[error("MP3 LAME encode error: {0:?}")]
    Encode(#[from] mp3lame_encoder::EncodeError),
}

/// Encoded MP3 audio bytes.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MP3(Vec<u8>);

impl MP3 {
    /// Wrap raw MP3 bytes.
    pub fn new(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }

    /// Consume the wrapper and return the inner byte vector.
    pub fn into_inner(self) -> Vec<u8> {
        self.0
    }

    /// Number of bytes.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// True when the buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Encode L16 mono PCM to MP3 using LAME (CBR 64 kbps, 24 kHz mono).
    ///
    /// Returns `(mp3, encoding_duration, builder_duration)`.
    pub fn encode(pcm: &PCM, tag: Id3Tag) -> Result<(MP3, Duration, Duration), Error> {
        encode_inner(pcm, tag)
    }
}

#[cfg(feature = "serde")]
impl MP3 {
    /// Encode this MP3 buffer as a base64 string (STANDARD alphabet).
    pub fn to_b64(&self) -> String {
        use base64::{Engine as _, engine::general_purpose::STANDARD};
        STANDARD.encode(&self.0)
    }

    /// Decode a base64 string (STANDARD alphabet) into a MP3 buffer.
    ///
    /// Returns [`base64::DecodeError`] when the input is not valid base64.
    pub fn from_b64(s: &str) -> Result<Self, base64::DecodeError> {
        use base64::{Engine as _, engine::general_purpose::STANDARD};
        Ok(Self(STANDARD.decode(s)?))
    }
}

/// Serde helpers for serialising [`MP3`] as a base64 string.
///
/// Use `#[serde(with = "pcm::b64")]` on a `MP3` field.
#[cfg(feature = "serde")]
pub mod b64 {
    use super::MP3;
    use serde::{Deserialize, Deserializer, Serializer, de::Error};

    /// Serialize `MP3` as a base64 string.
    pub fn serialize<S: Serializer>(pcm: &MP3, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&pcm.to_b64())
    }

    /// Deserialize `MP3` from a base64 string.
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<MP3, D::Error> {
        let raw = String::deserialize(d)?;
        MP3::from_b64(&raw).map_err(D::Error::custom)
    }
}

/// Serde helpers for serialising `Option<`[`MP3`]`>` as a nullable base64 string.
///
/// Use `#[serde(with = "pcm::b64_option")]` on an `Option<MP3>` field.
#[cfg(feature = "serde")]
pub mod b64_option {
    use super::MP3;
    use serde::{Deserialize, Deserializer, Serializer, de::Error};

    /// Serialize `Option<MP3>` as a base64 string or `null`.
    pub fn serialize<S: Serializer>(opt: &Option<MP3>, s: S) -> Result<S::Ok, S::Error> {
        match opt {
            Some(pcm) => s.serialize_str(&pcm.to_b64()),
            None => s.serialize_none(),
        }
    }

    /// Deserialize `Option<MP3>` from a base64 string or `null`.
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<MP3>, D::Error> {
        Option::<String>::deserialize(d)?
            .map(|raw| MP3::from_b64(&raw).map_err(D::Error::custom))
            .transpose()
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for MP3 {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_b64())
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for MP3 {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        use serde::de::Error;
        let raw = <String as serde::Deserialize>::deserialize(d)?;
        MP3::from_b64(&raw).map_err(D::Error::custom)
    }
}

impl Deref for MP3 {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        &self.0
    }
}

impl AsRef<[u8]> for MP3 {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl From<Vec<u8>> for MP3 {
    fn from(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }
}

impl From<MP3> for Vec<u8> {
    fn from(mp3: MP3) -> Self {
        mp3.0
    }
}

fn encode_inner(pcm: &PCM, tag: Id3Tag) -> Result<(MP3, Duration, Duration), Error> {
    if !pcm.len().is_multiple_of(2) {
        return Err(Error::ByteLengthNotEven);
    }

    let now = Instant::now();

    let pcm_samples = pcm.i16_samples();

    let mut builder = Builder::new().expect("Failed to allocate MP3 encoder builder");
    builder.set_num_channels(1)?;
    builder.set_sample_rate(MP3_SAMPLE_RATE_HZ as u32)?;
    builder.set_brate(Bitrate::Kbps64)?;
    builder.set_quality(Quality::Best)?;
    builder.set_mode(Mode::Mono)?;
    builder.set_vbr_mode(VbrMode::Off)?;
    let _ = builder.set_id3_tag(tag);

    let mut encoder = builder.build()?;
    let builder_elapsed = now.elapsed();

    let input = DualPcm {
        left: pcm_samples.as_slice(),
        right: pcm_samples.as_slice(),
    };

    let mut mp3_out =
        Vec::with_capacity(mp3lame_encoder::max_required_buffer_size(input.left.len()));

    let written = encoder.encode(input, mp3_out.spare_capacity_mut())?;
    unsafe { mp3_out.set_len(mp3_out.len().wrapping_add(written)) };

    let flushed = encoder.flush::<FlushNoGap>(mp3_out.spare_capacity_mut())?;
    unsafe { mp3_out.set_len(mp3_out.len().wrapping_add(flushed)) };

    let total_elapsed = now.elapsed();

    info!(
        "MP3 encoding completed in {:.2?} (builder: {:.2?})",
        total_elapsed, builder_elapsed
    );

    let encoding_duration = Duration::try_from(total_elapsed).unwrap_or(Duration::ZERO);
    let builder_duration = Duration::try_from(builder_elapsed).unwrap_or(Duration::ZERO);

    Ok((MP3::from(mp3_out), encoding_duration, builder_duration))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_tag() -> Id3Tag<'static> {
        Id3Tag {
            title: b"t",
            artist: b"a",
            album: b"al",
            album_art: &[],
            year: b"2026",
            comment: b"c",
        }
    }

    #[test]
    fn odd_pcm_byte_length_is_rejected() {
        let result = MP3::encode(&PCM::from(vec![0u8; 3]), dummy_tag());
        assert!(matches!(result, Err(Error::ByteLengthNotEven)));
    }

    #[test]
    fn empty_pcm_encodes_to_nonempty_mp3() {
        let result = MP3::encode(&PCM::default(), dummy_tag());
        assert!(result.is_ok());
        let (mp3, _, _) = result.unwrap();
        assert!(!mp3.is_empty());
    }

    #[test]
    fn mp3_new_and_conversions() {
        let bytes = vec![1, 2, 3, 4];
        let mp3 = MP3::new(bytes.clone());
        assert_eq!(mp3.len(), 4);
        assert!(!mp3.is_empty());
        assert_eq!(&*mp3, &[1, 2, 3, 4]); // Deref
        assert_eq!(mp3.as_ref(), &[1, 2, 3, 4]); // AsRef

        let vec_from_mp3: Vec<u8> = mp3.clone().into_inner();
        assert_eq!(vec_from_mp3, bytes);

        let mp3_from_vec = MP3::from(bytes.clone());
        assert_eq!(mp3_from_vec, mp3);

        let vec_from_mp3_conv = Vec::from(mp3);
        assert_eq!(vec_from_mp3_conv, bytes);

        let empty_mp3 = MP3::default();
        assert!(empty_mp3.is_empty());
        assert_eq!(empty_mp3.len(), 0);
    }

    #[test]
    fn encode_valid_pcm() {
        // Create 1 second of silent PCM (24000 samples * 2 bytes/sample = 48000 bytes)
        let sample_count = 24000;
        let pcm_bytes = vec![0u8; sample_count * 2];
        let pcm = PCM::from(pcm_bytes);

        let result = MP3::encode(&pcm, dummy_tag());
        assert!(result.is_ok());

        let (mp3, encoding_duration, builder_duration) = result.unwrap();
        assert!(!mp3.is_empty());

        assert!(encoding_duration >= Duration::ZERO);
        assert!(builder_duration >= Duration::ZERO);
    }

    #[cfg(feature = "serde")]
    #[test]
    fn test_serde_roundtrip() {
        let bytes = vec![1, 2, 3, 4];
        let mp3 = MP3::new(bytes);
        let serialized = serde_json::to_string(&mp3).expect("failed to serialize");
        let deserialized: MP3 = serde_json::from_str(&serialized).expect("failed to deserialize");
        assert_eq!(mp3, deserialized);
    }
}
