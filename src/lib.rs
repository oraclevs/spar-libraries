//! TCP and TLS transport primitives for Spar's native ABI 1.
//!
//! HTTP parsing remains in the Spar HTTP server package. An empty read means EOF.

#[spar_native::module(name = "nativeTcp", version = "0.2.0")]
mod native_tcp {
    use rustls::pki_types::ServerName;
    use rustls::{
        ClientConfig, ClientConnection, RootCertStore, ServerConfig, ServerConnection, StreamOwned,
    };
    use socket2::{Domain, Protocol, SockAddr, SockRef, Socket, TcpKeepalive, Type};
    use spar_native::{Error, Owned, Resource};
    use std::fs::File;
    use std::io::{BufReader, Read, Write};
    use std::net::{Shutdown, TcpListener as StdListener, TcpStream, ToSocketAddrs};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    pub struct TcpListenerHandle {
        socket: Mutex<Option<StdListener>>,
        tls: Option<Arc<ServerConfig>>,
    }
    impl Resource for TcpListenerHandle {
        const SPAR_TYPE: &'static str = "TcpListenerHandle";
    }

    enum Connection {
        Plain(TcpStream),
        Tls(StreamOwned<ServerConnection, TcpStream>),
        TlsClient(StreamOwned<ClientConnection, TcpStream>),
    }

    impl Connection {
        fn socket(&self) -> &TcpStream {
            match self {
                Self::Plain(socket) => socket,
                Self::Tls(stream) => stream.get_ref(),
                Self::TlsClient(stream) => stream.get_ref(),
            }
        }

        fn close(mut self) -> std::io::Result<()> {
            fn already_closed(error: &std::io::Error) -> bool {
                matches!(
                    error.kind(),
                    std::io::ErrorKind::NotConnected
                        | std::io::ErrorKind::BrokenPipe
                        | std::io::ErrorKind::ConnectionReset
                )
            }
            let result = match &mut self {
                Self::Plain(socket) => socket.shutdown(Shutdown::Both),
                Self::Tls(stream) => {
                    stream.conn.send_close_notify();
                    if let Err(error) = stream.flush() {
                        if !already_closed(&error) {
                            return Err(error);
                        }
                    }
                    stream.get_ref().shutdown(Shutdown::Both)
                }
                Self::TlsClient(stream) => {
                    stream.conn.send_close_notify();
                    if let Err(error) = stream.flush() {
                        if !already_closed(&error) {
                            return Err(error);
                        }
                    }
                    stream.get_ref().shutdown(Shutdown::Both)
                }
            };
            match result {
                Err(error) if already_closed(&error) => Ok(()),
                other => other,
            }
        }
    }

    impl Read for Connection {
        fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
            match self {
                Self::Plain(socket) => socket.read(bytes),
                Self::Tls(stream) => stream.read(bytes),
                Self::TlsClient(stream) => stream.read(bytes),
            }
        }
    }

    impl Write for Connection {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            match self {
                Self::Plain(socket) => socket.write(bytes),
                Self::Tls(stream) => stream.write(bytes),
                Self::TlsClient(stream) => stream.write(bytes),
            }
        }

        fn flush(&mut self) -> std::io::Result<()> {
            match self {
                Self::Plain(socket) => socket.flush(),
                Self::Tls(stream) => stream.flush(),
                Self::TlsClient(stream) => stream.flush(),
            }
        }
    }

    pub struct TcpConnectionHandle {
        state: Arc<Mutex<Option<Connection>>>,
        // A separate socket handle lets a blocked read coexist with a write.
        plain_reader: Option<Arc<Mutex<TcpStream>>>,
    }
    impl TcpConnectionHandle {
        fn new(connection: Connection) -> Result<Self, Error> {
            let plain_reader = match &connection {
                Connection::Plain(socket) => Some(Arc::new(Mutex::new(
                    socket
                        .try_clone()
                        .map_err(|e| io_error("split connection", e))?,
                ))),
                _ => None,
            };
            Ok(Self {
                state: Arc::new(Mutex::new(Some(connection))),
                plain_reader,
            })
        }
    }
    impl Resource for TcpConnectionHandle {
        const SPAR_TYPE: &'static str = "TcpConnectionHandle";
    }

    fn io_error(action: &str, error: std::io::Error) -> Error {
        Error::new(format!(
            "TCP {action} failed (kind={:?}, osCode={:?}): {error}",
            error.kind(),
            error.raw_os_error()
        ))
    }

    fn server_config(cert_path: &str, key_path: &str) -> Result<Arc<ServerConfig>, Error> {
        let cert_file = File::open(cert_path).map_err(|e| io_error("open certificate", e))?;
        let certs: Vec<_> = rustls_pemfile::certs(&mut BufReader::new(cert_file))
            .collect::<Result<_, _>>()
            .map_err(|e| io_error("read certificate", e))?;
        if certs.is_empty() {
            return Err(Error::new("TLS certificate file has no certificates"));
        }
        let key_file = File::open(key_path).map_err(|e| io_error("open private key", e))?;
        let key = rustls_pemfile::private_key(&mut BufReader::new(key_file))
            .map_err(|e| io_error("read private key", e))?
            .ok_or_else(|| Error::new("TLS private key file has no usable key"))?;
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let config = ServerConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .map_err(|e| Error::new(format!("TLS protocol configuration failed: {e}")))?
            .with_no_client_auth()
            .with_single_cert(certs, key)
            .map_err(|e| Error::new(format!("TLS certificate or private key is invalid: {e}")))?;
        Ok(Arc::new(config))
    }

    fn client_config(ca_cert_path: &str) -> Result<Arc<ClientConfig>, Error> {
        let cert_file =
            File::open(ca_cert_path).map_err(|e| io_error("open CA certificates", e))?;
        let certs: Vec<_> = rustls_pemfile::certs(&mut BufReader::new(cert_file))
            .collect::<Result<_, _>>()
            .map_err(|e| io_error("read CA certificates", e))?;
        if certs.is_empty() {
            return Err(Error::new("TLS CA file has no certificates"));
        }
        let mut roots = RootCertStore::empty();
        for cert in certs {
            roots
                .add(cert)
                .map_err(|e| Error::new(format!("invalid TLS CA certificate: {e}")))?;
        }
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let config = ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .map_err(|e| Error::new(format!("TLS protocol configuration failed: {e}")))?
            .with_root_certificates(roots)
            .with_no_client_auth();
        Ok(Arc::new(config))
    }

    fn listener_socket(listener: &TcpListenerHandle) -> Result<StdListener, Error> {
        let guard = listener
            .socket
            .lock()
            .map_err(|_| Error::new("TCP listener lock poisoned"))?;
        guard
            .as_ref()
            .ok_or_else(|| Error::new("TCP listener is closed"))?
            .try_clone()
            .map_err(|e| io_error("clone listener", e))
    }

    #[spar_native::function]
    fn listen(address: &str) -> Result<Owned<TcpListenerHandle>, Error> {
        let socket = StdListener::bind(address).map_err(|e| io_error("listen", e))?;
        socket
            .set_nonblocking(true)
            .map_err(|e| io_error("configure listener", e))?;
        Ok(Owned(TcpListenerHandle {
            socket: Mutex::new(Some(socket)),
            tls: None,
        }))
    }

    #[spar_native::function]
    fn listen_tls(
        address: &str,
        cert_path: &str,
        key_path: &str,
    ) -> Result<Owned<TcpListenerHandle>, Error> {
        let tls = server_config(cert_path, key_path)?;
        let socket = StdListener::bind(address).map_err(|e| io_error("listen", e))?;
        socket
            .set_nonblocking(true)
            .map_err(|e| io_error("configure listener", e))?;
        Ok(Owned(TcpListenerHandle {
            socket: Mutex::new(Some(socket)),
            tls: Some(tls),
        }))
    }

    #[spar_native::function]
    fn local_address(listener: &TcpListenerHandle) -> Result<String, Error> {
        listener_socket(listener)?
            .local_addr()
            .map(|addr| addr.to_string())
            .map_err(|e| io_error("local address", e))
    }

    fn accept_inner(
        listener: &TcpListenerHandle,
        timeout: Option<Duration>,
    ) -> Result<Owned<TcpConnectionHandle>, Error> {
        let started = Instant::now();
        loop {
            let (mut socket, _) = match listener_socket(listener)?.accept() {
                Ok(pair) => pair,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    if timeout.is_some_and(|limit| started.elapsed() >= limit) {
                        return Err(Error::new("TCP accept timed out"));
                    }
                    std::thread::sleep(Duration::from_millis(10));
                    continue;
                }
                Err(error) => return Err(io_error("accept", error)),
            };
            socket
                .set_nonblocking(false)
                .map_err(|e| io_error("configure connection", e))?;
            let connection = match &listener.tls {
                None => Connection::Plain(socket),
                Some(config) => {
                    let timeout = Some(Duration::from_secs(5));
                    socket
                        .set_read_timeout(timeout)
                        .map_err(|e| io_error("set TLS handshake timeout", e))?;
                    socket
                        .set_write_timeout(timeout)
                        .map_err(|e| io_error("set TLS handshake timeout", e))?;
                    let mut session = ServerConnection::new(config.clone())
                        .map_err(|e| Error::new(format!("TLS session setup failed: {e}")))?;
                    if session.complete_io(&mut socket).is_err() {
                        continue;
                    }
                    Connection::Tls(StreamOwned::new(session, socket))
                }
            };
            return Ok(Owned(TcpConnectionHandle::new(connection)?));
        }
    }

    #[spar_native::function]
    fn accept(listener: &TcpListenerHandle) -> Result<Owned<TcpConnectionHandle>, Error> {
        accept_inner(listener, None)
    }

    #[spar_native::function]
    fn accept_timeout(
        listener: &TcpListenerHandle,
        millis: i64,
    ) -> Result<Owned<TcpConnectionHandle>, Error> {
        if millis <= 0 {
            return Err(Error::range("TCP accept timeout must be positive"));
        }
        accept_inner(listener, Some(Duration::from_millis(millis as u64)))
    }

    #[spar_native::function]
    fn close_listener(listener: &TcpListenerHandle) -> Result<(), Error> {
        let mut guard = listener
            .socket
            .lock()
            .map_err(|_| Error::new("TCP listener lock poisoned"))?;
        guard.take();
        Ok(())
    }

    fn connect_happy(address: &str, timeout: Duration) -> Result<TcpStream, Error> {
        use std::sync::mpsc;
        let mut ipv6 = std::collections::VecDeque::new();
        let mut ipv4 = std::collections::VecDeque::new();
        let mut first_is_v6 = None;
        for resolved in address
            .to_socket_addrs()
            .map_err(|e| io_error("resolve address", e))?
        {
            first_is_v6.get_or_insert(resolved.is_ipv6());
            if resolved.is_ipv6() {
                ipv6.push_back(resolved);
            } else {
                ipv4.push_back(resolved);
            }
        }
        if ipv6.is_empty() && ipv4.is_empty() {
            return Err(Error::new("TCP DNS returned no addresses"));
        }
        // Interleave families, preserving the resolver's order within each family.
        let first_is_v6 = first_is_v6.expect("nonempty address list");
        let mut addresses = Vec::with_capacity(ipv6.len() + ipv4.len());
        while !ipv6.is_empty() || !ipv4.is_empty() {
            for family in [first_is_v6, !first_is_v6] {
                let values = if family { &mut ipv6 } else { &mut ipv4 };
                if !values.is_empty() {
                    addresses.push(values.pop_front().expect("nonempty family"));
                }
            }
        }
        let deadline = Instant::now() + timeout;
        let (tx, rx) = mpsc::channel();
        let mut started = 0usize;
        let mut finished = 0usize;
        let mut last_error = None;
        let mut next_start = Instant::now();
        loop {
            let now = Instant::now();
            if now >= deadline {
                return Err(Error::new("TCP connect timed out"));
            }
            if started < addresses.len() && now >= next_start {
                let target = addresses[started];
                let remaining = deadline.saturating_duration_since(now);
                let sender = tx.clone();
                std::thread::spawn(move || {
                    let _ = sender.send(TcpStream::connect_timeout(&target, remaining));
                });
                started += 1;
                next_start = now + Duration::from_millis(250);
            }
            let until_next = if started < addresses.len() {
                next_start.saturating_duration_since(Instant::now())
            } else {
                timeout
            };
            let wait = deadline
                .saturating_duration_since(Instant::now())
                .min(until_next);
            match rx.recv_timeout(wait) {
                Ok(Ok(stream)) => return Ok(stream),
                Ok(Err(error)) => {
                    last_error = Some(error);
                    finished += 1;
                    if started == addresses.len() && finished == started {
                        break;
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }
        Err(Error::new(format!(
            "TCP connect failed: {}",
            last_error.map_or_else(|| "no connection succeeded".to_string(), |e| e.to_string())
        )))
    }

    #[spar_native::function]
    fn connect(address: &str, timeout_millis: i64) -> Result<Owned<TcpConnectionHandle>, Error> {
        if timeout_millis <= 0 {
            return Err(Error::range("TCP connect timeout must be positive"));
        }
        let socket = connect_happy(address, Duration::from_millis(timeout_millis as u64))?;
        Ok(Owned(TcpConnectionHandle::new(Connection::Plain(socket))?))
    }

    #[spar_native::function]
    fn connect_tls(
        address: &str,
        server_name: &str,
        ca_cert_path: &str,
        timeout_millis: i64,
    ) -> Result<Owned<TcpConnectionHandle>, Error> {
        if timeout_millis <= 0 {
            return Err(Error::range("TCP connect timeout must be positive"));
        }
        let config = client_config(ca_cert_path)?;
        let name = ServerName::try_from(server_name.to_owned())
            .map_err(|e| Error::new(format!("invalid TLS server name: {e}")))?;
        let mut socket = connect_happy(address, Duration::from_millis(timeout_millis as u64))?;
        let timeout = Some(Duration::from_millis(timeout_millis as u64));
        socket
            .set_read_timeout(timeout)
            .map_err(|e| io_error("set TLS timeout", e))?;
        socket
            .set_write_timeout(timeout)
            .map_err(|e| io_error("set TLS timeout", e))?;
        let mut session = ClientConnection::new(config, name)
            .map_err(|e| Error::new(format!("TLS session setup failed: {e}")))?;
        session
            .complete_io(&mut socket)
            .map_err(|e| io_error("TLS handshake", e))?;
        Ok(Owned(TcpConnectionHandle::new(Connection::TlsClient(
            StreamOwned::new(session, socket),
        ))?))
    }

    #[spar_native::function]
    fn peer_address(connection: &TcpConnectionHandle) -> Result<String, Error> {
        let guard = connection
            .state
            .lock()
            .map_err(|_| Error::new("TCP connection lock poisoned"))?;
        let stream = guard
            .as_ref()
            .ok_or_else(|| Error::new("TCP connection is closed"))?;
        stream
            .socket()
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
            .state
            .lock()
            .map_err(|_| Error::new("TCP connection lock poisoned"))?;
        let stream = guard
            .as_ref()
            .ok_or_else(|| Error::new("TCP connection is closed"))?;
        let timeout = if millis == 0 {
            None
        } else {
            Some(Duration::from_millis(millis as u64))
        };
        stream
            .socket()
            .set_read_timeout(timeout)
            .map_err(|e| io_error("set read timeout", e))
    }

    #[spar_native::function]
    fn connection_local_address(connection: &TcpConnectionHandle) -> Result<String, Error> {
        let guard = connection
            .state
            .lock()
            .map_err(|_| Error::new("TCP connection lock poisoned"))?;
        guard
            .as_ref()
            .ok_or_else(|| Error::new("TCP connection is closed"))?
            .socket()
            .local_addr()
            .map(|addr| addr.to_string())
            .map_err(|e| io_error("local address", e))
    }

    #[spar_native::function]
    fn set_write_timeout(connection: &TcpConnectionHandle, millis: i64) -> Result<(), Error> {
        if millis < 0 {
            return Err(Error::range("TCP write timeout cannot be negative"));
        }
        let guard = connection
            .state
            .lock()
            .map_err(|_| Error::new("TCP connection lock poisoned"))?;
        let timeout = if millis == 0 {
            None
        } else {
            Some(Duration::from_millis(millis as u64))
        };
        guard
            .as_ref()
            .ok_or_else(|| Error::new("TCP connection is closed"))?
            .socket()
            .set_write_timeout(timeout)
            .map_err(|e| io_error("set write timeout", e))
    }

    #[spar_native::function]
    fn set_no_delay(connection: &TcpConnectionHandle, enabled: bool) -> Result<(), Error> {
        let guard = connection
            .state
            .lock()
            .map_err(|_| Error::new("TCP connection lock poisoned"))?;
        guard
            .as_ref()
            .ok_or_else(|| Error::new("TCP connection is closed"))?
            .socket()
            .set_nodelay(enabled)
            .map_err(|e| io_error("set TCP_NODELAY", e))
    }

    #[spar_native::function]
    fn read(connection: &TcpConnectionHandle, max_bytes: i64) -> Result<Vec<u8>, Error> {
        if !(1..=1_048_576).contains(&max_bytes) {
            return Err(Error::range(
                "TCP read maxBytes must be between 1 and 1048576",
            ));
        }
        let mut bytes = vec![0; max_bytes as usize];
        let count = if let Some(reader) = &connection.plain_reader {
            let mut reader = reader
                .lock()
                .map_err(|_| Error::new("TCP reader lock poisoned"))?;
            reader.read(&mut bytes).map_err(|e| io_error("read", e))?
        } else {
            let mut guard = connection
                .state
                .lock()
                .map_err(|_| Error::new("TCP connection lock poisoned"))?;
            let stream = guard
                .as_mut()
                .ok_or_else(|| Error::new("TCP connection is closed"))?;
            stream.read(&mut bytes).map_err(|e| io_error("read", e))?
        };
        bytes.truncate(count);
        Ok(bytes)
    }

    #[spar_native::function]
    fn write_bytes(connection: &TcpConnectionHandle, data: &[u8]) -> Result<(), Error> {
        let mut guard = connection
            .state
            .lock()
            .map_err(|_| Error::new("TCP connection lock poisoned"))?;
        let stream = guard
            .as_mut()
            .ok_or_else(|| Error::new("TCP connection is closed"))?;
        stream.write_all(data).map_err(|e| io_error("write", e))?;
        stream.flush().map_err(|e| io_error("flush", e))
    }

    #[spar_native::function]
    fn write_text(connection: &TcpConnectionHandle, text: &str) -> Result<(), Error> {
        let mut guard = connection
            .state
            .lock()
            .map_err(|_| Error::new("TCP connection lock poisoned"))?;
        let stream = guard
            .as_mut()
            .ok_or_else(|| Error::new("TCP connection is closed"))?;
        stream
            .write_all(text.as_bytes())
            .map_err(|e| io_error("write", e))?;
        stream.flush().map_err(|e| io_error("flush", e))
    }

    #[spar_native::function]
    fn close(connection: &TcpConnectionHandle) -> Result<(), Error> {
        let mut guard = connection
            .state
            .lock()
            .map_err(|_| Error::new("TCP connection lock poisoned"))?;
        if let Some(stream) = guard.take() {
            stream.close().map_err(|e| io_error("close", e))?;
        }
        Ok(())
    }
    pub struct TcpSocketHandle(Mutex<Option<Socket>>);
    impl Resource for TcpSocketHandle {
        const SPAR_TYPE: &'static str = "TcpSocketHandle";
    }

    fn parse_addr(address: &str) -> Result<std::net::SocketAddr, Error> {
        address
            .parse()
            .map_err(|_| Error::new(format!("TCP invalid numeric socket address: {address}")))
    }

    fn with_socket<T>(
        handle: &TcpSocketHandle,
        action: &str,
        operation: impl FnOnce(&Socket) -> std::io::Result<T>,
    ) -> Result<T, Error> {
        let guard = handle
            .0
            .lock()
            .map_err(|_| Error::new("TCP socket lock poisoned"))?;
        let socket = guard
            .as_ref()
            .ok_or_else(|| Error::new("TCP socket has been consumed"))?;
        operation(socket).map_err(|e| io_error(action, e))
    }

    #[spar_native::function]
    fn socket_ipv4() -> Result<Owned<TcpSocketHandle>, Error> {
        let socket = Socket::new(Domain::IPV4, Type::STREAM, Some(Protocol::TCP))
            .map_err(|e| io_error("create IPv4 socket", e))?;
        Ok(Owned(TcpSocketHandle(Mutex::new(Some(socket)))))
    }

    #[spar_native::function]
    fn socket_ipv6() -> Result<Owned<TcpSocketHandle>, Error> {
        let socket = Socket::new(Domain::IPV6, Type::STREAM, Some(Protocol::TCP))
            .map_err(|e| io_error("create IPv6 socket", e))?;
        Ok(Owned(TcpSocketHandle(Mutex::new(Some(socket)))))
    }

    #[spar_native::function]
    fn socket_set_reuse_address(handle: &TcpSocketHandle, enabled: bool) -> Result<(), Error> {
        with_socket(handle, "set SO_REUSEADDR", |socket| {
            socket.set_reuse_address(enabled)
        })
    }

    #[spar_native::function]
    fn socket_set_reuse_port(handle: &TcpSocketHandle, enabled: bool) -> Result<(), Error> {
        #[cfg(all(
            unix,
            not(any(target_os = "solaris", target_os = "illumos", target_os = "cygwin"))
        ))]
        {
            with_socket(handle, "set SO_REUSEPORT", |socket| {
                socket.set_reuse_port(enabled)
            })
        }
        #[cfg(not(all(
            unix,
            not(any(target_os = "solaris", target_os = "illumos", target_os = "cygwin"))
        )))]
        {
            let _ = (handle, enabled);
            Err(Error::new("TCP SO_REUSEPORT unsupported on this platform"))
        }
    }

    #[spar_native::function]
    fn socket_set_ipv6_only(handle: &TcpSocketHandle, enabled: bool) -> Result<(), Error> {
        with_socket(handle, "set IPV6_V6ONLY", |socket| {
            socket.set_only_v6(enabled)
        })
    }

    #[spar_native::function]
    fn socket_set_receive_buffer_size(handle: &TcpSocketHandle, size: i64) -> Result<(), Error> {
        if size <= 0 {
            return Err(Error::range("TCP receive buffer size must be positive"));
        }
        with_socket(handle, "set SO_RCVBUF", |socket| {
            socket.set_recv_buffer_size(size as usize)
        })
    }

    #[spar_native::function]
    fn socket_set_send_buffer_size(handle: &TcpSocketHandle, size: i64) -> Result<(), Error> {
        if size <= 0 {
            return Err(Error::range("TCP send buffer size must be positive"));
        }
        with_socket(handle, "set SO_SNDBUF", |socket| {
            socket.set_send_buffer_size(size as usize)
        })
    }

    #[spar_native::function]
    fn socket_bind(handle: &TcpSocketHandle, address: &str) -> Result<(), Error> {
        let address = parse_addr(address)?;
        with_socket(handle, "bind", |socket| {
            socket.bind(&SockAddr::from(address))
        })
    }

    #[spar_native::function]
    fn socket_listen(
        handle: &TcpSocketHandle,
        backlog: i64,
    ) -> Result<Owned<TcpListenerHandle>, Error> {
        if !(1..=i32::MAX as i64).contains(&backlog) {
            return Err(Error::range("TCP backlog must be positive and fit i32"));
        }
        let mut guard = handle
            .0
            .lock()
            .map_err(|_| Error::new("TCP socket lock poisoned"))?;
        let socket = guard
            .as_ref()
            .ok_or_else(|| Error::new("TCP socket has been consumed"))?;
        socket
            .listen(backlog as i32)
            .map_err(|e| io_error("listen", e))?;
        socket
            .set_nonblocking(true)
            .map_err(|e| io_error("configure listener", e))?;
        let listener: StdListener = guard.take().expect("socket checked").into();
        Ok(Owned(TcpListenerHandle {
            socket: Mutex::new(Some(listener)),
            tls: None,
        }))
    }

    #[spar_native::function]
    fn socket_connect(
        handle: &TcpSocketHandle,
        address: &str,
        timeout_millis: i64,
    ) -> Result<Owned<TcpConnectionHandle>, Error> {
        if timeout_millis <= 0 {
            return Err(Error::range("TCP connect timeout must be positive"));
        }
        let address = parse_addr(address)?;
        let mut guard = handle
            .0
            .lock()
            .map_err(|_| Error::new("TCP socket lock poisoned"))?;
        let socket = guard
            .as_ref()
            .ok_or_else(|| Error::new("TCP socket has been consumed"))?;
        socket
            .connect_timeout(
                &SockAddr::from(address),
                Duration::from_millis(timeout_millis as u64),
            )
            .map_err(|e| io_error("connect", e))?;
        let stream: TcpStream = guard.take().expect("socket checked").into();
        Ok(Owned(TcpConnectionHandle::new(Connection::Plain(stream))?))
    }

    #[spar_native::function]
    fn read_exact(connection: &TcpConnectionHandle, count: i64) -> Result<Vec<u8>, Error> {
        if !(0..=16_777_216).contains(&count) {
            return Err(Error::range(
                "TCP readExact count must be between 0 and 16777216",
            ));
        }
        let mut bytes = vec![0; count as usize];
        if let Some(reader) = &connection.plain_reader {
            let mut reader = reader
                .lock()
                .map_err(|_| Error::new("TCP reader lock poisoned"))?;
            reader
                .read_exact(&mut bytes)
                .map_err(|e| io_error("readExact", e))?;
        } else {
            let mut guard = connection
                .state
                .lock()
                .map_err(|_| Error::new("TCP connection lock poisoned"))?;
            let stream = guard
                .as_mut()
                .ok_or_else(|| Error::new("TCP connection is closed"))?;
            stream
                .read_exact(&mut bytes)
                .map_err(|e| io_error("readExact", e))?;
        }
        Ok(bytes)
    }

    #[spar_native::function]
    fn write_partial(connection: &TcpConnectionHandle, data: &[u8]) -> Result<i64, Error> {
        let mut guard = connection
            .state
            .lock()
            .map_err(|_| Error::new("TCP connection lock poisoned"))?;
        let stream = guard
            .as_mut()
            .ok_or_else(|| Error::new("TCP connection is closed"))?;
        stream
            .write(data)
            .map(|n| n as i64)
            .map_err(|e| io_error("write", e))
    }

    #[spar_native::function]
    fn peek(connection: &TcpConnectionHandle, max_bytes: i64) -> Result<Vec<u8>, Error> {
        if !(1..=1_048_576).contains(&max_bytes) {
            return Err(Error::range(
                "TCP peek maxBytes must be between 1 and 1048576",
            ));
        }
        let reader = connection
            .plain_reader
            .as_ref()
            .ok_or_else(|| Error::new("TCP peek on TLS stream is unsupported"))?;
        let reader = reader
            .lock()
            .map_err(|_| Error::new("TCP reader lock poisoned"))?;
        let mut bytes = vec![0; max_bytes as usize];
        let count = reader.peek(&mut bytes).map_err(|e| io_error("peek", e))?;
        bytes.truncate(count);
        Ok(bytes)
    }

    #[spar_native::function]
    fn shutdown_read(connection: &TcpConnectionHandle) -> Result<(), Error> {
        let guard = connection
            .state
            .lock()
            .map_err(|_| Error::new("TCP connection lock poisoned"))?;
        let stream = guard
            .as_ref()
            .ok_or_else(|| Error::new("TCP connection is closed"))?;
        if !matches!(stream, Connection::Plain(_)) {
            return Err(Error::new("TCP half-close on TLS stream is unsupported"));
        }
        stream
            .socket()
            .shutdown(Shutdown::Read)
            .map_err(|e| io_error("shutdown read", e))
    }

    #[spar_native::function]
    fn shutdown_write(connection: &TcpConnectionHandle) -> Result<(), Error> {
        let guard = connection
            .state
            .lock()
            .map_err(|_| Error::new("TCP connection lock poisoned"))?;
        let stream = guard
            .as_ref()
            .ok_or_else(|| Error::new("TCP connection is closed"))?;
        if !matches!(stream, Connection::Plain(_)) {
            return Err(Error::new("TCP half-close on TLS stream is unsupported"));
        }
        stream
            .socket()
            .shutdown(Shutdown::Write)
            .map_err(|e| io_error("shutdown write", e))
    }

    #[spar_native::function]
    fn no_delay(connection: &TcpConnectionHandle) -> Result<bool, Error> {
        let guard = connection
            .state
            .lock()
            .map_err(|_| Error::new("TCP connection lock poisoned"))?;
        guard
            .as_ref()
            .ok_or_else(|| Error::new("TCP connection is closed"))?
            .socket()
            .nodelay()
            .map_err(|e| io_error("get TCP_NODELAY", e))
    }

    #[spar_native::function]
    fn set_keep_alive(connection: &TcpConnectionHandle, enabled: bool) -> Result<(), Error> {
        let guard = connection
            .state
            .lock()
            .map_err(|_| Error::new("TCP connection lock poisoned"))?;
        let stream = guard
            .as_ref()
            .ok_or_else(|| Error::new("TCP connection is closed"))?;
        SockRef::from(stream.socket())
            .set_keepalive(enabled)
            .map_err(|e| io_error("set SO_KEEPALIVE", e))
    }

    #[spar_native::function]
    fn set_keep_alive_timing(
        connection: &TcpConnectionHandle,
        idle_seconds: i64,
        interval_seconds: i64,
    ) -> Result<(), Error> {
        if idle_seconds <= 0 || interval_seconds <= 0 {
            return Err(Error::range("TCP keepalive timing must be positive"));
        }
        let guard = connection
            .state
            .lock()
            .map_err(|_| Error::new("TCP connection lock poisoned"))?;
        let stream = guard
            .as_ref()
            .ok_or_else(|| Error::new("TCP connection is closed"))?;
        let options = TcpKeepalive::new()
            .with_time(Duration::from_secs(idle_seconds as u64))
            .with_interval(Duration::from_secs(interval_seconds as u64));
        SockRef::from(stream.socket())
            .set_tcp_keepalive(&options)
            .map_err(|e| io_error("set TCP keepalive timing", e))
    }

    #[spar_native::function]
    fn set_receive_buffer_size(connection: &TcpConnectionHandle, size: i64) -> Result<(), Error> {
        if size <= 0 {
            return Err(Error::range("TCP receive buffer size must be positive"));
        }
        let guard = connection
            .state
            .lock()
            .map_err(|_| Error::new("TCP connection lock poisoned"))?;
        let stream = guard
            .as_ref()
            .ok_or_else(|| Error::new("TCP connection is closed"))?;
        SockRef::from(stream.socket())
            .set_recv_buffer_size(size as usize)
            .map_err(|e| io_error("set SO_RCVBUF", e))
    }

    #[spar_native::function]
    fn set_send_buffer_size(connection: &TcpConnectionHandle, size: i64) -> Result<(), Error> {
        if size <= 0 {
            return Err(Error::range("TCP send buffer size must be positive"));
        }
        let guard = connection
            .state
            .lock()
            .map_err(|_| Error::new("TCP connection lock poisoned"))?;
        let stream = guard
            .as_ref()
            .ok_or_else(|| Error::new("TCP connection is closed"))?;
        SockRef::from(stream.socket())
            .set_send_buffer_size(size as usize)
            .map_err(|e| io_error("set SO_SNDBUF", e))
    }

    #[spar_native::function]
    fn pending_error(connection: &TcpConnectionHandle) -> Result<String, Error> {
        let guard = connection
            .state
            .lock()
            .map_err(|_| Error::new("TCP connection lock poisoned"))?;
        let stream = guard
            .as_ref()
            .ok_or_else(|| Error::new("TCP connection is closed"))?;
        stream
            .socket()
            .take_error()
            .map(|e| e.map_or_else(String::new, |e| e.to_string()))
            .map_err(|e| io_error("get pending error", e))
    }

    #[spar_native::function]
    fn address_ip(address: &str) -> Result<String, Error> {
        Ok(parse_addr(address)?.ip().to_string())
    }

    #[spar_native::function]
    fn address_port(address: &str) -> Result<i64, Error> {
        Ok(i64::from(parse_addr(address)?.port()))
    }

    #[spar_native::function]
    fn address_is_ipv6(address: &str) -> Result<bool, Error> {
        Ok(parse_addr(address)?.is_ipv6())
    }

    #[spar_native::function]
    fn resolve_first(host: &str, port: i64) -> Result<String, Error> {
        if !(0..=65535).contains(&port) {
            return Err(Error::range("TCP port must be between 0 and 65535"));
        }
        (host, port as u16)
            .to_socket_addrs()
            .map_err(|e| io_error("resolve", e))?
            .next()
            .map(|address| address.to_string())
            .ok_or_else(|| Error::new("TCP DNS returned no addresses"))
    }
    #[spar_native::function]
    fn new_buffer(size: i64) -> Result<spar_native::Buf<u8>, Error> {
        if !(1..=16_777_216).contains(&size) {
            return Err(Error::range(
                "TCP buffer size must be between 1 and 16777216",
            ));
        }
        Ok(spar_native::Buf(vec![0; size as usize]))
    }

    #[spar_native::function]
    fn read_into(connection: &TcpConnectionHandle, buffer: &mut [u8]) -> Result<i64, Error> {
        if buffer.is_empty() {
            return Err(Error::range("TCP readInto buffer cannot be empty"));
        }
        if let Some(reader) = &connection.plain_reader {
            let mut reader = reader
                .lock()
                .map_err(|_| Error::new("TCP reader lock poisoned"))?;
            reader
                .read(buffer)
                .map(|n| n as i64)
                .map_err(|e| io_error("readInto", e))
        } else {
            let mut guard = connection
                .state
                .lock()
                .map_err(|_| Error::new("TCP connection lock poisoned"))?;
            let stream = guard
                .as_mut()
                .ok_or_else(|| Error::new("TCP connection is closed"))?;
            stream
                .read(buffer)
                .map(|n| n as i64)
                .map_err(|e| io_error("readInto", e))
        }
    }

    #[spar_native::function]
    fn buffer_prefix(buffer: &mut [u8], count: i64) -> Result<Vec<u8>, Error> {
        if count < 0 || count as usize > buffer.len() {
            return Err(Error::range("TCP bufferPrefix count out of range"));
        }
        Ok(buffer[..count as usize].to_vec())
    }

    #[spar_native::function]
    fn write_buffer(
        connection: &TcpConnectionHandle,
        buffer: &mut [u8],
        count: i64,
    ) -> Result<(), Error> {
        if count < 0 || count as usize > buffer.len() {
            return Err(Error::range("TCP writeBuffer count out of range"));
        }
        let mut guard = connection
            .state
            .lock()
            .map_err(|_| Error::new("TCP connection lock poisoned"))?;
        let stream = guard
            .as_mut()
            .ok_or_else(|| Error::new("TCP connection is closed"))?;
        stream
            .write_all(&buffer[..count as usize])
            .map_err(|e| io_error("writeBuffer", e))
    }
    pub struct DnsResultHandle(Vec<std::net::SocketAddr>);
    impl Resource for DnsResultHandle {
        const SPAR_TYPE: &'static str = "DnsResultHandle";
    }

    #[spar_native::function]
    fn resolve_all(host: &str, port: i64) -> Result<Owned<DnsResultHandle>, Error> {
        if !(0..=65535).contains(&port) {
            return Err(Error::range("TCP port must be between 0 and 65535"));
        }
        let addresses = (host, port as u16)
            .to_socket_addrs()
            .map_err(|e| io_error("resolve", e))?
            .collect::<Vec<_>>();
        if addresses.is_empty() {
            return Err(Error::new("TCP DNS returned no addresses"));
        }
        Ok(Owned(DnsResultHandle(addresses)))
    }

    #[spar_native::function]
    fn resolved_count(result: &DnsResultHandle) -> i64 {
        result.0.len() as i64
    }

    #[spar_native::function]
    fn resolved_address(result: &DnsResultHandle, index: i64) -> Result<String, Error> {
        let index =
            usize::try_from(index).map_err(|_| Error::range("TCP address index out of range"))?;
        result
            .0
            .get(index)
            .map(|address| address.to_string())
            .ok_or_else(|| Error::range("TCP address index out of range"))
    }
    #[spar_native::function]
    fn set_linger(connection: &TcpConnectionHandle, millis: i64) -> Result<(), Error> {
        if millis < 0 {
            return Err(Error::range("TCP linger cannot be negative"));
        }
        let guard = connection
            .state
            .lock()
            .map_err(|_| Error::new("TCP connection lock poisoned"))?;
        let stream = guard
            .as_ref()
            .ok_or_else(|| Error::new("TCP connection is closed"))?;
        let linger = if millis == 0 {
            None
        } else {
            Some(Duration::from_millis(millis as u64))
        };
        SockRef::from(stream.socket())
            .set_linger(linger)
            .map_err(|e| io_error("set SO_LINGER", e))
    }

    #[spar_native::function]
    fn abort_connection(connection: &TcpConnectionHandle) -> Result<(), Error> {
        let mut guard = connection
            .state
            .lock()
            .map_err(|_| Error::new("TCP connection lock poisoned"))?;
        let stream = guard
            .as_ref()
            .ok_or_else(|| Error::new("TCP connection is closed"))?;
        if !matches!(stream, Connection::Plain(_)) {
            return Err(Error::new("TCP abort on TLS stream is unsupported"));
        }
        SockRef::from(stream.socket())
            .set_linger(Some(Duration::ZERO))
            .map_err(|e| io_error("set abortive close", e))?;
        guard.take();
        Ok(())
    }

    #[spar_native::function]
    fn receive_buffer_size(connection: &TcpConnectionHandle) -> Result<i64, Error> {
        let guard = connection
            .state
            .lock()
            .map_err(|_| Error::new("TCP connection lock poisoned"))?;
        let stream = guard
            .as_ref()
            .ok_or_else(|| Error::new("TCP connection is closed"))?;
        SockRef::from(stream.socket())
            .recv_buffer_size()
            .map(|n| n as i64)
            .map_err(|e| io_error("get SO_RCVBUF", e))
    }

    #[spar_native::function]
    fn send_buffer_size(connection: &TcpConnectionHandle) -> Result<i64, Error> {
        let guard = connection
            .state
            .lock()
            .map_err(|_| Error::new("TCP connection lock poisoned"))?;
        let stream = guard
            .as_ref()
            .ok_or_else(|| Error::new("TCP connection is closed"))?;
        SockRef::from(stream.socket())
            .send_buffer_size()
            .map(|n| n as i64)
            .map_err(|e| io_error("get SO_SNDBUF", e))
    }

    #[spar_native::function]
    fn send_file(connection: &TcpConnectionHandle, path: &str) -> Result<i64, Error> {
        let mut file = File::open(path).map_err(|e| io_error("open file", e))?;
        let mut guard = connection
            .state
            .lock()
            .map_err(|_| Error::new("TCP connection lock poisoned"))?;
        let stream = guard
            .as_mut()
            .ok_or_else(|| Error::new("TCP connection is closed"))?;
        std::io::copy(&mut file, stream)
            .map(|n| n as i64)
            .map_err(|e| io_error("send file", e))
    }

    #[spar_native::function]
    fn supports_reuse_port() -> bool {
        cfg!(all(
            unix,
            not(any(
                target_os = "solaris",
                target_os = "illumos",
                target_os = "cygwin"
            ))
        ))
    }
    pub struct TcpReadHalfHandle(Mutex<Option<Arc<Mutex<TcpStream>>>>);
    impl Resource for TcpReadHalfHandle {
        const SPAR_TYPE: &'static str = "TcpReadHalfHandle";
    }
    pub struct TcpWriteHalfHandle(Mutex<Option<Arc<Mutex<Option<Connection>>>>>);
    impl Resource for TcpWriteHalfHandle {
        const SPAR_TYPE: &'static str = "TcpWriteHalfHandle";
    }

    #[spar_native::function]
    fn split_reader(connection: &TcpConnectionHandle) -> Result<Owned<TcpReadHalfHandle>, Error> {
        let guard = connection
            .state
            .lock()
            .map_err(|_| Error::new("TCP connection lock poisoned"))?;
        if !matches!(guard.as_ref(), Some(Connection::Plain(_))) {
            return Err(Error::new("TCP splitReader requires an open plain stream"));
        }
        let reader = connection
            .plain_reader
            .as_ref()
            .expect("plain reader exists")
            .clone();
        Ok(Owned(TcpReadHalfHandle(Mutex::new(Some(reader)))))
    }
    #[spar_native::function]
    fn split_writer(connection: &TcpConnectionHandle) -> Result<Owned<TcpWriteHalfHandle>, Error> {
        let guard = connection
            .state
            .lock()
            .map_err(|_| Error::new("TCP connection lock poisoned"))?;
        if !matches!(guard.as_ref(), Some(Connection::Plain(_))) {
            return Err(Error::new("TCP splitWriter requires an open plain stream"));
        }
        Ok(Owned(TcpWriteHalfHandle(Mutex::new(Some(
            connection.state.clone(),
        )))))
    }
    #[spar_native::function]
    fn read_half(reader: &TcpReadHalfHandle, max_bytes: i64) -> Result<Vec<u8>, Error> {
        if !(1..=1_048_576).contains(&max_bytes) {
            return Err(Error::range(
                "TCP readHalf maxBytes must be between 1 and 1048576",
            ));
        }
        let guard = reader
            .0
            .lock()
            .map_err(|_| Error::new("TCP reader lock poisoned"))?;
        let shared = guard
            .as_ref()
            .ok_or_else(|| Error::new("TCP reader is closed"))?;
        let mut stream = shared
            .lock()
            .map_err(|_| Error::new("TCP reader lock poisoned"))?;
        let mut bytes = vec![0; max_bytes as usize];
        let count = stream
            .read(&mut bytes)
            .map_err(|e| io_error("readHalf", e))?;
        bytes.truncate(count);
        Ok(bytes)
    }
    #[spar_native::function]
    fn read_half_exact(reader: &TcpReadHalfHandle, count: i64) -> Result<Vec<u8>, Error> {
        if !(0..=16_777_216).contains(&count) {
            return Err(Error::range(
                "TCP readHalfExact count must be between 0 and 16777216",
            ));
        }
        let guard = reader
            .0
            .lock()
            .map_err(|_| Error::new("TCP reader lock poisoned"))?;
        let shared = guard
            .as_ref()
            .ok_or_else(|| Error::new("TCP reader is closed"))?;
        let mut stream = shared
            .lock()
            .map_err(|_| Error::new("TCP reader lock poisoned"))?;
        let mut bytes = vec![0; count as usize];
        stream
            .read_exact(&mut bytes)
            .map_err(|e| io_error("readHalfExact", e))?;
        Ok(bytes)
    }
    #[spar_native::function]
    fn write_half_bytes(writer: &TcpWriteHalfHandle, data: &[u8]) -> Result<(), Error> {
        let guard = writer
            .0
            .lock()
            .map_err(|_| Error::new("TCP writer lock poisoned"))?;
        let shared = guard
            .as_ref()
            .ok_or_else(|| Error::new("TCP writer is closed"))?;
        let mut state = shared
            .lock()
            .map_err(|_| Error::new("TCP connection lock poisoned"))?;
        let stream = state
            .as_mut()
            .ok_or_else(|| Error::new("TCP connection is closed"))?;
        stream
            .write_all(data)
            .map_err(|e| io_error("writeHalfBytes", e))
    }
    #[spar_native::function]
    fn write_half_text(writer: &TcpWriteHalfHandle, text: &str) -> Result<(), Error> {
        let guard = writer
            .0
            .lock()
            .map_err(|_| Error::new("TCP writer lock poisoned"))?;
        let shared = guard
            .as_ref()
            .ok_or_else(|| Error::new("TCP writer is closed"))?;
        let mut state = shared
            .lock()
            .map_err(|_| Error::new("TCP connection lock poisoned"))?;
        let stream = state
            .as_mut()
            .ok_or_else(|| Error::new("TCP connection is closed"))?;
        stream
            .write_all(text.as_bytes())
            .map_err(|e| io_error("writeHalfText", e))
    }
    #[spar_native::function]
    fn close_read_half(reader: &TcpReadHalfHandle) -> Result<(), Error> {
        reader
            .0
            .lock()
            .map_err(|_| Error::new("TCP reader lock poisoned"))?
            .take();
        Ok(())
    }
    #[spar_native::function]
    fn close_write_half(writer: &TcpWriteHalfHandle) -> Result<(), Error> {
        writer
            .0
            .lock()
            .map_err(|_| Error::new("TCP writer lock poisoned"))?
            .take();
        Ok(())
    }
    // Reactor-backed TCP state. Async completions carry only integers through ABI 1;
    // accepted streams and read buffers stay in typed native resources until taken.
    use spar_native::{AsyncCompleter, AsyncTask};
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::sync::OnceLock;
    use tokio::net::{TcpListener as TokioListener, TcpStream as TokioStream};
    use tokio::runtime::{Builder, Runtime};
    use tokio::sync::Mutex as AsyncMutex;

    static TCP_RUNTIME: OnceLock<Runtime> = OnceLock::new();
    fn tcp_runtime() -> &'static Runtime {
        TCP_RUNTIME.get_or_init(|| {
            Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .expect("TCP reactor could not start")
        })
    }

    struct Busy(Arc<AtomicBool>);
    impl Busy {
        fn acquire(flag: &Arc<AtomicBool>, action: &str) -> Result<Self, Error> {
            flag.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .map_err(|_| Error::new(format!("TCP {action} already in progress")))?;
            Ok(Self(flag.clone()))
        }
    }
    impl Drop for Busy {
        fn drop(&mut self) {
            self.0.store(false, Ordering::Release);
        }
    }

    // Drop the in-flight guard before settling a promise. An awaiting Spar task may
    // call the same operation again as soon as completion becomes visible.
    struct BusyCompleter {
        inner: AsyncCompleter<i64>,
        busy: Option<Busy>,
    }
    impl BusyCompleter {
        fn new(inner: AsyncCompleter<i64>, busy: Option<Busy>) -> Self {
            Self { inner, busy }
        }
        fn is_cancelled(&self) -> bool {
            self.inner.is_cancelled()
        }
        fn complete(mut self, value: i64) -> Result<(), Error> {
            drop(self.busy.take());
            self.inner.complete(value)
        }
        fn fail(mut self, message: impl AsRef<str>) -> Result<(), Error> {
            drop(self.busy.take());
            self.inner.fail(message)
        }
    }

    fn async_limit(value: i64, name: &str) -> Result<usize, Error> {
        if !(1..=16 * 1024 * 1024).contains(&value) {
            return Err(Error::range(format!(
                "{name} must be between 1 and 16777216"
            )));
        }
        Ok(value as usize)
    }
    fn async_timeout(value: i64) -> Result<Duration, Error> {
        if !(1..=3_600_000).contains(&value) {
            return Err(Error::range("timeoutMillis must be between 1 and 3600000"));
        }
        Ok(Duration::from_millis(value as u64))
    }

    pub struct AsyncTcpListenerHandle {
        socket: Mutex<Option<Arc<TokioListener>>>,
        pending: Arc<Mutex<Option<TokioStream>>>,
        busy: Arc<AtomicBool>,
        closed: Arc<AtomicBool>,
        generation: Arc<AtomicU64>,
    }
    impl Resource for AsyncTcpListenerHandle {
        const SPAR_TYPE: &'static str = "AsyncTcpListenerHandle";
    }

    pub struct AsyncTcpConnectHandle {
        pending: Arc<Mutex<Option<TokioStream>>>,
        busy: Arc<AtomicBool>,
        closed: Arc<AtomicBool>,
        generation: Arc<AtomicU64>,
    }
    impl Resource for AsyncTcpConnectHandle {
        const SPAR_TYPE: &'static str = "AsyncTcpConnectHandle";
    }

    pub struct AsyncTcpConnectionHandle {
        socket: Mutex<Option<Arc<TokioStream>>>,
        read_result: Arc<Mutex<Option<Vec<u8>>>>,
        read_busy: Arc<AtomicBool>,
        write_lock: Arc<AsyncMutex<()>>,
        closed: Arc<AtomicBool>,
        read_generation: Arc<AtomicU64>,
        write_generation: Arc<AtomicU64>,
    }
    impl Resource for AsyncTcpConnectionHandle {
        const SPAR_TYPE: &'static str = "AsyncTcpConnectionHandle";
    }
    impl AsyncTcpConnectionHandle {
        fn from_stream(stream: TokioStream) -> Self {
            Self {
                socket: Mutex::new(Some(Arc::new(stream))),
                read_result: Arc::new(Mutex::new(None)),
                read_busy: Arc::new(AtomicBool::new(false)),
                write_lock: Arc::new(AsyncMutex::new(())),
                closed: Arc::new(AtomicBool::new(false)),
                read_generation: Arc::new(AtomicU64::new(0)),
                write_generation: Arc::new(AtomicU64::new(0)),
            }
        }
    }

    fn cancelled(
        completer: &BusyCompleter,
        closed: &AtomicBool,
        generation: &AtomicU64,
        ticket: u64,
    ) -> bool {
        completer.is_cancelled()
            || closed.load(Ordering::Acquire)
            || generation.load(Ordering::Acquire) != ticket
    }

    #[spar_native::function]
    fn listen_async(address: &str) -> Result<Owned<AsyncTcpListenerHandle>, Error> {
        let socket = StdListener::bind(address).map_err(|e| io_error("listenAsync", e))?;
        socket
            .set_nonblocking(true)
            .map_err(|e| io_error("listenAsync", e))?;
        let _enter = tcp_runtime().enter();
        let socket = TokioListener::from_std(socket).map_err(|e| io_error("listenAsync", e))?;
        Ok(Owned(AsyncTcpListenerHandle {
            socket: Mutex::new(Some(Arc::new(socket))),
            pending: Arc::new(Mutex::new(None)),
            busy: Arc::new(AtomicBool::new(false)),
            closed: Arc::new(AtomicBool::new(false)),
            generation: Arc::new(AtomicU64::new(0)),
        }))
    }

    #[spar_native::function]
    fn async_listener_local_address(listener: &AsyncTcpListenerHandle) -> Result<String, Error> {
        listener
            .socket
            .lock()
            .unwrap()
            .as_ref()
            .ok_or_else(|| Error::new("TCP listener is closed"))?
            .local_addr()
            .map(|a| a.to_string())
            .map_err(|e| io_error("asyncListenerLocalAddress", e))
    }

    #[spar_native::function(asynchronous)]
    fn async_accept(listener: &AsyncTcpListenerHandle) -> Result<AsyncTask<i64>, Error> {
        if listener.closed.load(Ordering::Acquire) {
            return Err(Error::new("TCP listener is closed"));
        }
        if listener.pending.lock().unwrap().is_some() {
            return Err(Error::new("accepted connection must be taken first"));
        }
        let busy = Busy::acquire(&listener.busy, "accept")?;
        let socket = listener
            .socket
            .lock()
            .unwrap()
            .as_ref()
            .cloned()
            .ok_or_else(|| Error::new("TCP listener is closed"))?;
        let pending = listener.pending.clone();
        let closed = listener.closed.clone();
        let generation = listener.generation.clone();
        let ticket = generation.load(Ordering::Acquire);
        Ok(AsyncTask::new(move |completer| {
            tcp_runtime().spawn(async move {
            let completer = BusyCompleter::new(completer, Some(busy));
            loop {
                tokio::select! {
                    result = socket.accept() => {
                        match result {
                            Ok((stream, _)) if !cancelled(&completer, &closed, &generation, ticket) => {
                                *pending.lock().unwrap() = Some(stream);
                                let _ = completer.complete(1);
                            }
                            Ok(_) => { let _ = completer.fail("TCP accept cancelled"); }
                            Err(e) => { let _ = completer.fail(io_error("asyncAccept", e).message); }
                        }
                        break;
                    }
                    _ = tokio::time::sleep(Duration::from_millis(20)) => {
                        if cancelled(&completer, &closed, &generation, ticket) {
                            let _ = completer.fail("TCP accept cancelled");
                            break;
                        }
                    }
                }
            }
        });
        }))
    }

    #[spar_native::function]
    fn async_take_accepted(
        listener: &AsyncTcpListenerHandle,
    ) -> Result<Owned<AsyncTcpConnectionHandle>, Error> {
        let stream = listener
            .pending
            .lock()
            .unwrap()
            .take()
            .ok_or_else(|| Error::new("no accepted TCP connection is ready"))?;
        Ok(Owned(AsyncTcpConnectionHandle::from_stream(stream)))
    }

    #[spar_native::function]
    fn close_async_listener(listener: &AsyncTcpListenerHandle) -> Result<(), Error> {
        listener.closed.store(true, Ordering::Release);
        listener.socket.lock().unwrap().take();
        listener.pending.lock().unwrap().take();
        Ok(())
    }

    #[spar_native::function]
    fn new_async_connect() -> Owned<AsyncTcpConnectHandle> {
        Owned(AsyncTcpConnectHandle {
            pending: Arc::new(Mutex::new(None)),
            busy: Arc::new(AtomicBool::new(false)),
            closed: Arc::new(AtomicBool::new(false)),
            generation: Arc::new(AtomicU64::new(0)),
        })
    }

    #[spar_native::function(asynchronous)]
    fn async_connect(
        handle: &AsyncTcpConnectHandle,
        address: &str,
        timeout_millis: i64,
    ) -> Result<AsyncTask<i64>, Error> {
        let timeout = async_timeout(timeout_millis)?;
        if handle.closed.load(Ordering::Acquire) {
            return Err(Error::new("TCP connect handle is closed"));
        }
        if handle.pending.lock().unwrap().is_some() {
            return Err(Error::new("connected socket must be taken first"));
        }
        let busy = Busy::acquire(&handle.busy, "connect")?;
        let address = address.to_owned();
        let pending = handle.pending.clone();
        let closed = handle.closed.clone();
        let generation = handle.generation.clone();
        let ticket = generation.load(Ordering::Acquire);
        Ok(AsyncTask::new(move |completer| {
            tcp_runtime().spawn(async move {
            let completer = BusyCompleter::new(completer, Some(busy));
            let connect = tokio::time::timeout(timeout, TokioStream::connect(address.as_str()));
            tokio::pin!(connect);
            loop {
                tokio::select! {
                    result = &mut connect => {
                        match result {
                            Ok(Ok(stream)) if !cancelled(&completer, &closed, &generation, ticket) => {
                                *pending.lock().unwrap() = Some(stream);
                                let _ = completer.complete(1);
                            }
                            Ok(Ok(_)) => { let _ = completer.fail("TCP connect cancelled"); }
                            Ok(Err(e)) => { let _ = completer.fail(io_error("asyncConnect", e).message); }
                            Err(_) => { let _ = completer.fail("TCP connect timed out"); }
                        }
                        break;
                    }
                    _ = tokio::time::sleep(Duration::from_millis(20)) => {
                        if cancelled(&completer, &closed, &generation, ticket) {
                            let _ = completer.fail("TCP connect cancelled");
                            break;
                        }
                    }
                }
            }
        });
        }))
    }

    #[spar_native::function]
    fn async_take_connected(
        handle: &AsyncTcpConnectHandle,
    ) -> Result<Owned<AsyncTcpConnectionHandle>, Error> {
        let stream = handle
            .pending
            .lock()
            .unwrap()
            .take()
            .ok_or_else(|| Error::new("no connected TCP socket is ready"))?;
        Ok(Owned(AsyncTcpConnectionHandle::from_stream(stream)))
    }

    #[spar_native::function]
    fn close_async_connect(handle: &AsyncTcpConnectHandle) -> Result<(), Error> {
        handle.closed.store(true, Ordering::Release);
        handle.pending.lock().unwrap().take();
        Ok(())
    }

    #[spar_native::function(asynchronous)]
    fn async_read(
        connection: &AsyncTcpConnectionHandle,
        max_bytes: i64,
    ) -> Result<AsyncTask<i64>, Error> {
        let max_bytes = async_limit(max_bytes, "maxBytes")?;
        if connection.closed.load(Ordering::Acquire) {
            return Err(Error::new("TCP connection is closed"));
        }
        if connection.read_result.lock().unwrap().is_some() {
            return Err(Error::new("read buffer must be taken first"));
        }
        let busy = Busy::acquire(&connection.read_busy, "read")?;
        let socket = connection
            .socket
            .lock()
            .unwrap()
            .as_ref()
            .cloned()
            .ok_or_else(|| Error::new("TCP connection is closed"))?;
        let result_slot = connection.read_result.clone();
        let closed = connection.closed.clone();
        let generation = connection.read_generation.clone();
        let ticket = generation.load(Ordering::Acquire);
        Ok(AsyncTask::new(move |completer| {
            tcp_runtime().spawn(async move {
            let completer = BusyCompleter::new(completer, Some(busy));
            let mut bytes = vec![0u8; max_bytes];
            loop {
                tokio::select! {
                    ready = socket.readable() => {
                        match ready {
                            Ok(()) => match socket.try_read(&mut bytes) {
                                Ok(count) if !cancelled(&completer, &closed, &generation, ticket) => {
                                    bytes.truncate(count);
                                    *result_slot.lock().unwrap() = Some(bytes);
                                    let _ = completer.complete(count as i64);
                                    break;
                                }
                                Ok(_) => { let _ = completer.fail("TCP read cancelled"); break; }
                                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => continue,
                                Err(e) => { let _ = completer.fail(io_error("asyncRead", e).message); break; }
                            },
                            Err(e) => { let _ = completer.fail(io_error("asyncRead", e).message); break; }
                        }
                    }
                    _ = tokio::time::sleep(Duration::from_millis(20)) => {
                        if cancelled(&completer, &closed, &generation, ticket) { let _ = completer.fail("TCP read cancelled"); break; }
                    }
                }
            }
        });
        }))
    }

    #[spar_native::function]
    fn async_take_read(connection: &AsyncTcpConnectionHandle) -> Result<Vec<u8>, Error> {
        connection
            .read_result
            .lock()
            .unwrap()
            .take()
            .ok_or_else(|| Error::new("no TCP read buffer is ready"))
    }

    #[spar_native::function(asynchronous)]
    fn async_write_all(
        connection: &AsyncTcpConnectionHandle,
        data: &[u8],
    ) -> Result<AsyncTask<i64>, Error> {
        if data.len() > 16 * 1024 * 1024 {
            return Err(Error::range("write buffer exceeds 16777216 bytes"));
        }
        if connection.closed.load(Ordering::Acquire) {
            return Err(Error::new("TCP connection is closed"));
        }
        let data = data.to_vec();
        let socket = connection
            .socket
            .lock()
            .unwrap()
            .as_ref()
            .cloned()
            .ok_or_else(|| Error::new("TCP connection is closed"))?;
        let write_lock = connection.write_lock.clone();
        let closed = connection.closed.clone();
        let generation = connection.write_generation.clone();
        let ticket = generation.load(Ordering::Acquire);
        Ok(AsyncTask::new(move |completer| {
            tcp_runtime().spawn(async move {
            let completer = BusyCompleter::new(completer, None);
            let guard = write_lock.lock().await;
            let mut written = 0;
            while written < data.len() {
                if cancelled(&completer, &closed, &generation, ticket) { let _ = completer.fail("TCP write cancelled"); return; }
                tokio::select! {
                    ready = socket.writable() => {
                        match ready {
                            Ok(()) => match socket.try_write(&data[written..]) {
                                Ok(0) => { let _ = completer.fail("TCP write returned zero"); return; }
                                Ok(count) => written += count,
                                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => continue,
                                Err(e) => { let _ = completer.fail(io_error("asyncWriteAll", e).message); return; }
                            },
                            Err(e) => { let _ = completer.fail(io_error("asyncWriteAll", e).message); return; }
                        }
                    }
                    _ = tokio::time::sleep(Duration::from_millis(20)) => {}
                }
            }
            drop(guard);
            if cancelled(&completer, &closed, &generation, ticket) { let _ = completer.fail("TCP write cancelled"); }
            else { let _ = completer.complete(written as i64); }
        });
        }))
    }

    #[spar_native::function]
    fn cancel_async_accept(listener: &AsyncTcpListenerHandle) -> Result<(), Error> {
        listener.generation.fetch_add(1, Ordering::AcqRel);
        Ok(())
    }

    #[spar_native::function]
    fn cancel_async_connect(handle: &AsyncTcpConnectHandle) -> Result<(), Error> {
        handle.generation.fetch_add(1, Ordering::AcqRel);
        Ok(())
    }

    #[spar_native::function]
    fn cancel_async_read(connection: &AsyncTcpConnectionHandle) -> Result<(), Error> {
        connection.read_generation.fetch_add(1, Ordering::AcqRel);
        Ok(())
    }

    #[spar_native::function]
    fn cancel_async_write(connection: &AsyncTcpConnectionHandle) -> Result<(), Error> {
        connection.write_generation.fetch_add(1, Ordering::AcqRel);
        Ok(())
    }

    #[spar_native::function]
    fn close_async_connection(connection: &AsyncTcpConnectionHandle) -> Result<(), Error> {
        connection.closed.store(true, Ordering::Release);
        connection.read_result.lock().unwrap().take();
        let Some(socket) = connection.socket.lock().unwrap().take() else {
            return Ok(());
        };
        match SockRef::from(&*socket).shutdown(Shutdown::Both) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotConnected => Ok(()),
            Err(e) => Err(io_error("closeAsync", e)),
        }
    }
}
