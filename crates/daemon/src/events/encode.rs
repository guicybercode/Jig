use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use cli_master_core::wire::{MAX_PTY_OUTPUT_BYTES, PtyOutputBase64, WireValidationError};

pub fn encode_output_chunk(bytes: &[u8]) -> Result<PtyOutputBase64, WireValidationError> {
    let encoded = STANDARD.encode(bytes);
    PtyOutputBase64::try_new(encoded)
}

pub fn split_output(bytes: &[u8]) -> Vec<&[u8]> {
    if bytes.is_empty() {
        Vec::new()
    } else {
        bytes.chunks(MAX_PTY_OUTPUT_BYTES).collect()
    }
}
