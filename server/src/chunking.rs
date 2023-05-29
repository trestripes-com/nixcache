//! Chunking.
//!
//! We perform chunking on uncompressed NARs using the FastCDC
//! algorithm.

use std::collections::VecDeque;
use std::pin::Pin;
use std::future::Future;
use async_stream::try_stream;
use bytes::{Bytes, BytesMut, BufMut};
use fastcdc::ronomon::FastCDC;
use futures::stream::{Stream, StreamExt, BoxStream};
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::spawn;

/// Greedily reads from a stream to fill a buffer.
pub async fn read_chunk_async<S: AsyncRead + Unpin + Send>(
    stream: &mut S,
    mut chunk: BytesMut,
) -> std::io::Result<Bytes> {
    while chunk.len() < chunk.capacity() {
        let read = stream.read_buf(&mut chunk).await?;

        if read == 0 {
            break;
        }
    }

    Ok(chunk.freeze())
}

/// Splits a streams into content-defined chunks.
///
/// This is a wrapper over fastcdc-rs that takes an `AsyncRead` and
/// returns a `Stream` of chunks as `Bytes`s.
pub fn chunk_stream<R>(
    mut stream: R,
    min_size: usize,
    avg_size: usize,
    max_size: usize,
) -> impl Stream<Item = std::io::Result<Bytes>>
where
    R: AsyncRead + Unpin + Send,
{
    let s = try_stream! {
        let mut buf = BytesMut::with_capacity(max_size);

        loop {
            let read = read_chunk_async(&mut stream, buf).await?;

            let mut eof = false;
            if read.is_empty() {
                // Already EOF
                break;
            } else if read.len() < max_size {
                // Last read
                eof = true;
            }

            let chunks = FastCDC::with_eof(&read, min_size, avg_size, max_size, eof);
            let mut consumed = 0;

            for chunk in chunks {
                consumed += chunk.length;

                let slice = read.slice(chunk.offset..chunk.offset + chunk.length);
                yield slice;
            }

            if eof {
                break;
            }

            buf = BytesMut::with_capacity(max_size);

            if consumed < read.len() {
                // remaining bytes for the next read
                buf.put_slice(&read[consumed..]);
            }
        }
    };

    Box::pin(s)
}

/// Merge chunks lazily into a continuous stream.
///
/// For each chunk, a function is called to transform it into a
/// `Stream<Item = Result<Bytes>>`. This function does something like
/// opening the local file or sending a request to S3.
///
/// We call this function some time before the start of the chunk
/// is reached to eliminate delays between chunks so the merged
/// stream is smooth. We don't want to start streaming all chunks
/// at once as it's a waste of resources.
///
/// ```text
/// | S3 GET | Chunk | S3 GET | ... | S3 GET | Chunk
/// ```
///
/// ```text
/// | S3 GET | Chunk | Chunk | Chunk | Chunk
/// | S3 GET |-----------^       ^       ^
///              | S3 GET |------|       |
///              | S3 GET |--------------|
///
/// ```
///
/// TODO: Support range requests so we can have seekable NARs.
pub fn merge_chunks<C, F, S, Fut, E>(
    mut chunks: VecDeque<C>,
    streamer: F,
    streamer_arg: S,
    num_prefetch: usize,
) -> Pin<Box<impl Stream<Item = Result<Bytes, E>>>>
where
    F: Fn(C, S) -> Fut,
    S: Clone,
    Fut: Future<Output = Result<BoxStream<'static, Result<Bytes, E>>, E>> + Send + 'static,
    E: Send + 'static,
{
    let s = try_stream! {
        let mut streams = VecDeque::new(); // a queue of JoinHandles

        // otherwise type inference gets confused :/
        if false {
            let chunk = chunks.pop_front().unwrap();
            let stream = spawn(streamer(chunk, streamer_arg.clone()));
            streams.push_back(stream);
        }

        loop {
            if let Some(stream) = streams.pop_front() {
                let mut stream = stream.await.unwrap()?;
                while let Some(item) = stream.next().await {
                    let item = item?;
                    yield item;
                }
            }

            while streams.len() < num_prefetch {
                if let Some(chunk) = chunks.pop_front() {
                    let stream = spawn(streamer(chunk, streamer_arg.clone()));
                    streams.push_back(stream);
                } else {
                    break;
                }
            }

            if chunks.is_empty() && streams.is_empty() {
                // we are done!
                break;
            }
        }
    };
    Box::pin(s)
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;
    use async_stream::stream;
    use futures::StreamExt;
    use tokio_test::block_on;
    use futures::future;

    use super::*;

    #[test]
    fn test_merge_chunks() {
        let chunk_a: BoxStream<Result<Bytes, ()>> = {
            let s = stream! {
                yield Ok(Bytes::from_static(b"Hello"));
            };
            Box::pin(s)
        };

        let chunk_b: BoxStream<Result<Bytes, ()>> = {
            let s = stream! {
                yield Ok(Bytes::from_static(b", "));
                yield Ok(Bytes::from_static(b"world"));
            };
            Box::pin(s)
        };

        let chunk_c: BoxStream<Result<Bytes, ()>> = {
            let s = stream! {
                yield Ok(Bytes::from_static(b"!"));
            };
            Box::pin(s)
        };

        let chunks: VecDeque<BoxStream<'static, Result<Bytes, ()>>> =
            [chunk_a, chunk_b, chunk_c].into_iter().collect();

        let streamer = |c, _| future::ok(c);
        let mut merged = merge_chunks(chunks, streamer, (), 2);

        let bytes = block_on(async move {
            let mut bytes = BytesMut::with_capacity(100);
            while let Some(item) = merged.next().await {
                bytes.put(item.unwrap());
            }
            bytes.freeze()
        });

        assert_eq!(&*bytes, b"Hello, world!");
    }

    /// Chunks and reconstructs a file.
    #[test]
    fn test_chunking_basic() {
        fn case(size: usize) {
            block_on(async move {
                let test_file = get_data(size); // 32 MiB
                let mut reconstructed_file = Vec::new();

                let cursor = Cursor::new(&test_file);
                let mut chunks = chunk_stream(cursor, 8 * 1024, 16 * 1024, 32 * 1024);

                while let Some(chunk) = chunks.next().await {
                    let chunk = chunk.unwrap();
                    eprintln!("Got a {}-byte chunk", chunk.len());
                    reconstructed_file.extend(chunk);
                }

                assert_eq!(reconstructed_file, test_file);
            });
        }

        case(32 * 1024 * 1024 - 1);
        case(32 * 1024 * 1024);
        case(32 * 1024 * 1024 + 1);
    }

    /// Returns some fake data.
    fn get_data(len: usize) -> Vec<u8> {
        let mut state = 42u32;
        let mut data = vec![0u8; len];

        for i in 0..data.len() {
            (state, _) = state.overflowing_mul(1664525u32);
            (state, _) = state.overflowing_add(1013904223u32);
            data[i] = ((state >> (i % 24)) & 0xff) as u8;
        }

        data
    }
}
