use std::collections::VecDeque;

use crate::event::OutputChunk;

/// Bounded replay history for UI reconnects.
#[derive(Debug)]
pub struct ReplayBuffer {
    chunks: VecDeque<OutputChunk>,
    bytes: usize,
    max_bytes: usize,
    next_sequence: u64,
    dropped: bool,
}

impl ReplayBuffer {
    pub fn new(max_bytes: usize) -> Self {
        Self {
            chunks: VecDeque::new(),
            bytes: 0,
            max_bytes: max_bytes.max(1),
            next_sequence: 0,
            dropped: false,
        }
    }

    /// Appends bytes and returns the assigned sequence number.
    pub fn push(&mut self, data: Vec<u8>) -> u64 {
        if data.is_empty() {
            return self.next_sequence;
        }
        let sequence = self.next_sequence;
        self.next_sequence += 1;

        self.bytes = self.bytes.saturating_add(data.len());
        self.chunks.push_back(OutputChunk { sequence, data });
        self.evict();
        sequence
    }

    pub fn snapshot(&self) -> crate::event::OutputSnapshot {
        crate::event::OutputSnapshot {
            first_sequence: self
                .chunks
                .front()
                .map_or(self.next_sequence, |chunk| chunk.sequence),
            next_sequence: self.next_sequence,
            truncated: self.dropped,
            chunks: self.chunks.iter().cloned().collect(),
        }
    }

    fn evict(&mut self) {
        while self.bytes > self.max_bytes {
            let Some(front_len) = self.chunks.front().map(|chunk| chunk.data.len()) else {
                self.bytes = 0;
                break;
            };
            self.chunks.pop_front();
            self.bytes -= front_len;
            self.dropped = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ReplayBuffer;

    #[test]
    fn drops_oldest_bytes_when_over_capacity() {
        let mut buffer = ReplayBuffer::new(8);
        buffer.push(b"aaaa".to_vec());
        buffer.push(b"bbbb".to_vec());
        buffer.push(b"cccc".to_vec());
        let snapshot = buffer.snapshot();

        assert!(snapshot.truncated);
        assert_eq!(snapshot.next_sequence, 3);
        let joined: Vec<u8> = snapshot
            .chunks
            .iter()
            .flat_map(|chunk| chunk.data.clone())
            .collect();
        assert!(joined.len() <= 8);
        assert!(joined.ends_with(b"cccc"));
    }

    #[test]
    fn empty_chunks_do_not_consume_an_output_sequence() {
        let mut buffer = ReplayBuffer::new(8);
        assert_eq!(buffer.push(Vec::new()), 0);
        assert_eq!(buffer.push(b"x".to_vec()), 0);
        assert_eq!(buffer.snapshot().next_sequence, 1);
    }

    #[test]
    fn drops_a_chunk_larger_than_capacity_without_splitting_its_sequence() {
        let mut buffer = ReplayBuffer::new(4);
        assert_eq!(buffer.push(b"oversized".to_vec()), 0);

        let snapshot = buffer.snapshot();

        assert!(snapshot.truncated);
        assert_eq!(snapshot.first_sequence, 1);
        assert_eq!(snapshot.next_sequence, 1);
        assert!(snapshot.chunks.is_empty());
        assert!(snapshot.concatenated().is_empty());
    }
}
