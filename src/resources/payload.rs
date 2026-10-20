//! The data of `/payload`: bytes to send, made once and shared, and a count of the bytes received.
//! Nothing is held in memory but one chunk.
use crate::resources::splitmix::{GAMMA, mix};
use axum::body::Bytes;
use futures_util::Stream;
use std::convert::Infallible;
use std::sync::OnceLock;

/// The size of a piece of the answer
const CHUNK: usize = 64 * 1024;

/// A chunk of bytes that don't compress, so a proxy that compresses doesn't shrink what is being sent
fn chunk() -> &'static Bytes {
    static CHUNK_OF_BYTES: OnceLock<Bytes> = OnceLock::new();
    CHUNK_OF_BYTES.get_or_init(|| {
        // a fixed seed: the content does not matter, only that it looks random
        let mut state: u64 = 0x2545_F491_4F6C_DD1D;
        let mut bytes = Vec::with_capacity(CHUNK);
        while bytes.len() < CHUNK {
            state = state.wrapping_add(GAMMA);
            bytes.extend_from_slice(&mix(state).to_le_bytes());
        }
        Bytes::from(bytes)
    })
}

/// `total` bytes in pieces: the same chunk again and again, which costs no memory
pub fn stream(total: u64) -> impl Stream<Item = Result<Bytes, Infallible>> {
    futures_util::stream::unfold(total, |left| async move {
        if left == 0 {
            return None;
        }
        let size = left.min(CHUNK as u64);
        Some((Ok(chunk().slice(..size as usize)), left - size))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::StreamExt;

    #[tokio::test]
    async fn the_stream_has_exactly_the_bytes_asked_for() {
        for total in [0, 1, CHUNK as u64, CHUNK as u64 + 5, 3 * CHUNK as u64] {
            let sent: u64 = stream(total).map(|piece| piece.unwrap().len() as u64).fold(0, |sum, len| async move { sum + len }).await;
            assert_eq!(sent, total);
        }
    }

    #[test]
    fn the_bytes_do_not_repeat_inside_a_chunk() {
        let bytes = chunk();
        assert_eq!(bytes.len(), CHUNK);
        assert_ne!(bytes[..8], bytes[8..16]);
        assert!(bytes.iter().collect::<std::collections::HashSet<_>>().len() > 200);
    }
}
