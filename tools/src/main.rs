//! Development HTTP bridge for stdio based Spar apps.
use std::env;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const MAX_REQUEST: usize = 1_048_576;
const FAILURE: &[u8] =
    b"HTTP/1.1 500 Internal Server Error\r\ncontent-length: 0\r\nconnection: close\r\n\r\n";

fn boundary(data: &[u8]) -> Option<usize> {
    data.windows(4).position(|part| part == b"\r\n\r\n")
}

fn read_request(stream: &mut TcpStream) -> std::io::Result<Vec<u8>> {
    let mut data = Vec::new();
    let mut chunk = [0; 8192];
    while boundary(&data).is_none() {
        let n = stream.read(&mut chunk)?;
        if n == 0 {
            return Ok(data);
        }
        data.extend_from_slice(&chunk[..n]);
        if data.len() > MAX_REQUEST {
            return Err(std::io::Error::other("request too large"));
        }
    }
    let head_end = boundary(&data).unwrap() + 4;
    let head = String::from_utf8_lossy(&data[..head_end]);
    let mut length = 0usize;
    for line in head.split("\r\n").skip(1) {
        if let Some((name, value)) = line.split_once(':') {
            if name.trim().eq_ignore_ascii_case("content-length") {
                length = value.trim().parse().unwrap_or(0);
            }
        }
    }
    if head_end + length > MAX_REQUEST {
        return Err(std::io::Error::other("request too large"));
    }
    while data.len() < head_end + length {
        let n = stream.read(&mut chunk)?;
        if n == 0 {
            break;
        }
        data.extend_from_slice(&chunk[..n]);
        if data.len() > MAX_REQUEST {
            return Err(std::io::Error::other("request too large"));
        }
    }
    Ok(data)
}

fn add_length(raw: &[u8]) -> Vec<u8> {
    let Some(at) = boundary(raw) else {
        return raw.to_vec();
    };
    let mut response = Vec::with_capacity(raw.len() + 40);
    response.extend_from_slice(&raw[..at]);
    response.extend_from_slice(
        format!("\r\ncontent-length: {}\r\n\r\n", raw.len() - at - 4).as_bytes(),
    );
    response.extend_from_slice(&raw[at + 4..]);
    response
}

fn handle(mut stream: TcpStream, script: &str, args: &[String]) -> std::io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(30)))?;
    stream.set_write_timeout(Some(Duration::from_secs(30)))?;
    let request = read_request(&mut stream)?;
    let mut command = Command::new("spar");
    command.arg("exec").arg(script);
    if !args.is_empty() {
        command.arg("--").args(args);
    }
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;
    child.stdin.take().unwrap().write_all(&request)?;
    let mut stdout = child.stdout.take().unwrap();
    let output = thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout.read_to_end(&mut bytes).map(|_| bytes)
    });
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if child.try_wait()?.is_some() {
            break;
        }
        if Instant::now() >= deadline {
            child.kill()?;
            child.wait()?;
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "Spar request timed out",
            ));
        }
        thread::sleep(Duration::from_millis(10));
    }
    let output = output
        .join()
        .map_err(|_| std::io::Error::other("Spar output reader failed"))??;
    let response = if output.is_empty() {
        FAILURE.to_vec()
    } else {
        add_length(&output)
    };
    stream.write_all(&response)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut host = "127.0.0.1".to_string();
    let mut port = 8080u16;
    let mut argv = env::args().skip(1);
    let mut script = None;
    let mut script_args = Vec::new();
    while let Some(arg) = argv.next() {
        match arg.as_str() {
            "--host" => host = argv.next().ok_or("missing --host value")?,
            "--port" => port = argv.next().ok_or("missing --port value")?.parse()?,
            "--" => script_args.extend(&mut argv),
            _ if script.is_none() => script = Some(arg),
            _ => script_args.push(arg),
        }
    }
    let script =
        script.ok_or("usage: spar-web-bridge [--host HOST] [--port PORT] script [-- args]")?;
    let listener = TcpListener::bind((host.as_str(), port))?;
    println!("bridge listening on http://{}:{}", host, port);
    for connection in listener.incoming() {
        let stream = connection?;
        let script = script.clone();
        let args = script_args.clone();
        thread::spawn(move || {
            if let Err(error) = handle(stream, &script, &args) {
                eprintln!("bridge request failed: {error}");
            }
        });
    }
    Ok(())
}
