use std::{collections::VecDeque, io::Write, sync::Mutex};
use tracing::Level;
use tracing_subscriber::{fmt::MakeWriter, layer::SubscriberExt, util::SubscriberInitExt};

/// Ring buffer for storing log entries with a fixed capacity
#[derive(Debug)]
pub struct RingBuffer {
    buffer: Mutex<VecDeque<String>>,
    capacity: usize,
}

impl RingBuffer {
    pub fn new(capacity: usize) -> Self {
        Self {
            buffer: Mutex::new(VecDeque::with_capacity(capacity)),
            capacity,
        }
    }

    pub fn push(&self, line: String) {
        let mut buffer = self.buffer.lock().unwrap();
        if buffer.len() >= self.capacity {
            buffer.pop_front();
        }
        buffer.push_back(line);
    }

    #[allow(dead_code)]
    pub fn get_logs(&self) -> Vec<String> {
        let buffer = self.buffer.lock().unwrap();
        buffer.iter().cloned().collect()
    }
}

impl Write for &RingBuffer {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let line = String::from_utf8_lossy(buf).into_owned();
        self.push(line);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[derive(Clone)]
pub struct RingBufferWriter {
    buffer: std::sync::Arc<RingBuffer>,
}

impl RingBufferWriter {
    pub fn new(capacity: usize) -> Self {
        Self {
            buffer: std::sync::Arc::new(RingBuffer::new(capacity)),
        }
    }
}

impl<'a> MakeWriter<'a> for RingBufferWriter {
    type Writer = RingBufferWriterInstance;

    fn make_writer(&'a self) -> Self::Writer {
        RingBufferWriterInstance {
            buffer: self.buffer.clone(),
        }
    }
}

pub struct RingBufferWriterInstance {
    buffer: std::sync::Arc<RingBuffer>,
}

impl Write for RingBufferWriterInstance {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let line = String::from_utf8_lossy(buf).into_owned();
        self.buffer.push(line);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

pub fn init_tracing(log_level: Level, ring_buffer_size: usize) {
    let ring_writer = RingBufferWriter::new(ring_buffer_size);

    let subscriber = tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(ring_writer)
                .with_ansi(false)
                .with_target(false)
                .with_level(true)
                .with_thread_ids(false)
                .with_thread_names(false)
                .with_file(false)
                .with_line_number(false),
        )
        .with(tracing_subscriber::filter::LevelFilter::from_level(
            log_level,
        ));

    subscriber.init();
}