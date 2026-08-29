use std::mem;

use cli_master_daemon::MAX_FRAME_LENGTH;
use thiserror::Error;

/// Size of the big-endian length prefix that precedes every JSON payload.
pub const LENGTH_PREFIX_BYTES: usize = mem::size_of::<u32>();

/// Failure while encoding or decoding a length-delimited daemon frame.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum FrameError {
    /// The declared payload length was zero.
    #[error("daemon frame length must not be zero")]
    Empty,
    /// The declared or encoded payload exceeded [`MAX_FRAME_LENGTH`].
    #[error("daemon frame length {length} exceeds limit {max}")]
    TooLarge {
        /// Requested payload size in bytes, excluding the length prefix.
        length: usize,
        /// Configured maximum payload size.
        max: usize,
    },
    /// A previous framing error poisoned this decoder.
    #[error("daemon frame decoder is closed after a protocol violation")]
    Failed,
}

/// Incremental decoder for the daemon's 4-byte big-endian length-prefixed frames.
///
/// The decoder rejects an oversize length as soon as the header is complete. It
/// never allocates a buffer for a payload that exceeds [`MAX_FRAME_LENGTH`].
#[derive(Debug, Default)]
pub struct FrameDecoder {
    buffer: Vec<u8>,
    expected: Option<usize>,
    failed: bool,
}

impl FrameDecoder {
    /// Creates an empty decoder.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends bytes and returns every complete payload that can be extracted.
    ///
    /// # Errors
    ///
    /// Returns [`FrameError`] when a length prefix is zero, exceeds the limit,
    /// or this decoder was already poisoned.
    pub fn push(&mut self, incoming: &[u8]) -> Result<Vec<Vec<u8>>, FrameError> {
        if self.failed {
            return Err(FrameError::Failed);
        }
        self.buffer.extend_from_slice(incoming);
        let mut frames = Vec::new();
        while let Some(frame) = self.take_frame()? {
            frames.push(frame);
        }
        Ok(frames)
    }

    /// Number of undecoded bytes currently buffered, including a partial header.
    #[cfg(test)]
    #[must_use]
    pub fn buffered_len(&self) -> usize {
        self.buffer.len()
    }

    fn take_frame(&mut self) -> Result<Option<Vec<u8>>, FrameError> {
        if self.expected.is_none() {
            if self.buffer.len() < LENGTH_PREFIX_BYTES {
                return Ok(None);
            }
            let mut header = [0_u8; LENGTH_PREFIX_BYTES];
            header.copy_from_slice(&self.buffer[..LENGTH_PREFIX_BYTES]);
            let length = usize::try_from(u32::from_be_bytes(header)).unwrap_or(usize::MAX);
            if length == 0 {
                return Err(self.fail(FrameError::Empty));
            }
            if length > MAX_FRAME_LENGTH {
                return Err(self.fail(FrameError::TooLarge {
                    length,
                    max: MAX_FRAME_LENGTH,
                }));
            }
            self.buffer.drain(..LENGTH_PREFIX_BYTES);
            self.expected = Some(length);
        }

        let Some(length) = self.expected else {
            return Ok(None);
        };
        if self.buffer.len() < length {
            return Ok(None);
        }
        let frame = self.buffer.drain(..length).collect();
        self.expected = None;
        Ok(Some(frame))
    }

    fn fail(&mut self, error: FrameError) -> FrameError {
        self.failed = true;
        self.buffer.clear();
        self.expected = None;
        error
    }
}

/// Encodes a payload as a length-prefixed daemon frame.
///
/// # Errors
///
/// Returns [`FrameError::Empty`] when `payload` is empty and
/// [`FrameError::TooLarge`] when it exceeds [`MAX_FRAME_LENGTH`].
pub fn encode_frame(payload: &[u8]) -> Result<Vec<u8>, FrameError> {
    if payload.is_empty() {
        return Err(FrameError::Empty);
    }
    if payload.len() > MAX_FRAME_LENGTH {
        return Err(FrameError::TooLarge {
            length: payload.len(),
            max: MAX_FRAME_LENGTH,
        });
    }
    let length = u32::try_from(payload.len()).map_err(|_| FrameError::TooLarge {
        length: payload.len(),
        max: MAX_FRAME_LENGTH,
    })?;
    let mut frame = Vec::with_capacity(LENGTH_PREFIX_BYTES + payload.len());
    frame.extend_from_slice(&length.to_be_bytes());
    frame.extend_from_slice(payload);
    Ok(frame)
}

#[cfg(test)]
mod tests {
    use cli_master_daemon::MAX_FRAME_LENGTH;

    use super::{FrameDecoder, FrameError, LENGTH_PREFIX_BYTES, encode_frame};

    #[test]
    fn partial_header_is_buffered_until_complete() {
        let mut decoder = FrameDecoder::new();
        let payload = br#"{"kind":"response"}"#;
        let encoded = encode_frame(payload).expect("frame should encode");

        assert!(decoder.push(&encoded[..1]).expect("byte 1").is_empty());
        assert!(decoder.push(&encoded[1..2]).expect("byte 2").is_empty());
        assert!(decoder.push(&encoded[2..3]).expect("byte 3").is_empty());
        assert_eq!(decoder.buffered_len(), 3);

        let frames = decoder
            .push(&encoded[3..])
            .expect("remaining bytes should complete the frame");
        assert_eq!(frames, vec![payload.to_vec()]);
        assert_eq!(decoder.buffered_len(), 0);
    }

    #[test]
    fn split_body_is_buffered_until_complete() {
        let mut decoder = FrameDecoder::new();
        let payload = br#"{"kind":"response","version":1}"#;
        let encoded = encode_frame(payload).expect("frame should encode");
        let header_and_one = LENGTH_PREFIX_BYTES + 1;

        assert!(
            decoder
                .push(&encoded[..header_and_one])
                .expect("header plus one body byte")
                .is_empty()
        );
        let frames = decoder
            .push(&encoded[header_and_one..])
            .expect("remaining body should complete the frame");
        assert_eq!(frames, vec![payload.to_vec()]);
    }

    #[test]
    fn oversize_length_is_rejected_without_buffering_the_body() {
        let mut decoder = FrameDecoder::new();
        let length = u32::try_from(MAX_FRAME_LENGTH.saturating_add(1)).expect("fits u32");
        let error = decoder
            .push(&length.to_be_bytes())
            .expect_err("oversize header should fail before any body arrives");
        assert_eq!(
            error,
            FrameError::TooLarge {
                length: MAX_FRAME_LENGTH + 1,
                max: MAX_FRAME_LENGTH,
            }
        );
        assert_eq!(decoder.buffered_len(), 0);
        assert_eq!(
            decoder
                .push(&[0_u8; 8])
                .expect_err("decoder stays poisoned"),
            FrameError::Failed
        );
    }

    #[test]
    fn zero_length_is_rejected() {
        let mut decoder = FrameDecoder::new();
        assert_eq!(
            decoder
                .push(&0_u32.to_be_bytes())
                .expect_err("empty frames are invalid"),
            FrameError::Empty
        );
    }

    #[test]
    fn encode_rejects_empty_and_oversize_payloads() {
        assert_eq!(encode_frame(&[]).expect_err("empty"), FrameError::Empty);
        assert_eq!(
            encode_frame(&vec![0_u8; MAX_FRAME_LENGTH + 1]).expect_err("oversize"),
            FrameError::TooLarge {
                length: MAX_FRAME_LENGTH + 1,
                max: MAX_FRAME_LENGTH,
            }
        );
    }

    #[test]
    fn two_frames_in_one_push_are_both_returned() {
        let mut decoder = FrameDecoder::new();
        let first = encode_frame(b"{\"a\":1}").expect("first");
        let second = encode_frame(b"{\"b\":2}").expect("second");
        let mut combined = first;
        combined.extend_from_slice(&second);
        let frames = decoder.push(&combined).expect("both frames");
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0], b"{\"a\":1}");
        assert_eq!(frames[1], b"{\"b\":2}");
    }
}
