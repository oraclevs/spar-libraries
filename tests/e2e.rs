use std::fs::{self, File};
use std::io::{BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use rustls::pki_types::ServerName;
use rustls::{ClientConfig, ClientConnection, RootCertStore, StreamOwned};

static NEXT: AtomicUsize = AtomicUsize::new(0);
struct App(PathBuf);
impl App {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!(
            "spar-http-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(dir.join("src")).unwrap();
        fs::write(dir.join("spar.package.spar"), format!(
            "struct Package {{ name: str = \"http-probe\"; version: str = \"0.1.0\"; kind: str = \"application\"; entry: str = \"src/main.spar\"; }};\nstruct Dependencies {{ server: str = \"path:{}\"; }};\n",
            Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().display()
        )).unwrap();
        let app = Self(dir);
        app.run(&["install"]);
        app
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
    fn write(&self, source: &str) {
        fs::write(self.0.join("src/main.spar"), source).unwrap();
    }
    fn spawn(&self) -> Child {
        Command::new(Self::binary())
            .args(["exec", "src/main.spar"])
            .current_dir(&self.0)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap()
    }
}
impl Drop for App {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}
fn connect_when_ready(port: u16) -> TcpStream {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match TcpStream::connect(("127.0.0.1", port)) {
            Ok(stream) => {
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                return stream;
            }
            Err(_) if Instant::now() < deadline => thread::sleep(Duration::from_millis(10)),
            Err(error) => panic!("server did not listen: {error}"),
        }
    }
}

#[test]
fn plain_http_handles_split_utf8_request() {
    let app = App::new();
    let port = port();
    app.write(&format!(r#"
import pkg {{ openListener, serveOne, ServerRequest, ServerResponse }} from "server";
fn handle(request: ServerRequest) -> ServerResponse {{
    return ServerResponse(body: "received ${{request.method}} ${{request.target}}: ${{request.body}}");
}};
fn main() -> int {{
    var listener = openListener(address: "127.0.0.1:{port}");
    serveOne(listener: listener, handler: handle);
    return 0;
}};
"#));
    app.run(&["check", "src/main.spar"]);
    let child = app.spawn();
    let mut stream = connect_when_ready(port);
    stream
        .write_all(b"POST /hello HTTP/1.1\r\nHost: localhost\r\nContent-Length: 2\r\n\r\n\xc3")
        .unwrap();
    thread::sleep(Duration::from_millis(50));
    stream.write_all(b"\xa9").unwrap();
    let mut response = Vec::new();
    stream.read_to_end(&mut response).unwrap();
    let boundary = response.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
    let (head, body) = response.split_at(boundary + 4);
    let expected = "received POST /hello: é".as_bytes();
    assert!(head.starts_with(b"HTTP/1.1 200 OK\r\n"));
    assert!(head
        .windows(format!("Content-Length: {}\r\n", expected.len()).len())
        .any(|w| w == format!("Content-Length: {}\r\n", expected.len()).as_bytes()));
    assert_eq!(body, expected);
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn https_verifies_certificate() {
    let app = App::new();
    let cert = app.0.join("server.crt");
    let key = app.0.join("server.key");
    let output = Command::new("openssl")
        .args(["req", "-x509", "-newkey", "rsa:2048", "-nodes", "-keyout"])
        .arg(&key)
        .arg("-out")
        .arg(&cert)
        .args([
            "-subj",
            "/CN=localhost",
            "-addext",
            "subjectAltName=DNS:localhost",
            "-addext",
            "basicConstraints=critical,CA:FALSE",
            "-days",
            "1",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let port = port();
    app.write(&format!(
        r#"
import pkg {{ openTlsListener, serveOne, ServerRequest, ServerResponse }} from "server";
fn handle(request: ServerRequest) -> ServerResponse {{
    return ServerResponse(body: "secure ${{request.target}}");
}};
fn main() -> int {{
    var listener = openTlsListener(address: "127.0.0.1:{port}", certPath: "{}", keyPath: "{}");
    serveOne(listener: listener, handler: handle);
    return 0;
}};
"#,
        cert.display(),
        key.display()
    ));
    app.run(&["check", "src/main.spar"]);
    let child = app.spawn();
    // A probe may fail the handshake; the TCP TLS listener discards that socket and continues.
    drop(connect_when_ready(port));
    let mut roots = RootCertStore::empty();
    for certificate in rustls_pemfile::certs(&mut BufReader::new(File::open(&cert).unwrap())) {
        roots.add(certificate.unwrap()).unwrap();
    }
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let config = ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_root_certificates(roots)
        .with_no_client_auth();
    let connection = ClientConnection::new(
        Arc::new(config),
        ServerName::try_from("localhost").unwrap().to_owned(),
    )
    .unwrap();
    let mut stream = StreamOwned::new(connection, connect_when_ready(port));
    stream
        .write_all(b"GET /tls HTTP/1.1\r\nHost: localhost\r\n\r\n")
        .unwrap();
    let mut response = Vec::new();
    stream.read_to_end(&mut response).unwrap();
    assert!(response.starts_with(b"HTTP/1.1 200 OK\r\n"));
    assert!(response.ends_with(b"secure /tls"));
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
