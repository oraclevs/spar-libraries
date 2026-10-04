use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant};

static NEXT: AtomicUsize = AtomicUsize::new(0);

fn spar_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}
struct App(PathBuf);
impl App {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "spar-tcp-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(path.join("src")).unwrap();
        let package = Path::new(env!("CARGO_MANIFEST_DIR"));
        fs::write(path.join("spar.package.spar"), format!(
            "struct Package {{ name: str = \"tcp-probe\"; version: str = \"0.1.0\"; kind: str = \"application\"; entry: str = \"src/main.spar\"; }};\nstruct Dependencies {{ tcp: str = \"path:{}\"; }};\n", spar_path(package)
        )).unwrap();
        let app = Self(path);
        app.run(&["install"]);
        app
    }
    fn write(&self, code: &str) {
        fs::write(self.0.join("src/main.spar"), code).unwrap();
    }
    fn binary() -> String {
        std::env::var("SPAR_BIN").unwrap_or_else(|_| "spar".into())
    }
    fn run(&self, args: &[&str]) {
        let output = Command::new(Self::binary())
            .args(args)
            .current_dir(&self.0)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "spar {args:?}:\n{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
impl Drop for App {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn client_byte_stream_and_half_close() {
    let server = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = server.local_addr().unwrap();
    let peer = thread::spawn(move || {
        let (mut stream, _) = server.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut request = [0; 4];
        stream.read_exact(&mut request).unwrap();
        assert_eq!(&request, b"ping");
        stream.read_exact(&mut request).unwrap();
        assert_eq!(&request, b"file");
        stream.write_all(b"pongpong").unwrap();
        let mut tail = [0; 1];
        assert_eq!(stream.read(&mut tail).unwrap(), 0);
    });
    let app = App::new();
    let payload = app.0.join("payload.txt");
    fs::write(&payload, b"file").unwrap();
    app.write(&format!(r#"
import pkg {{ socketIpv4, socketBind, socketConnect, setNoDelay, noDelay, setKeepAlive,
    setReadTimeout, writeText, peek, readExact, newBuffer, readInto, bufferPrefix, sendFile, receiveBufferSize, sendBufferSize, splitReader, splitWriter, readHalfExact, writeHalfText, closeReadHalf, closeWriteHalf, shutdownWrite, close,
    addressIp, addressPort, addressIsIpv6, resolveAddresses, parseSocketAddress, formatSocketAddress }} from "tcp";
fn main() -> int {{
    var address = parseSocketAddress(value: "{addr}");
    if formatSocketAddress(value: address) != "{addr}" {{ panic(message: "address formatting"); }}
    if addressIp(address: "{addr}") != "127.0.0.1" {{ panic(message: "address ip"); }}
    if addressPort(address: "{addr}") != {port} {{ panic(message: "address port"); }}
    if !addressIsIpv6(address: "[::1]:8080") {{ panic(message: "IPv6 parse"); }}
    if resolveAddresses(host: "localhost", port: 8080).length() == 0 {{ panic(message: "DNS"); }}
    var socket = socketIpv4();
    socketBind(handle: socket, address: "127.0.0.1:0");
    var conn = socketConnect(handle: socket, address: "{addr}", timeoutMillis: 5000);
    setNoDelay(connection: conn, enabled: true);
    if !noDelay(connection: conn) {{ panic(message: "TCP_NODELAY"); }}
    setKeepAlive(connection: conn, enabled: true);
    setReadTimeout(connection: conn, millis: 5000);
    if receiveBufferSize(connection: conn) <= 0 || sendBufferSize(connection: conn) <= 0 {{ panic(message: "socket buffers"); }}
    var reader = splitReader(connection: conn);
    var writer = splitWriter(connection: conn);
    writeHalfText(writer: writer, text: "ping");
    if sendFile(connection: conn, path: "{payload}") != 4 {{ panic(message: "sendFile"); }}
    if peek(connection: conn, maxBytes: 4).toUtf8().unwrap() != "pong" {{ panic(message: "peek"); }}
    if readHalfExact(reader: reader, count: 4).toUtf8().unwrap() != "pong" {{ panic(message: "readHalfExact"); }}
    closeReadHalf(reader: reader);
    closeWriteHalf(writer: writer);
    var buffer = newBuffer(size: 16);
    var count: int = readInto(connection: conn, buffer: buffer);
    if count != 4 || bufferPrefix(buffer: buffer, count: count).toUtf8().unwrap() != "pong" {{ panic(message: "readInto"); }}
    shutdownWrite(connection: conn);
    close(connection: conn);
    return 0;
}};
"#, addr=addr, port=addr.port(), payload=spar_path(&payload)));
    app.run(&["check", "src/main.spar"]);
    app.run(&["exec", "src/main.spar"]);
    peer.join().unwrap();
}

#[test]
fn configured_listener_accepts_and_closes() {
    let probe = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = probe.local_addr().unwrap();
    drop(probe);
    let app = App::new();
    app.write(&format!(r#"
import pkg {{ socketIpv4, socketSetReuseAddress, socketBind, socketListen,
    localAddress, acceptTimeout, newBuffer, readInto, bufferPrefix, writeBuffer, close, closeListener }} from "tcp";
fn main() -> int {{
    var socket = socketIpv4();
    socketSetReuseAddress(handle: socket, enabled: true);
    socketBind(handle: socket, address: "{addr}");
    var listener = socketListen(handle: socket, backlog: 16);
    if localAddress(listener: listener) != "{addr}" {{ panic(message: "local address"); }}
    var conn = acceptTimeout(listener: listener, millis: 5000);
    var buffer = newBuffer(size: 16);
    var count: int = readInto(connection: conn, buffer: buffer);
    if count != 4 || bufferPrefix(buffer: buffer, count: count).toUtf8().unwrap() != "ping" {{ panic(message: "server read"); }}
    writeBuffer(connection: conn, buffer: buffer, count: count);
    close(connection: conn);
    closeListener(listener: listener);
    return 0;
}};
"#));
    app.run(&["check", "src/main.spar"]);
    let child = Command::new(App::binary())
        .args(["exec", "src/main.spar"])
        .current_dir(&app.0)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut stream = loop {
        match TcpStream::connect(addr) {
            Ok(stream) => break stream,
            Err(_) if Instant::now() < deadline => thread::sleep(Duration::from_millis(10)),
            Err(error) => panic!("server did not listen: {error}"),
        }
    };
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    stream.write_all(b"ping").unwrap();
    let mut reply = [0; 4];
    stream.read_exact(&mut reply).unwrap();
    assert_eq!(&reply, b"ping");
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn tls_client_verifies_name_and_exchanges_bytes() {
    use rustls::{ServerConfig, ServerConnection, StreamOwned};
    use std::fs::File;
    use std::io::BufReader;
    use std::sync::Arc;

    let app = App::new();
    let cert = app.0.join("server.crt");
    let key = app.0.join("server.key");
    let generated = Command::new("openssl")
        .args(["req", "-x509", "-newkey", "rsa:2048", "-nodes",
            "-days", "1", "-subj", "/CN=localhost",
            "-addext", "subjectAltName=DNS:localhost",
            "-addext", "basicConstraints=critical,CA:FALSE",
            "-addext", "keyUsage=critical,digitalSignature,keyEncipherment",
            "-addext", "extendedKeyUsage=serverAuth",
            "-keyout"])
        .arg(&key)
        .arg("-out")
        .arg(&cert)
        .output()
        .expect("openssl is required for the TLS integration test");
    assert!(generated.status.success(), "openssl: {}", String::from_utf8_lossy(&generated.stderr));
    let certs = rustls_pemfile::certs(&mut BufReader::new(File::open(&cert).unwrap()))
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let private_key = rustls_pemfile::private_key(&mut BufReader::new(File::open(&key).unwrap()))
        .unwrap()
        .unwrap();
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let config = ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(certs, private_key)
        .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let config = Arc::new(config);
        let (stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let session = ServerConnection::new(config.clone()).unwrap();
        let mut tls = StreamOwned::new(session, stream);
        let mut ping = [0; 4];
        tls.read_exact(&mut ping).unwrap();
        assert_eq!(&ping, b"ping");
        tls.write_all(b"pong").unwrap();
        tls.flush().unwrap();
        drop(tls);
        let (stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let session = ServerConnection::new(config).unwrap();
        let mut tls = StreamOwned::new(session, stream);
        let mut byte = [0; 1];
        assert!(tls.read(&mut byte).is_err());
    });
    app.write(&format!(r#"
import pkg {{ connectTls, writeText, readExact, close }} from "tcp";
fn main() -> int {{
    var stream = connectTls(address: "{addr}", serverName: "localhost", caCertPath: "{}", timeoutMillis: 5000);
    writeText(connection: stream, text: "ping");
    if readExact(connection: stream, count: 4).toUtf8().unwrap() != "pong" {{ panic(message: "TLS reply"); }}
    close(connection: stream);
    return 0;
}};
"#, spar_path(&cert)));
    app.run(&["check", "src/main.spar"]);
    app.run(&["exec", "src/main.spar"]);
    app.write(&format!(r#"
import pkg {{ connectTls }} from "tcp";
fn main() -> int {{
    connectTls(address: "{addr}", serverName: "wrong.example", caCertPath: "{}", timeoutMillis: 5000);
    return 0;
}};
"#, spar_path(&cert)));
    app.run(&["check", "src/main.spar"]);
    let rejected = Command::new(App::binary())
        .args(["exec", "src/main.spar"])
        .current_dir(&app.0)
        .output()
        .unwrap();
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("invalid peer certificate"));
    server.join().unwrap();
}

#[test]
fn listener_timeout_and_close_are_reported() {
    let app = App::new();
    app.write(
        r#"
import pkg { listen, acceptTimeout } from "tcp";
fn main() -> int {
    var listener = listen(address: "127.0.0.1:0");
    acceptTimeout(listener: listener, millis: 25);
    return 0;
};
"#,
    );
    app.run(&["check", "src/main.spar"]);
    let output = Command::new(App::binary())
        .args(["exec", "src/main.spar"])
        .current_dir(&app.0)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("TCP accept timed out"));
    app.write(
        r#"
import pkg { listen, closeListener, accept } from "tcp";
fn main() -> int {
    var listener = listen(address: "127.0.0.1:0");
    closeListener(listener: listener);
    accept(listener: listener);
    return 0;
};
"#,
    );
    app.run(&["check", "src/main.spar"]);
    let output = Command::new(App::binary())
        .args(["exec", "src/main.spar"])
        .current_dir(&app.0)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("TCP listener is closed"));
}

#[test]
fn ipv6_only_listener() {
    let Ok(probe) = TcpListener::bind("[::1]:0") else {
        return;
    };
    let address = probe.local_addr().unwrap();
    drop(probe);
    let app = App::new();
    app.write(&format!(r#"
import pkg {{ socketIpv6, socketSetIpv6Only, socketBind, socketListen, acceptTimeout,
    readExact, writeText, close, closeListener }} from "tcp";
fn main() -> int {{
    var socket = socketIpv6();
    socketSetIpv6Only(handle: socket, enabled: true);
    socketBind(handle: socket, address: "{address}");
    var listener = socketListen(handle: socket, backlog: 8);
    var stream = acceptTimeout(listener: listener, millis: 5000);
    if readExact(connection: stream, count: 4).toUtf8().unwrap() != "ping" {{ panic(message: "IPv6 read"); }}
    writeText(connection: stream, text: "pong");
    close(connection: stream);
    closeListener(listener: listener);
    return 0;
}};
"#));
    app.run(&["check", "src/main.spar"]);
    let child = Command::new(App::binary())
        .args(["exec", "src/main.spar"])
        .current_dir(&app.0)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut stream = loop {
        match TcpStream::connect(address) {
            Ok(stream) => break stream,
            Err(_) if Instant::now() < deadline => thread::sleep(Duration::from_millis(10)),
            Err(error) => panic!("IPv6 server did not listen: {error}"),
        }
    };
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    stream.write_all(b"ping").unwrap();
    let mut reply = [0; 4];
    stream.read_exact(&mut reply).unwrap();
    assert_eq!(&reply, b"pong");
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn hostname_connect_uses_available_address_family() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        socket.write_all(b"ok").unwrap();
    });
    let app = App::new();
    app.write(&format!(r#"
import pkg {{ connect, readExact, close }} from "tcp";
fn main() -> int {{
    var conn = connect(address: "localhost:{port}", timeoutMillis: 3000);
    if readExact(connection: conn, count: 2).toUtf8().unwrap() != "ok" {{ panic(message: "hostname connect"); }}
    close(connection: conn);
    return 0;
}};
"#));
    app.run(&["check", "src/main.spar"]);
    app.run(&["exec", "src/main.spar"]);
    server.join().unwrap();
}

#[test]
fn async_client_reads_writes_and_observes_eof() {
    let server = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = server.local_addr().unwrap();
    let peer = thread::spawn(move || {
        let (mut socket, _) = server.accept().unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut bytes = [0; 4];
        socket.read_exact(&mut bytes).unwrap();
        assert_eq!(&bytes, b"ping");
        socket.write_all(b"pong").unwrap();
    });
    let app = App::new();
    let payload = app.0.join("payload.bin");
    fs::write(&payload, b"ping").unwrap();
    app.write(&format!(
        r#"
import pkg {{ connectAsync, readAsync, writeAllAsync, closeAsync }} from "tcp";
import pkg {{ readBytes }} from "std/fs";
async fn main() -> int {{
    var connection = await connectAsync(address: "{address}", timeoutMillis: 5000);
    var sent: int = await writeAllAsync(connection: connection, data: readBytes(path: "{payload}"));
    if sent != 4 {{ panic(message: "async write count"); }}
    var response = await readAsync(connection: connection, maxBytes: 16);
    if response.toUtf8().unwrap() != "pong" {{ panic(message: "async read"); }}
    var end = await readAsync(connection: connection, maxBytes: 16);
    if end.length() != 0 {{ panic(message: "async EOF"); }}
    closeAsync(connection: connection);
    return 0;
}};
"#,
        payload = spar_path(&payload)
    ));
    app.run(&["check", "src/main.spar"]);
    app.run(&["exec", "src/main.spar"]);
    peer.join().unwrap();
}

#[test]
fn async_listener_accepts_and_echoes() {
    let probe = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = probe.local_addr().unwrap();
    drop(probe);
    let app = App::new();
    app.write(&format!(r#"
import pkg {{ listenAsync, asyncListenerLocalAddress, acceptAsync, readAsync, writeAllAsync, closeAsync, closeAsyncListener }} from "tcp";
async fn main() -> int {{
    var listener = listenAsync(address: "{address}");
    if asyncListenerLocalAddress(listener: listener) != "{address}" {{ panic(message: "async address"); }}
    var connection = await acceptAsync(listener: listener);
    var message = await readAsync(connection: connection, maxBytes: 16);
    if message.toUtf8().unwrap() != "hello" {{ panic(message: "async accept read"); }}
    var sent = await writeAllAsync(connection: connection, data: message);
    if sent != 5 {{ panic(message: "async accept write"); }}
    closeAsync(connection: connection);
    closeAsyncListener(listener: listener);
    return 0;
}};
"#));
    app.run(&["check", "src/main.spar"]);
    let child = Command::new(App::binary())
        .args(["exec", "src/main.spar"])
        .current_dir(&app.0)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut socket = loop {
        match TcpStream::connect(address) {
            Ok(socket) => break socket,
            Err(_) if Instant::now() < deadline => thread::sleep(Duration::from_millis(10)),
            Err(error) => panic!("async listener unavailable: {error}"),
        }
    };
    socket
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    socket.write_all(b"hello").unwrap();
    let mut echo = [0; 5];
    socket.read_exact(&mut echo).unwrap();
    assert_eq!(&echo, b"hello");
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn async_accept_cancels_pending_operation() {
    let app = App::new();
    app.write(
        r#"
import pkg { listenAsync, acceptAsync, cancelAsyncAccept } from "tcp";
import pkg { sleepMillis } from "std/time";
async fn main() -> int {
    var listener = listenAsync(address: "127.0.0.1:0");
    var pending = acceptAsync(listener: listener);
    sleepMillis(millis: 50);
    cancelAsyncAccept(listener: listener);
    var connection = await pending;
    return 0;
};
"#,
    );
    app.run(&["check", "src/main.spar"]);
    let started = Instant::now();
    let output = Command::new(App::binary())
        .args(["exec", "src/main.spar"])
        .current_dir(&app.0)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("TCP accept cancelled"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(started.elapsed() < Duration::from_secs(3));
}

#[test]
fn async_read_cancels_without_closing_socket() {
    let server = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = server.local_addr().unwrap();
    let peer = thread::spawn(move || {
        let (_socket, _) = server.accept().unwrap();
        thread::sleep(Duration::from_millis(300));
    });
    let app = App::new();
    app.write(&format!(
        r#"
import pkg {{ connectAsync, readAsync, cancelAsyncRead }} from "tcp";
import pkg {{ sleepMillis }} from "std/time";
async fn main() -> int {{
    var connection = await connectAsync(address: "{address}", timeoutMillis: 5000);
    var pending = readAsync(connection: connection, maxBytes: 16);
    sleepMillis(millis: 50);
    cancelAsyncRead(connection: connection);
    var data = await pending;
    return 0;
}};
"#
    ));
    app.run(&["check", "src/main.spar"]);
    let started = Instant::now();
    let output = Command::new(App::binary())
        .args(["exec", "src/main.spar"])
        .current_dir(&app.0)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("TCP read cancelled"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(started.elapsed() < Duration::from_secs(3));
    peer.join().unwrap();
}

#[test]
fn async_many_pending_reads_complete_without_worker_starvation() {
    let server = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = server.local_addr().unwrap();
    let peer = thread::spawn(move || {
        let mut sockets = Vec::new();
        for _ in 0..12 {
            let (socket, _) = server.accept().unwrap();
            sockets.push(socket);
        }
        for mut socket in sockets {
            socket.write_all(b"x").unwrap();
        }
    });
    let app = App::new();
    app.write(&format!(
        r#"
import pkg {{ connectAsync, readAsync, closeAsync }} from "tcp";
import pkg {{ all }} from "std/async";
async fn main() -> int {{
    var mut reads: [Promise<Bytes>] = [];
    var mut connections: [AsyncTcpConnectionHandle] = [];
    var mut index: int = 0;
    while index < 12 {{
        var connection = await connectAsync(address: "{address}", timeoutMillis: 5000);
        connections.append(value: connection);
        reads.append(value: readAsync(connection: connection, maxBytes: 1));
        index = index + 1;
    }}
    var results = await all<Bytes>(promises: reads);
    index = 0;
    while index < 12 {{
        if results[index].toUtf8().unwrap() != "x" {{ panic(message: "async fanout"); }}
        closeAsync(connection: connections[index]);
        index = index + 1;
    }}
    return 0;
}};
"#
    ));
    app.run(&["check", "src/main.spar"]);
    app.run(&["exec", "src/main.spar"]);
    peer.join().unwrap();
}

#[test]
fn async_write_all_handles_socket_backpressure() {
    let server = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = server.local_addr().unwrap();
    let peer = thread::spawn(move || {
        let (mut socket, _) = server.accept().unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        thread::sleep(Duration::from_millis(100));
        let mut received = 0usize;
        let mut buffer = [0u8; 8192];
        loop {
            let count = socket.read(&mut buffer).unwrap();
            if count == 0 {
                break;
            }
            assert!(buffer[..count].iter().all(|byte| *byte == 0x5a));
            received += count;
        }
        assert_eq!(received, 2 * 1024 * 1024);
    });
    let app = App::new();
    let payload = app.0.join("large.bin");
    fs::write(&payload, vec![0x5a; 2 * 1024 * 1024]).unwrap();
    app.write(&format!(
        r#"
import pkg {{ connectAsync, writeAllAsync, closeAsync }} from "tcp";
import pkg {{ readBytes }} from "std/fs";
async fn main() -> int {{
    var connection = await connectAsync(address: "{address}", timeoutMillis: 5000);
    var sent = await writeAllAsync(connection: connection, data: readBytes(path: "{payload}"));
    if sent != 2097152 {{ panic(message: "async backpressure count"); }}
    closeAsync(connection: connection);
    return 0;
}};
"#,
        payload = spar_path(&payload)
    ));
    app.run(&["check", "src/main.spar"]);
    app.run(&["exec", "src/main.spar"]);
    peer.join().unwrap();
}
