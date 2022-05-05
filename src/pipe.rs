use std::fmt::{Display, Formatter};
use std::io;
use std::io::ErrorKind;
use std::time::Duration;
use async_trait::async_trait;
use bytes::Bytes;
use future::Either;
use futures::{future, FutureExt};
use tokio::time::Instant;
use crate::{log_id, log_utils};


macro_rules! log_dir {
    ($lvl:ident, $id_chain:expr, $direction:expr, $msg:expr) => {
        log_id!($lvl, $id_chain, std::concat!("{} ", $msg), $direction)
    };
    ($lvl:ident, $id_chain:expr, $direction:expr, $fmt:expr, $($arg:tt)*) => {
        log_id!($lvl, $id_chain, std::concat!("{} ", $fmt), $direction, $($arg)*)
    };
}


pub(crate) enum Data {
    /// Data chunk
    Chunk(Bytes),
    /// No more data will be transmitted in that direction
    Eof,
}

/// An abstract interface for a receiver implementation
#[async_trait]
pub(crate) trait Source: Send {
    /// Get the request ID for logging
    fn id(&self) -> log_utils::IdChain<u64>;

    /// Listen for incoming data on the connection.
    async fn read(&mut self) -> io::Result<Data>;

    /// Slide receive window on the connection. Must be called by a caller after a portion of
    /// received data is processed.
    fn consume(&mut self, size: usize) -> io::Result<()>;
}

/// An abstract interface for a transmitter implementation
#[async_trait]
pub(crate) trait Sink: Send {
    /// Get the request ID for logging
    fn id(&self) -> log_utils::IdChain<u64>;

    /// Write a data chunk to the connection.
    ///
    /// # Return
    ///
    /// An unsent portion of `data` due to flow control limits. It must be sent later by a caller.
    fn write(&mut self, data: Bytes) -> io::Result<Bytes>;

    /// Indicate that no more data will be sent to the sink
    fn eof(&mut self) -> io::Result<()>;

    /// Wait for the connection to be writable. Should be called if [`Self::write()`] return non-empty
    /// buffer.
    async fn wait_writable(&mut self) -> io::Result<()>;
}

#[derive(Copy, Clone, PartialEq)]
pub(crate) enum SimplexPipeDirection {
    /// Packet goes from a peer to a client
    Incoming,
    /// Packet goes from a client to a peer
    Outgoing,
}

/// Feeds packets received from [`Source`] to [`Sink`] doing some flow control
pub(crate) struct SimplexPipe {
    source: Box<dyn Source>,
    sink: Box<dyn Sink>,
    pending_chunk: Option<Data>,
    direction: SimplexPipeDirection,
    last_activity: Instant,
}

pub(crate) struct Error<T> {
    pub id: T,
    pub io: io::Error,
}


impl Display for Data {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Data::Chunk(x) => write!(f, "Chunk({} bytes)", x.len()),
            Data::Eof => write!(f, "Eof"),
        }
    }
}

impl Display for SimplexPipeDirection {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            SimplexPipeDirection::Incoming => write!(f, "<--"),
            SimplexPipeDirection::Outgoing => write!(f, "-->"),
        }
    }
}

impl SimplexPipe {
    pub fn new(
        source: Box<dyn Source>,
        sink: Box<dyn Sink>,
        direction: SimplexPipeDirection,
    ) -> Self {
        Self {
            source,
            sink,
            pending_chunk: Default::default(),
            direction,
            last_activity: Instant::now(),
        }
    }

    /// Initiate data exchange until the `Source` is closed or some error happened
    pub async fn exchange<T: Copy>(&mut self, id: T, timeout: Duration) -> Result<T, Error<T>> {
        loop {
            self.last_activity = Instant::now();

            let future = match self.pending_chunk.take() {
                None => async {
                    let x = self.source.read().await?;
                    log_dir!(trace, self.source.id(), self.direction, "TCP data: {}", x);
                    Ok(x)
                }.boxed(),
                Some(x) => async {
                    self.sink.wait_writable().await?;
                    log_dir!(trace, self.sink.id(), self.direction, "Sending unsent");
                    Ok(x)
                }.boxed(),
            };

            let data = tokio::time::timeout(timeout, future).await
                .unwrap_or_else(|_| Err(io::Error::from(ErrorKind::TimedOut)))
                .map_err(|e| io_to_pipe_error(id, e))?;

            match data {
                Data::Chunk(bytes) => {
                    let data_len = bytes.len();
                    let unsent_data = self.sink.write(bytes)
                        .map_err(|e| io_to_pipe_error(id, e))?;
                    self.source.consume(data_len - unsent_data.len())
                        .map_err(|e| io_to_pipe_error(id, e))?;
                    if !unsent_data.is_empty() {
                        log_dir!(trace, self.source.id(), self.direction, "Unsent: {} bytes", unsent_data.len());
                        self.pending_chunk = Some(Data::Chunk(unsent_data));
                    }
                }
                Data::Eof => return self.sink.eof()
                    .map_err(|e| io_to_pipe_error(id, e))
                    .map(|_| id),
            }
        }
    }
}

pub(crate) struct DuplexPipe {
    left_pipe: SimplexPipe,
    right_pipe: SimplexPipe,
}

impl DuplexPipe {
    pub fn new(
        left_pipe: SimplexPipe,
        right_pipe: SimplexPipe,
    ) -> Self {
        Self {
            left_pipe,
            right_pipe,
        }
    }

    pub async fn exchange(&mut self, timeout: Duration) -> io::Result<()> {
        loop {
            match self.exchange_once(timeout).await {
                Err(e) if e.kind() == ErrorKind::TimedOut => {
                    let last_unexpired_timestamp = Instant::now() - timeout;
                    if self.left_pipe.last_activity < last_unexpired_timestamp
                        && self.right_pipe.last_activity < last_unexpired_timestamp
                    {
                        return Err(e);
                    }
                    // it is ok if only one of them timed out
                }
                x => return x,
            }
        }
    }

    async fn exchange_once(&mut self, timeout: Duration) -> io::Result<()> {
        let id = self.left_pipe.source.id();
        let f1 = self.left_pipe.exchange(self.left_pipe.direction, timeout);
        futures::pin_mut!(f1);
        let f2 = self.right_pipe.exchange(self.right_pipe.direction, timeout);
        futures::pin_mut!(f2);

        match future::try_select(f1, f2).await {
            Ok(Either::Left((dir, another)))
            | Ok(Either::Right((dir, another))) => {
                log_dir!(trace, id, dir, "Pipe gracefully closed");
                another.await
                    .map(|_| ())
                    .map_err(|e| {
                        log_dir!(debug, id, e.id, "Error on pipe: {}", e.io);
                        e.io
                    })
            }
            Err(Either::Left((e, _))) | Err(Either::Right((e, _))) => {
                if e.io.kind() != ErrorKind::WouldBlock {
                    log_dir!(debug, id, e.id, "Error on pipe: {}", e.io);
                }
                Err(e.io)
            }
        }
    }
}


fn io_to_pipe_error<T>(id: T, io: io::Error) -> Error<T> {
    Error { id, io, }
}
