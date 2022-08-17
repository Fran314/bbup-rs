use std::{
    pin::Pin,
    sync::{Arc, Mutex},
};

use indicatif::{ProgressBar, ProgressStyle};
use tokio::io::{AsyncRead, AsyncWrite};

fn pb_style_from(direction: &str, name: &str) -> ProgressStyle {
    let style_path = String::from("[")
        + direction
        + "]\t"
        + name
        + "\t\t{bytes}\t{percent}%\t{bytes_per_sec}\t{elapsed_precise}";
    ProgressStyle::default_bar().template(style_path.as_str())
}
async fn update_progressbar(pb: Arc<Mutex<ProgressBar>>, bytes: Arc<Mutex<u64>>) {
    loop {
        pb.lock().unwrap().set_position(*bytes.lock().unwrap());
        tokio::time::sleep(tokio::time::Duration::from_millis(45)).await;
    }
}

pub struct ProgressWriter<'a, W: AsyncWrite + Unpin + Sync + Send> {
    pb_task_handle: tokio::task::JoinHandle<()>,
    pb: Arc<Mutex<ProgressBar>>,
    bytes_written: Arc<Mutex<u64>>,
    pub writer: &'a mut W,
}
impl<'a, W: AsyncWrite + Unpin + Sync + Send> ProgressWriter<'a, W> {
    pub fn new(writer: &'a mut W, len: u64, name: &str) -> ProgressWriter<'a, W> {
        let pb = ProgressBar::new(len);
        pb.set_style(pb_style_from("upload", name));

        let pb = Arc::new(Mutex::new(pb));
        let bytes_written = Arc::new(Mutex::new(0u64));

        let pb_task_handle = tokio::spawn(update_progressbar(pb.clone(), bytes_written.clone()));

        ProgressWriter {
            pb_task_handle,
            pb,
            bytes_written,
            writer,
        }
    }

    pub async fn finish(self) {
        self.pb.lock().unwrap().finish();
        self.pb_task_handle.abort();
    }
}
impl<'a, W: AsyncWrite + Unpin + Sync + Send> AsyncWrite for ProgressWriter<'a, W> {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<Result<usize, std::io::Error>> {
        match Pin::new(&mut self.writer).poll_write(cx, buf) {
            std::task::Poll::Ready(writer) => match writer {
                Ok(bytes) => {
                    *self.bytes_written.lock().unwrap() += bytes as u64;
                    std::task::Poll::Ready(Ok(bytes))
                }
                Err(err) => std::task::Poll::Ready(Err(err)),
            },
            std::task::Poll::Pending => std::task::Poll::Pending,
        }
    }
    fn poll_flush(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), std::io::Error>> {
        Pin::new(&mut self.writer).poll_flush(cx)
    }
    fn poll_shutdown(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), std::io::Error>> {
        Pin::new(&mut self.writer).poll_shutdown(cx)
    }
}

pub struct ProgressReader<'a, W: AsyncRead + Unpin + Sync + Send> {
    pb_task_handle: tokio::task::JoinHandle<()>,
    pb: Arc<Mutex<ProgressBar>>,
    bytes_read: Arc<Mutex<u64>>,
    pub reader: &'a mut W,
}
impl<'a, R: AsyncRead + Unpin + Sync + Send> ProgressReader<'a, R> {
    pub fn new(reader: &'a mut R, len: u64, name: &str) -> ProgressReader<'a, R> {
        let pb = ProgressBar::new(len);
        pb.set_style(pb_style_from("download", name));

        let pb = Arc::new(Mutex::new(pb));
        let bytes_read = Arc::new(Mutex::new(0u64));

        let pb_task_handle = tokio::spawn(update_progressbar(pb.clone(), bytes_read.clone()));

        ProgressReader {
            pb_task_handle,
            pb,
            bytes_read,
            reader,
        }
    }

    pub async fn finish(self) {
        self.pb.lock().unwrap().finish();
        self.pb_task_handle.abort();
    }
}
impl<'a, R: AsyncRead + Unpin + Sync + Send> AsyncRead for ProgressReader<'a, R> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match Pin::new(&mut self.reader).poll_read(cx, buf) {
            std::task::Poll::Ready(reader) => match reader {
                Ok(()) => {
                    *self.bytes_read.lock().unwrap() += buf.filled().len() as u64;
                    std::task::Poll::Ready(Ok(()))
                }
                Err(err) => std::task::Poll::Ready(Err(err)),
            },
            std::task::Poll::Pending => std::task::Poll::Pending,
        }
    }
}
