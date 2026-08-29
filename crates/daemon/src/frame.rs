//! Length-prefixed Unix socket frames.

use std::io::{self, Read, Write};

use serde::Serialize;

/// Maximum accepted JSON frame size.
pub const MAX_FRAME_BYTES: u32 = 8 * 1024 * 1024;

/// Reads one length-prefixed JSON frame.
///
/// # Errors
///
/// Returns an I/O error when the stream ends, the length is invalid, or the
/// body cannot be read.
pub fn read_frame(stream: &mut impl Read) -> io::Result<Vec<u8>> {
    let mut header = [0_u8; 4];
    stream.read_exact(&mut header)?;
    let length = u32::from_le_bytes(header);
    if length == 0 || length > MAX_FRAME_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid frame length",
        ));
    }
    let mut body = vec![0_u8; usize::try_from(length).map_err(io::Error::other)?];
    stream.read_exact(&mut body)?;
    Ok(body)
}

/// Writes one length-prefixed JSON frame.
///
/// # Errors
///
/// Returns an I/O error when the value cannot be serialized or the stream
/// rejects the write.
pub fn write_frame(stream: &mut impl Write, value: &impl Serialize) -> io::Result<()> {
    let body = serde_json::to_vec(value).map_err(io::Error::other)?;
    let length = u32::try_from(body.len()).map_err(io::Error::other)?;
    if length > MAX_FRAME_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "frame exceeds the daemon size limit",
        ));
    }
    stream.write_all(&length.to_le_bytes())?;
    stream.write_all(&body)?;
    stream.flush()
}

#[cfg(test)]
mod tests {
    use super::{read_frame, write_frame};
    use serde_json::json;
    use std::io::Cursor;

    #[test]
    fn round_trips_a_json_object() {
        let mut buffer = Vec::new();
        write_frame(&mut buffer, &json!({ "ok": true })).expect("write");
        let mut cursor = Cursor::new(buffer);
        let body = read_frame(&mut cursor).expect("read");
        let value: serde_json::Value = serde_json::from_slice(&body).expect("json");
        assert_eq!(value, json!({ "ok": true }));
    }

    #[test]
    fn rejects_an_empty_frame_length() {
        let mut cursor = Cursor::new([0_u8, 0, 0, 0]);
        let error = read_frame(&mut cursor).expect_err("zero length");
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    }
}
