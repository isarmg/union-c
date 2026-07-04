//! HTTP body 和异步流之间的适配工具。
//!
//! hyper 的请求体、tokio 的文件 reader、响应 body 使用的是不同抽象。
//! 这个文件提供小适配器，让“上传请求体写入文件”和“文件片段输出成响应”更容易写。

use bytes::{Bytes, BytesMut};
use futures_util::Stream;
use http_body_util::{combinators::BoxBody, BodyExt, Full};
use hyper::body::{Body, Incoming};
use std::{
    pin::Pin,
    task::{Context, Poll},
};
use tokio::io::AsyncRead;
use tokio_util::io::poll_read_buf;

#[derive(Debug)]
pub struct IncomingStream {
    inner: Incoming,
}

impl IncomingStream {
    /// 把 hyper 请求体包装成 futures Stream。
    pub fn new(inner: Incoming) -> Self {
        Self { inner }
    }
}

impl Stream for IncomingStream {
    type Item = Result<Bytes, anyhow::Error>;

    #[inline]
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        loop {
            // hyper body 是一帧一帧的；这里只取数据帧，忽略非数据帧。
            match futures_util::ready!(Pin::new(&mut self.inner).poll_frame(cx)?) {
                Some(frame) => match frame.into_data() {
                    Ok(data) => return Poll::Ready(Some(Ok(data))),
                    Err(_frame) => {}
                },
                None => return Poll::Ready(None),
            }
        }
    }
}

pin_project_lite::pin_project! {
    /// 限制最多读取 `remaining` 字节的异步流。
    ///
    /// 下载 Range 时会用它保证只输出文件的一小段，而不是一直读到文件末尾。
    pub struct LengthLimitedStream<R> {
        #[pin]
        reader: Option<R>,
        remaining: usize,
        buf: BytesMut,
        capacity: usize,
    }
}

impl<R> LengthLimitedStream<R> {
    /// 创建最多读取 `limit` 字节的流。
    pub fn new(reader: R, limit: usize) -> Self {
        Self {
            reader: Some(reader),
            remaining: limit,
            buf: BytesMut::new(),
            capacity: 4096,
        }
    }
}

impl<R: AsyncRead> Stream for LengthLimitedStream<R> {
    type Item = std::io::Result<Bytes>;
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut this = self.as_mut().project();

        if *this.remaining == 0 {
            // 剩余字节为 0 时结束流，并释放 reader。
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
                // 本次读到的数据可能超过剩余长度，所以要截断。
                let chunk_size = (*this.remaining).min(chunk.len());
                chunk.truncate(chunk_size);
                *this.remaining -= chunk_size;
                Poll::Ready(Some(Ok(chunk.freeze())))
            }
        }
    }
}

/// 创建一个完整响应体，适合返回短文本、JSON 或错误消息。
pub fn body_full(content: impl Into<hyper::body::Bytes>) -> BoxBody<Bytes, anyhow::Error> {
    // 把完整内容一次性包装成 hyper 响应 body，适合小文本、JSON、HTML。
    Full::new(content.into())
        .map_err(anyhow::Error::new)
        .boxed()
}
