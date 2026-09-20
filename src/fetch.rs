//! HTTP for feeds: `ureq` with rustls, conditional GET, a per-feed timeout,
//! and a fixed number of fetches in flight on plain threads.

use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use ureq::Agent;

const USER_AGENT: &str = concat!("herdr-rss/", env!("CARGO_PKG_VERSION"));
const MAX_BODY: u64 = 10 * 1024 * 1024;
pub const PARALLEL: usize = 8;

/// The validators from the last successful fetch.
#[derive(Debug, Clone, Default)]
pub struct Cache {
    pub etag: Option<String>,
    pub last_modified: Option<String>,
}

#[derive(Debug)]
pub enum Outcome {
    Fetched {
        body: Vec<u8>,
        etag: Option<String>,
        last_modified: Option<String>,
    },
    NotModified,
    Failed(String),
}

pub fn agent(timeout: Duration) -> Agent {
    Agent::config_builder()
        .timeout_global(Some(timeout))
        .http_status_as_error(false)
        .max_redirects(10)
        .user_agent(USER_AGENT)
        .build()
        .new_agent()
}

pub fn fetch_one(agent: &Agent, url: &str, cache: &Cache) -> Outcome {
    let mut req = agent.get(url);
    if let Some(etag) = &cache.etag {
        req = req.header("If-None-Match", etag);
    }
    if let Some(lm) = &cache.last_modified {
        req = req.header("If-Modified-Since", lm);
    }
    let mut resp = match req.call() {
        Ok(r) => r,
        Err(e) => return Outcome::Failed(e.to_string()),
    };
    let status = resp.status().as_u16();
    if status == 304 {
        return Outcome::NotModified;
    }
    if !(200..300).contains(&status) {
        return Outcome::Failed(format!("HTTP {status}"));
    }
    let header = |name: &str| {
        resp.headers()
            .get(name)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string)
    };
    let etag = header("etag");
    let last_modified = header("last-modified");
    match resp.body_mut().with_config().limit(MAX_BODY).read_to_vec() {
        Ok(body) => Outcome::Fetched {
            body,
            etag,
            last_modified,
        },
        Err(e) => Outcome::Failed(format!("read body: {e}")),
    }
}

/// Fetches every URL, `PARALLEL` at a time. Results come back in input order.
pub fn fetch_all(jobs: Vec<(String, Cache)>, timeout: Duration) -> Vec<(String, Outcome)> {
    let n = jobs.len();
    let queue = Arc::new(Mutex::new(jobs.into_iter().enumerate().collect::<Vec<_>>()));
    let (tx, rx) = mpsc::channel();
    let workers = PARALLEL.min(n);
    let handles: Vec<_> = (0..workers)
        .map(|_| {
            let queue = Arc::clone(&queue);
            let tx = tx.clone();
            std::thread::spawn(move || {
                let agent = agent(timeout);
                loop {
                    let job = queue.lock().ok().and_then(|mut q| q.pop());
                    let Some((i, (url, cache))) = job else { break };
                    let out = fetch_one(&agent, &url, &cache);
                    if tx.send((i, url, out)).is_err() {
                        break;
                    }
                }
            })
        })
        .collect();
    drop(tx);
    let mut results: Vec<Option<(String, Outcome)>> = (0..n).map(|_| None).collect();
    for (i, url, out) in rx {
        results[i] = Some((url, out));
    }
    for h in handles {
        let _ = h.join();
    }
    results
        .into_iter()
        .map(|r| r.expect("every job reports once"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;

    /// A one-shot HTTP server that answers from a closure and records the request.
    fn serve(
        respond: impl Fn(&str) -> String + Send + 'static,
    ) -> (String, std::thread::JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = std::thread::spawn(move || {
            let mut seen = Vec::new();
            let (mut sock, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(sock.try_clone().unwrap());
            let mut head = String::new();
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if line == "\r\n" || line.is_empty() {
                    break;
                }
                head.push_str(&line);
            }
            seen.push(head.to_ascii_lowercase());
            sock.write_all(respond(&head).as_bytes()).unwrap();
            let _ = sock.flush();
            let mut sink = Vec::new();
            let _ = sock.read_to_end(&mut sink);
            seen
        });
        (format!("http://{addr}/feed"), handle)
    }

    #[test]
    fn fetched_keeps_validators() {
        let (url, h) = serve(|_| {
            "HTTP/1.1 200 OK\r\nETag: \"abc\"\r\nLast-Modified: Fri, 19 Sep 2026 00:00:00 GMT\r\nContent-Length: 5\r\nConnection: close\r\n\r\nhello".into()
        });
        let out = fetch_one(&agent(Duration::from_secs(5)), &url, &Cache::default());
        let Outcome::Fetched {
            body,
            etag,
            last_modified,
        } = out
        else {
            panic!("{out:?}")
        };
        assert_eq!(body, b"hello");
        assert_eq!(etag.as_deref(), Some("\"abc\""));
        assert!(last_modified.unwrap().starts_with("Fri"));
        let seen = h.join().unwrap();
        assert!(
            seen[0].contains(&format!("user-agent: {USER_AGENT}")),
            "{}",
            seen[0]
        );
    }

    #[test]
    fn sends_validators_and_reads_304() {
        let (url, h) = serve(|_| "HTTP/1.1 304 Not Modified\r\nConnection: close\r\n\r\n".into());
        let cache = Cache {
            etag: Some("\"abc\"".into()),
            last_modified: Some("Fri, 19 Sep 2026 00:00:00 GMT".into()),
        };
        let out = fetch_one(&agent(Duration::from_secs(5)), &url, &cache);
        assert!(matches!(out, Outcome::NotModified), "{out:?}");
        let seen = h.join().unwrap();
        assert!(seen[0].contains("if-none-match: \"abc\""), "{}", seen[0]);
        assert!(seen[0].contains("if-modified-since: fri"), "{}", seen[0]);
    }

    #[test]
    fn http_error_is_failed_not_panic() {
        let (url, h) =
            serve(|_| "HTTP/1.1 503 Busy\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".into());
        let out = fetch_one(&agent(Duration::from_secs(5)), &url, &Cache::default());
        assert!(
            matches!(out, Outcome::Failed(ref e) if e == "HTTP 503"),
            "{out:?}"
        );
        h.join().unwrap();
    }

    #[test]
    fn refused_connection_is_failed() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/feed", listener.local_addr().unwrap());
        drop(listener);
        let out = fetch_one(&agent(Duration::from_secs(2)), &url, &Cache::default());
        assert!(matches!(out, Outcome::Failed(_)), "{out:?}");
    }

    #[test]
    fn fetch_all_keeps_input_order() {
        let (a, ha) =
            serve(|_| "HTTP/1.1 200 OK\r\nContent-Length: 1\r\nConnection: close\r\n\r\nA".into());
        let (b, hb) =
            serve(|_| "HTTP/1.1 200 OK\r\nContent-Length: 1\r\nConnection: close\r\n\r\nB".into());
        let jobs = vec![(a.clone(), Cache::default()), (b.clone(), Cache::default())];
        let got = fetch_all(jobs, Duration::from_secs(5));
        assert_eq!(got[0].0, a);
        assert_eq!(got[1].0, b);
        assert!(matches!(&got[0].1, Outcome::Fetched { body, .. } if body == b"A"));
        assert!(matches!(&got[1].1, Outcome::Fetched { body, .. } if body == b"B"));
        ha.join().unwrap();
        hb.join().unwrap();
        assert!(fetch_all(Vec::new(), Duration::from_secs(1)).is_empty());
    }
}
