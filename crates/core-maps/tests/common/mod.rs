#![allow(dead_code, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

use sha2::{Digest, Sha256};

/// Directory holding the region fixtures (`catalog.json` and `mini/`).
pub fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/regions")
}

/// A fresh, empty directory for one test.
pub fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("core-maps-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

pub fn sha256_hex(data: &[u8]) -> String {
    Sha256::digest(data)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Copies the fixture catalog into a fresh directory so that tests can alter it.
pub fn catalog_copy(name: &str) -> PathBuf {
    let dir = temp_dir(name);
    std::fs::copy(fixtures().join("catalog.json"), dir.join("catalog.json")).unwrap();
    std::fs::create_dir_all(dir.join("mini")).unwrap();
    for entry in std::fs::read_dir(fixtures().join("mini")).unwrap() {
        let entry = entry.unwrap();
        std::fs::copy(entry.path(), dir.join("mini").join(entry.file_name())).unwrap();
    }
    dir
}

/// One request seen by [`FileServer`]: path and the start of the requested range.
pub type Request = (String, Option<u64>);

/// Minimal HTTP server for tests: serves files below a directory and honours
/// `Range: bytes=<start>-` requests.
pub struct FileServer {
    pub url: String,
    pub requests: std::sync::Arc<std::sync::Mutex<Vec<Request>>>,
}

impl FileServer {
    pub fn start(root: PathBuf) -> Self {
        use std::io::{BufRead, BufReader, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let requests = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let log = std::sync::Arc::clone(&requests);
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                if reader.read_line(&mut line).is_err() {
                    continue;
                }
                let path = line.split_whitespace().nth(1).unwrap_or("/").to_string();
                let mut start = None;
                loop {
                    let mut header = String::new();
                    if reader.read_line(&mut header).unwrap_or(0) == 0 || header.trim().is_empty() {
                        break;
                    }
                    if let Some(value) = header.to_ascii_lowercase().strip_prefix("range: bytes=") {
                        start = value.trim().trim_end_matches('-').parse::<u64>().ok();
                    }
                }
                log.lock().unwrap().push((path.clone(), start));
                let response = match std::fs::read(root.join(path.trim_start_matches('/'))) {
                    Ok(body) => {
                        let from = (start.unwrap_or(0) as usize).min(body.len());
                        let status = if start.is_some() {
                            "206 Partial Content"
                        } else {
                            "200 OK"
                        };
                        let mut head = format!(
                            "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n",
                            body.len() - from
                        );
                        if start.is_some() {
                            head += &format!(
                                "Content-Range: bytes {from}-{}/{}\r\n",
                                body.len() - 1,
                                body.len()
                            );
                        }
                        let mut bytes = (head + "\r\n").into_bytes();
                        bytes.extend_from_slice(&body[from..]);
                        bytes
                    }
                    Err(_) => {
                        b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                            .to_vec()
                    }
                };
                // The client may hang up early when a download is cancelled.
                let _ = stream.write_all(&response);
            }
        });
        FileServer { url, requests }
    }
}
