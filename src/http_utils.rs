use bytes::{Bytes, BytesMut};
use futures_util::Stream;
use http_body_util::{combinators::BoxBody, BodyExt, Full};
use hyper::body::{Body, Incoming};
use std::{
    pin::Pin,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    task::{Context, Poll},
};
use tokio::io::AsyncRead;
use tokio_util::io::poll_read_buf;

use crate::metrics::METRICS;
use crate::server::BUF_SIZE;

/// What one request has shown so far that its access line and the upload
/// metrics need. It travels in the request's extensions.
#[derive(Debug, Default)]
pub struct RequestStats {
    /// Request body bytes read by an upload.
    pub bytes_received: AtomicU64,
    /// Reading the request body failed, i.e. the client went away or broke
    /// the framing, as opposed to the server failing to store it.
    pub body_failed: AtomicBool,
    /// The request was counted as an upload by `dufs_uploads_total`.
    pub upload_counted: AtomicBool,
    /// Kind and message of the io error that failed an upload.
    pub io_error: Mutex<Option<(String, String)>>,
}

impl RequestStats {
    pub fn of(req: &crate::server::Request) -> Arc<Self> {
        req.extensions()
            .get::<Arc<Self>>()
            .cloned()
            .unwrap_or_default()
    }
}

/// An upload request's body as a stream of chunks.
#[derive(Debug)]
pub struct IncomingStream {
    inner: Incoming,
    stats: Arc<RequestStats>,
}

impl IncomingStream {
    pub fn new(inner: Incoming, stats: Arc<RequestStats>) -> Self {
        Self { inner, stats }
    }
}

impl Stream for IncomingStream {
    type Item = Result<Bytes, anyhow::Error>;

    #[inline]
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        loop {
            match futures_util::ready!(Pin::new(&mut self.inner).poll_frame(cx)) {
                Some(Ok(frame)) => {
                    if let Ok(data) = frame.into_data() {
                        let len = data.len() as u64;
                        self.stats.bytes_received.fetch_add(len, Ordering::Relaxed);
                        METRICS.add_upload_bytes(len);
                        return Poll::Ready(Some(Ok(data)));
                    }
                }
                Some(Err(err)) => {
                    self.stats.body_failed.store(true, Ordering::Relaxed);
                    return Poll::Ready(Some(Err(err.into())));
                }
                None => return Poll::Ready(None),
            }
        }
    }
}

pin_project_lite::pin_project! {
    pub struct LengthLimitedStream<R> {
        #[pin]
        reader: Option<R>,
        remaining: usize,
        buf: BytesMut,
        capacity: usize,
    }
}

impl<R> LengthLimitedStream<R> {
    pub fn new(reader: R, limit: usize) -> Self {
        Self {
            reader: Some(reader),
            remaining: limit,
            buf: BytesMut::new(),
            capacity: BUF_SIZE,
        }
    }
}

impl<R: AsyncRead> Stream for LengthLimitedStream<R> {
    type Item = std::io::Result<Bytes>;
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut this = self.as_mut().project();

        if *this.remaining == 0 {
            self.project().reader.set(None);
            return Poll::Ready(None);
        }

        let reader = match this.reader.as_pin_mut() {
            Some(r) => r,
            None => return Poll::Ready(None),
        };

        if this.buf.capacity() == 0 {
            this.buf.reserve(*this.capacity);
        }

        match poll_read_buf(reader, cx, &mut this.buf) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Err(err)) => {
                self.project().reader.set(None);
                Poll::Ready(Some(Err(err)))
            }
            Poll::Ready(Ok(0)) => {
                self.project().reader.set(None);
                Poll::Ready(None)
            }
            Poll::Ready(Ok(_)) => {
                let mut chunk = this.buf.split();
                let chunk_size = (*this.remaining).min(chunk.len());
                chunk.truncate(chunk_size);
                *this.remaining -= chunk_size;
                Poll::Ready(Some(Ok(chunk.freeze())))
            }
        }
    }
}

pub fn body_full(content: impl Into<hyper::body::Bytes>) -> BoxBody<Bytes, anyhow::Error> {
    Full::new(content.into())
        .map_err(anyhow::Error::new)
        .boxed()
}
