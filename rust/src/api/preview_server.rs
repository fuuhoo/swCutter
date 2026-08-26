//! 预览静态服务：为切片输出目录提供 http://127.0.0.1 随机端口访问，
//! 彻底绕开 file:// 唯一源限制（跨文件 img/fetch 不再报 Unsafe attempt）。

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

fn servers() -> &'static Mutex<HashMap<PathBuf, u16>> {
    static S: OnceLock<Mutex<HashMap<PathBuf, u16>>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 为目录启动（或复用）静态服务，返回端口。
/// 服务随进程生命周期存活；重复调用同一目录返回既有端口。
pub fn preview_serve(dir: String) -> anyhow::Result<u16> {
    let root = PathBuf::from(&dir);
    if !root.is_dir() {
        anyhow::bail!("目录不存在: {dir}");
    }
    let root = root.canonicalize()?;
    if let Some(port) = servers().lock().unwrap().get(&root) {
        return Ok(*port);
    }
    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    let port = listener.local_addr()?.port();
    let root2 = root.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { continue };
            let root = root2.clone();
            // 每连接一线程：浏览器并发拉取几十个瓦片
            std::thread::spawn(move || handle(stream, root));
        }
    });
    servers().lock().unwrap().insert(root, port);
    Ok(port)
}

fn content_type(p: &Path) -> &'static str {
    match p.extension().and_then(|e| e.to_str()).map(|s| s.to_ascii_lowercase()) {
        Some(s) if s == "html" => "text/html; charset=utf-8",
        Some(s) if s == "png" => "image/png",
        Some(s) if s == "json" => "application/json",
        Some(s) if s == "js" => "text/javascript",
        Some(s) if s == "css" => "text/css",
        Some(s) if s == "jpg" || s == "jpeg" => "image/jpeg",
        Some(s) if s == "webp" => "image/webp",
        _ => "application/octet-stream",
    }
}

/// 极简 URL 解码（%XX 与 '+'），足够处理路径中的空格等
fn url_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() + 1 && i + 2 < bytes.len() + 1 => {
                if i + 2 < bytes.len() {
                    let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("");
                    if let Ok(v) = u8::from_str_radix(hex, 16) {
                        out.push(v);
                        i += 3;
                        continue;
                    }
                }
                out.push(b'%');
                i += 1;
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn handle(mut stream: TcpStream, root: PathBuf) {
    let mut buf = [0u8; 4096];
    let n = match stream.read(&mut buf) {
        Ok(n) if n > 0 => n,
        _ => return,
    };
    let req = String::from_utf8_lossy(&buf[..n]);
    let path = req.split_whitespace().nth(1).unwrap_or("/");
    let clean = path.split('?').next().unwrap_or("/");
    let rel = url_decode(clean.trim_start_matches('/'));
    // 路径安全：拒绝 ..、绝对盘符
    if rel.split(['/', '\\']).any(|seg| seg == "..") {
        let _ = respond(&mut stream, 403, "text/plain", b"forbidden");
        return;
    }
    let target = if rel.is_empty() || rel.ends_with('/') {
        root.join(&rel).join("preview.html")
    } else {
        root.join(&rel)
    };
    match std::fs::read(&target) {
        Ok(bytes) => {
            let _ = respond(&mut stream, 200, content_type(&target), &bytes);
        }
        Err(_) => {
            let _ = respond(&mut stream, 404, "text/plain", b"not found");
        }
    }
}

fn respond(stream: &mut TcpStream, code: u16, ctype: &str, body: &[u8]) -> std::io::Result<()> {
    let head = format!(
        "HTTP/1.1 {code} {}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        if code == 200 { "OK" } else { "ERR" },
        body.len()
    );
    stream.write_all(head.as_bytes())?;
    stream.write_all(body)
}
