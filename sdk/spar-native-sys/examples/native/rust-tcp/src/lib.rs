//! TCP primitives for Spar, implemented entirely as a native ABI 1 module.
//!
//! This module deliberately leaves HTTP parsing to Spar libraries. Each read returns at most
//! `max_bytes` bytes; callers must loop for full messages. An empty read means EOF.

#[spar_native::module(name = "nativeTcp", version = "0.1.0")]
mod native_tcp {
    use spar_native::{Error, Owned, Resource};
    use std::io::{Read, Write};
    use std::net::{Shutdown, TcpListener as StdListener, TcpStream};
    use std::sync::Mutex;

    pub struct TcpListenerHandle(StdListener);
    impl Resource for TcpListenerHandle {
        const SPAR_TYPE: &'static str = "TcpListenerHandle";
    }

    pub struct TcpConnectionHandle(Mutex<Option<TcpStream>>);
    impl Resource for TcpConnectionHandle {
        const SPAR_TYPE: &'static str = "TcpConnectionHandle";
    }

    fn io_error(action: &str, error: std::io::Error) -> Error {
        Error::new(format!("TCP {action} failed: {error}"))
    }

    #[spar_native::function]
    fn listen(address: &str) -> Result<Owned<TcpListenerHandle>, Error> {
        let listener = StdListener::bind(address).map_err(|e| io_error("listen", e))?;
        Ok(Owned(TcpListenerHandle(listener)))
    }

    #[spar_native::function]
    fn local_address(listener: &TcpListenerHandle) -> Result<String, Error> {
        listener
            .0
            .local_addr()
            .map(|addr| addr.to_string())
            .map_err(|e| io_error("local address", e))
    }

    #[spar_native::function]
    fn accept(listener: &TcpListenerHandle) -> Result<Owned<TcpConnectionHandle>, Error> {
        let (stream, _) = listener.0.accept().map_err(|e| io_error("accept", e))?;
        Ok(Owned(TcpConnectionHandle(Mutex::new(Some(stream)))))
    }

    #[spar_native::function]
    fn peer_address(connection: &TcpConnectionHandle) -> Result<String, Error> {
        let guard = connection
            .0
            .lock()
            .map_err(|_| Error::new("TCP connection lock poisoned"))?;
        let stream = guard
            .as_ref()
            .ok_or_else(|| Error::new("TCP connection is closed"))?;
        stream
            .peer_addr()
            .map(|addr| addr.to_string())
            .map_err(|e| io_error("peer address", e))
    }

    #[spar_native::function]
    fn set_read_timeout(connection: &TcpConnectionHandle, millis: i64) -> Result<(), Error> {
        if millis < 0 {
            return Err(Error::range("TCP read timeout cannot be negative"));
        }
        let guard = connection
            .0
            .lock()
            .map_err(|_| Error::new("TCP connection lock poisoned"))?;
        let stream = guard
            .as_ref()
            .ok_or_else(|| Error::new("TCP connection is closed"))?;
        let timeout = if millis == 0 {
            None
        } else {
            Some(std::time::Duration::from_millis(millis as u64))
        };
        stream
            .set_read_timeout(timeout)
            .map_err(|e| io_error("set read timeout", e))
    }

    #[spar_native::function]
    fn read(connection: &TcpConnectionHandle, max_bytes: i64) -> Result<Vec<u8>, Error> {
        if !(1..=1_048_576).contains(&max_bytes) {
            return Err(Error::range(
                "TCP read maxBytes must be between 1 and 1048576",
            ));
        }
        let mut guard = connection
            .0
            .lock()
            .map_err(|_| Error::new("TCP connection lock poisoned"))?;
        let stream = guard
            .as_mut()
            .ok_or_else(|| Error::new("TCP connection is closed"))?;
        let mut bytes = vec![0; max_bytes as usize];
        let count = stream.read(&mut bytes).map_err(|e| io_error("read", e))?;
        bytes.truncate(count);
        Ok(bytes)
    }

    #[spar_native::function]
    fn write_bytes(connection: &TcpConnectionHandle, data: &[u8]) -> Result<(), Error> {
        let mut guard = connection
            .0
            .lock()
            .map_err(|_| Error::new("TCP connection lock poisoned"))?;
        let stream = guard
            .as_mut()
            .ok_or_else(|| Error::new("TCP connection is closed"))?;
        stream.write_all(data).map_err(|e| io_error("write", e))
    }

    #[spar_native::function]
    fn write_text(connection: &TcpConnectionHandle, text: &str) -> Result<(), Error> {
        let mut guard = connection
            .0
            .lock()
            .map_err(|_| Error::new("TCP connection lock poisoned"))?;
        let stream = guard
            .as_mut()
            .ok_or_else(|| Error::new("TCP connection is closed"))?;
        stream
            .write_all(text.as_bytes())
            .map_err(|e| io_error("write", e))
    }

    #[spar_native::function]
    fn close(connection: &TcpConnectionHandle) -> Result<(), Error> {
        let mut guard = connection
            .0
            .lock()
            .map_err(|_| Error::new("TCP connection lock poisoned"))?;
        if let Some(stream) = guard.take() {
            stream
                .shutdown(Shutdown::Both)
                .map_err(|e| io_error("close", e))?;
        }
        Ok(())
    }
}
