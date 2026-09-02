use std::collections::VecDeque;

use cli_master_core::SessionId;

use crate::{OutputChunk, SessionError};

pub(crate) struct ReplayBuffer {
    chunks: VecDeque<OutputChunk>,
    retained_bytes: usize,
    maximum_bytes: usize,
    maximum_chunks: usize,
    next_sequence: u64,
}

pub(crate) struct ReplayView {
    pub chunks: Vec<OutputChunk>,
    pub first_available_sequence: Option<u64>,
    pub next_sequence: u64,
    pub gap: bool,
}

impl ReplayBuffer {
    pub fn new(maximum_bytes: usize, maximum_chunks: usize) -> Self {
        Self {
            chunks: VecDeque::new(),
            retained_bytes: 0,
            maximum_bytes,
            maximum_chunks,
            next_sequence: 1,
        }
    }

    pub fn append(
        &mut self,
        session_id: SessionId,
        bytes: Vec<u8>,
        occurred_at_ms: i64,
    ) -> Result<OutputChunk, SessionError> {
        let sequence = self.next_sequence;
        self.next_sequence = sequence
            .checked_add(1)
            .ok_or(SessionError::SequenceExhausted { session_id })?;
        let chunk = OutputChunk {
            session_id,
            sequence,
            bytes,
            occurred_at_ms,
        };
        self.retained_bytes += chunk.bytes.len();
        self.chunks.push_back(chunk.clone());
        self.evict_oldest_chunks();
        Ok(chunk)
    }

    pub fn view(
        &self,
        session_id: SessionId,
        after_sequence: u64,
    ) -> Result<ReplayView, SessionError> {
        let latest = self.next_sequence - 1;
        if after_sequence > latest {
            return Err(SessionError::ReplayCursorAhead {
                session_id,
                requested: after_sequence,
                latest,
            });
        }
        let first_available_sequence = self.chunks.front().map(|chunk| chunk.sequence);
        let gap =
            first_available_sequence.is_some_and(|first| after_sequence.saturating_add(1) < first);
        let chunks = self
            .chunks
            .iter()
            .filter(|chunk| chunk.sequence > after_sequence)
            .cloned()
            .collect();
        Ok(ReplayView {
            chunks,
            first_available_sequence,
            next_sequence: self.next_sequence,
            gap,
        })
    }

    fn evict_oldest_chunks(&mut self) {
        while self.retained_bytes > self.maximum_bytes || self.chunks.len() > self.maximum_chunks {
            let Some(evicted) = self.chunks.pop_front() else {
                break;
            };
            self.retained_bytes -= evicted.bytes.len();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replay_evicts_by_bytes_and_reports_gap() {
        let session_id = SessionId::new();
        let mut replay = ReplayBuffer::new(4, 8);
        replay.append(session_id, b"abc".to_vec(), 1).unwrap();
        replay.append(session_id, b"def".to_vec(), 2).unwrap();

        let view = replay.view(session_id, 0).unwrap();
        assert_eq!(view.chunks.len(), 1);
        assert_eq!(view.chunks[0].bytes, b"def");
        assert_eq!(view.first_available_sequence, Some(2));
        assert!(view.gap);
        assert_eq!(view.next_sequence, 3);
    }

    #[test]
    fn replay_evicts_by_chunk_count() {
        let session_id = SessionId::new();
        let mut replay = ReplayBuffer::new(32, 2);
        for value in *b"abc" {
            replay.append(session_id, vec![value], 1).unwrap();
        }

        let view = replay.view(session_id, 1).unwrap();
        assert_eq!(
            view.chunks
                .iter()
                .map(|chunk| chunk.bytes[0])
                .collect::<Vec<_>>(),
            vec![b'b', b'c']
        );
        assert!(!view.gap);
    }

    #[test]
    fn replay_rejects_a_cursor_from_the_future() {
        let session_id = SessionId::new();
        let replay = ReplayBuffer::new(8, 2);

        assert!(matches!(
            replay.view(session_id, 1),
            Err(SessionError::ReplayCursorAhead {
                requested: 1,
                latest: 0,
                ..
            })
        ));
    }
}
