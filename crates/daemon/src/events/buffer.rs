use std::collections::VecDeque;

/// One retained output chunk for cursor-based replay.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReplayChunk {
    /// Per-session output sequence.
    pub sequence: u64,
    /// Decoded terminal bytes. Never logged.
    pub bytes: Vec<u8>,
}

/// Replay window limited by both decoded bytes and chunk count.
#[derive(Debug)]
pub struct ReplayBuffer {
    max_bytes: usize,
    max_chunks: usize,
    chunks: VecDeque<ReplayChunk>,
    bytes: usize,
}

impl ReplayBuffer {
    pub fn new(max_bytes: usize, max_chunks: usize) -> Self {
        Self {
            max_bytes: max_bytes.max(1),
            max_chunks: max_chunks.max(1),
            chunks: VecDeque::new(),
            bytes: 0,
        }
    }

    pub fn push(&mut self, sequence: u64, bytes: Vec<u8>) {
        self.bytes = self.bytes.saturating_add(bytes.len());
        self.chunks.push_back(ReplayChunk { sequence, bytes });
        self.trim();
    }

    pub fn first_sequence(&self) -> Option<u64> {
        self.chunks.front().map(|chunk| chunk.sequence)
    }

    pub fn latest_sequence(&self) -> Option<u64> {
        self.chunks.back().map(|chunk| chunk.sequence)
    }

    /// Exclusive cursor: chunks with sequence greater than `cursor`.
    pub fn after(&self, cursor: u64) -> Vec<ReplayChunk> {
        self.chunks
            .iter()
            .filter(|chunk| chunk.sequence > cursor)
            .cloned()
            .collect()
    }

    fn trim(&mut self) {
        while self.bytes > self.max_bytes || self.chunks.len() > self.max_chunks {
            let Some(removed) = self.chunks.pop_front() else {
                break;
            };
            self.bytes = self.bytes.saturating_sub(removed.bytes.len());
        }
    }
}
