//! A local `tiny_http` stand-in for the API, for the example's tests: each
//! route answers its scripted responses in order (repeating the last one)
//! and every request is recorded. Unmatched requests get a 501. No
//! cassette and no network: the example's responses are the documented
//! ones, like manza-ruby's WebMock stubs in spec/examples/.

use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use serde_json::Value;

pub struct Reply {
    pub status: u16,
    pub body: String,
    pub request_id: Option<&'static str>,
}

pub fn reply(status: u16, body: Value) -> Reply {
    Reply {
        status,
        body: body.to_string(),
        request_id: None,
    }
}

pub struct Route {
    pub method: &'static str,
    pub path: String,
    /// The `Authorization` header the route requires, when set.
    pub api_key: Option<&'static str>,
    pub replies: Vec<Reply>,
}

pub fn route(method: &'static str, path: impl Into<String>, replies: Vec<Reply>) -> Route {
    Route {
        method,
        path: path.into(),
        api_key: None,
        replies,
    }
}

#[derive(Debug, Clone)]
pub struct Recorded {
    pub method: String,
    pub path: String,
    pub authorization: Option<String>,
    pub body: Value,
}

pub struct StubApi {
    pub url: String,
    requests: Arc<Mutex<Vec<Recorded>>>,
    server: Arc<tiny_http::Server>,
    handle: Option<JoinHandle<()>>,
}

impl StubApi {
    pub fn start(routes: Vec<Route>) -> StubApi {
        let server = Arc::new(tiny_http::Server::http("127.0.0.1:0").expect("bind stub API"));
        let port = server.server_addr().to_ip().expect("stub API addr").port();
        let requests = Arc::new(Mutex::new(Vec::new()));

        let worker = Arc::clone(&server);
        let recorded = Arc::clone(&requests);
        let handle = std::thread::spawn(move || {
            let mut served = vec![0usize; routes.len()];
            for mut request in worker.incoming_requests() {
                let mut body = String::new();
                let _ = request.as_reader().read_to_string(&mut body);
                let authorization = request
                    .headers()
                    .iter()
                    .find(|h| h.field.equiv("Authorization"))
                    .map(|h| h.value.as_str().to_owned());
                let method = request.method().as_str().to_owned();
                let path = request.url().to_owned();
                recorded.lock().unwrap().push(Recorded {
                    method: method.clone(),
                    path: path.clone(),
                    authorization: authorization.clone(),
                    body: serde_json::from_str(&body).unwrap_or(Value::Null),
                });

                let matched = routes.iter().position(|r| {
                    r.method == method
                        && r.path == path
                        && r.api_key.is_none_or(|key| {
                            authorization.as_deref() == Some(&format!("Bearer {key}"))
                        })
                });
                let response = match matched {
                    Some(index) => {
                        let replies = &routes[index].replies;
                        let reply = &replies[served[index].min(replies.len() - 1)];
                        served[index] += 1;
                        let mut response = tiny_http::Response::from_string(reply.body.clone())
                            .with_status_code(reply.status)
                            .with_header(header("Content-Type", "application/json"));
                        if let Some(id) = reply.request_id {
                            response = response.with_header(header("X-Request-Id", id));
                        }
                        response
                    }
                    None => tiny_http::Response::from_string(format!(
                        r#"{{"error":{{"message":"no stub for {method} {path}"}}}}"#
                    ))
                    .with_status_code(501),
                };
                let _ = request.respond(response);
            }
        });

        StubApi {
            url: format!("http://127.0.0.1:{port}"),
            requests,
            server,
            handle: Some(handle),
        }
    }

    pub fn requests(&self) -> Vec<Recorded> {
        self.requests.lock().unwrap().clone()
    }

    /// Requests whose path ends with `suffix`.
    pub fn requests_to(&self, suffix: &str) -> Vec<Recorded> {
        self.requests()
            .into_iter()
            .filter(|r| r.path.ends_with(suffix))
            .collect()
    }
}

impl Drop for StubApi {
    fn drop(&mut self) {
        self.server.unblock();
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn header(name: &str, value: &str) -> tiny_http::Header {
    tiny_http::Header::from_bytes(name.as_bytes(), value.as_bytes()).expect("header")
}

/// A fresh, empty directory under the system temp dir, removed on drop.
pub struct TempDir(pub std::path::PathBuf);

/// See [`TempDir`].
pub fn temp_dir() -> TempDir {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let path = std::env::temp_dir().join(format!(
        "manza-example-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).expect("create temp dir");
    TempDir(path)
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
