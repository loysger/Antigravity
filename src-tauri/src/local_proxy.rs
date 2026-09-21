//! Local HTTP proxy that sits on port 8047 and forwards IDE requests to the configured gateway.
//!
//! Intercepts local developer requests, manages session credentials, and streams
//! completion responses between local IDE instances and upstream services.

use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::watch;
use http_body_util::combinators::BoxBody;
use http_body_util::{BodyExt, StreamBody};
use hyper::body::Frame;
use futures_util::TryStreamExt;

/// Configuration for the local proxy
#[derive(Clone)]
pub struct ProxyConfig {
    pub listen_port: u16,
    pub target_url: String,   // e.g. "http://127.0.0.1:8045" or "http://vps:8045"
    pub bearer_token: String, // sk-* token
}

/// Start the local proxy server. Returns a shutdown sender.
/// When the sender is dropped or sends `true`, the server stops.
pub async fn start_proxy(config: ProxyConfig) -> Result<watch::Sender<bool>, String> {
    let addr = SocketAddr::from(([127, 0, 0, 1], config.listen_port));
    let mut listener = None;
    let mut last_err = None;
    for attempt in 1..=10 {
        let bind_result = async {
            let socket = if addr.is_ipv4() {
                tokio::net::TcpSocket::new_v4()?
            } else {
                tokio::net::TcpSocket::new_v6()?
            };
            let _ = socket.set_reuseaddr(true);
            #[cfg(all(unix, not(target_os = "solaris"), not(target_os = "illumos")))]
            {
                let _ = socket.set_reuseport(true);
            }
            socket.bind(addr)?;
            socket.listen(1024)
        }.await;
        match bind_result {
            Ok(l) => {
                listener = Some(l);
                break;
            }
            Err(e) => {
                eprintln!("[LocalProxy] Bind attempt {} to port {} failed: {}. Retrying in 200ms...", attempt, config.listen_port, e);
                last_err = Some(e);
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            }
        }
    }

    let listener = match listener {
        Some(l) => l,
        None => {
            return Err(format!(
                "Failed to bind to port {} after 10 attempts: {}",
                config.listen_port,
                last_err.map(|e| e.to_string()).unwrap_or_default()
            ));
        }
    };
    eprintln!("[LocalProxy] Bind successful");

    eprintln!("[LocalProxy] Listening on http://{}", addr);
    eprintln!("[LocalProxy] Forwarding to {}", config.target_url);

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let config = Arc::new(config);

    // Create a shared reqwest client for outgoing requests
    let http_client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(300)) // 5 min timeout for long requests
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;
    let http_client = Arc::new(http_client);

    tokio::spawn(async move {
        let mut shutdown = shutdown_rx;

        loop {
            tokio::select! {
                result = listener.accept() => {
                    match result {
                        Ok((stream, _remote)) => {
                            let config = config.clone();
                            let client = http_client.clone();
                            tokio::spawn(handle_connection(stream, config, client));
                        }
                        Err(e) => {
                            eprintln!("[LocalProxy] Accept error: {}", e);
                        }
                    }
                }
                _ = shutdown.changed() => {
                    if *shutdown.borrow() {
                        eprintln!("[LocalProxy] Shutting down");
                        break;
                    }
                }
            }
        }
    });

    Ok(shutdown_tx)
}

/// Handle a single TCP connection
async fn handle_connection(
    stream: tokio::net::TcpStream,
    config: Arc<ProxyConfig>,
    client: Arc<reqwest::Client>,
) {
    use hyper::server::conn::http1;
    use hyper::service::service_fn;
    use hyper_util::rt::TokioIo;

    let io = TokioIo::new(stream);
    let config = config.clone();
    let client = client.clone();

    let service = service_fn(move |req: hyper::Request<hyper::body::Incoming>| {
        let config = config.clone();
        let client = client.clone();
        async move { proxy_request(req, config, client).await }
    });

    if let Err(e) = http1::Builder::new()
        .serve_connection(io, service)
        .with_upgrades()
        .await
    {
        if !e.to_string().contains("connection closed") {
            eprintln!("[LocalProxy] Connection error: {}", e);
        }
    }
}

// Helper to box static body
fn full_box(data: bytes::Bytes) -> BoxBody<bytes::Bytes, std::io::Error> {
    http_body_util::Full::new(data)
        .map_err(|e| match e {})
        .boxed()
}

/// Proxy a single request to the Manager
async fn proxy_request(
    req: hyper::Request<hyper::body::Incoming>,
    config: Arc<ProxyConfig>,
    client: Arc<reqwest::Client>,
) -> Result<hyper::Response<BoxBody<bytes::Bytes, std::io::Error>>, Infallible> {
    let method = req.method().clone();
    let uri = req.uri().clone();
    let mut path = uri.path().to_string();
    let query = uri.query().map(|q| q.to_string());

    // Normalize path
    while path.contains("//") {
        path = path.replace("//", "/");
    }

    // Normalize API route prefixes for upstream gateway compatibility
    if path.starts_with("/v1internal/v1internal:") {
        path = path.replace("/v1internal/v1internal:", "/v1internal/");
    } else if path.starts_with("/v1internal:") {
        path = path.replace("v1internal:", "v1internal/");
    } else if path.starts_with("/v1internal/v1internal/") {
        path = path.replace("/v1internal/v1internal/", "/v1internal/");
    }

    // Add CORS headers to all responses
    let cors_origin = req.headers().get("origin").map(|v| v.to_str().unwrap_or("*")).unwrap_or("*").to_string();

    if method == hyper::Method::OPTIONS {
        eprintln!("[LocalProxy] Intercepting OPTIONS request for CORS: {}", path);
        let resp = hyper::Response::builder()
            .status(200)
            .header("Access-Control-Allow-Origin", &cors_origin)
            .header("Access-Control-Allow-Methods", "GET, POST, PUT, DELETE, OPTIONS")
            .header("Access-Control-Allow-Headers", "*")
            .header("Access-Control-Max-Age", "86400")
            .body(full_box(bytes::Bytes::from("")))
            .unwrap();
        return Ok(resp);
    }

    if path.contains("token") && !path.contains("tokeninfo") {
        eprintln!("[LocalProxy] Intercepting auth token refresh: {}", path);
        let mock_json = r#"{
            "access_token": "ya29.proxy_managed_token_do_not_use",
            "expires_in": 3599,
            "scope": "https://www.googleapis.com/auth/cloud-platform",
            "token_type": "Bearer",
            "id_token": "mock_id_token"
        }"#;

        let resp = hyper::Response::builder()
            .status(200)
            .header("Content-Type", "application/json")
            .header("Access-Control-Allow-Origin", &cors_origin)
            .body(full_box(bytes::Bytes::from(mock_json)))
            .unwrap();
        
        return Ok(resp);
    }

    // Intercept userinfo and tokeninfo to satisfy IDE authentication checks
    if path.contains("tokeninfo") {
        eprintln!("[LocalProxy] Intercepting auth check: {}", path);
        let mock_json = r#"{
            "issued_to": "antigravity-client",
            "audience": "antigravity-client",
            "scope": "https://www.googleapis.com/auth/cloud-platform",
            "expires_in": 3599,
            "access_type": "offline"
        }"#;

        let resp = hyper::Response::builder()
            .status(200)
            .header("Content-Type", "application/json")
            .header("Access-Control-Allow-Origin", &cors_origin)
            .body(full_box(bytes::Bytes::from(mock_json)))
            .unwrap();
        
        return Ok(resp);
    }

    if path.contains("userinfo") {
        eprintln!("[LocalProxy] Intercepting auth check: {}", path);
        let mock_json = r#"{
            "id": "1234567890",
            "email": "local@antigravity",
            "verified_email": true,
            "name": "Antigravity Local User",
            "given_name": "Antigravity",
            "family_name": "Local",
            "picture": "https://lh3.googleusercontent.com/a/default-user",
            "locale": "en"
        }"#;

        let resp = hyper::Response::builder()
            .status(200)
            .header("Content-Type", "application/json")
            .header("Access-Control-Allow-Origin", &cors_origin)
            .body(full_box(bytes::Bytes::from(mock_json)))
            .unwrap();
        
        return Ok(resp);
    }

    // Intercept operations polling (onboarding status check) to avoid 404
    if path.contains("/operations/") {
        eprintln!("[LocalProxy] Intercepting operations status check: {}", path);
        let mock_json = r#"{
            "name": "operation-completed",
            "done": true,
            "response": {
                "@type": "type.googleapis.com/google.protobuf.Empty"
            }
        }"#;

        let resp = hyper::Response::builder()
            .status(200)
            .header("Content-Type", "application/json")
            .header("Access-Control-Allow-Origin", &cors_origin)
            .body(full_box(bytes::Bytes::from(mock_json)))
            .unwrap();
        
        return Ok(resp);
    }

    if path.contains("fetchAdminControls") {
        eprintln!("[LocalProxy] Intercepting fetchAdminControls to avoid 400: {}", path);
        let mock_json = r#"{}"#;

        let resp = hyper::Response::builder()
            .status(200)
            .header("Content-Type", "application/json")
            .header("Access-Control-Allow-Origin", &cors_origin)
            .body(full_box(bytes::Bytes::from(mock_json)))
            .unwrap();
        
        return Ok(resp);
    }

    if path.contains("fetchUserInfo") {
        eprintln!("[LocalProxy] Handling fetchUserInfo profile configuration: {}", path);
        let mock_json = r#"{
            "userSettings": {
                "telemetryEnabled": true
            },
            "regionCode": "US"
        }"#;

        let resp = hyper::Response::builder()
            .status(200)
            .header("Content-Type", "application/json")
            .header("Access-Control-Allow-Origin", &cors_origin)
            .body(full_box(bytes::Bytes::from(mock_json)))
            .unwrap();
        
        return Ok(resp);
    }

    // Intercept telemetry/logging to prevent 401 crashes and protobuf parsing errors
    if path.contains("telemetry-noop") || path.contains("/log") && !path.contains("login") {
        eprintln!("[LocalProxy] Intercepting telemetry: {}", path);
        let resp = hyper::Response::builder()
            .status(200)
            .header("Content-Type", "application/x-protobuf")
            .header("Access-Control-Allow-Origin", &cors_origin)
            .body(full_box(bytes::Bytes::from("")))
            .unwrap();
        return Ok(resp);
    }

    // Build target URL
    let mut upstream_path = path.as_str();

    // Normalize path slashes
    while upstream_path.starts_with("//") {
        upstream_path = &upstream_path[1..];
    }
    // Remove trailing slashes
    let upstream_path = upstream_path.trim_end_matches('/');

    let target_url = if let Some(ref qs) = query {
        format!("{}{}{}?{}", config.target_url, if upstream_path.starts_with('/') { "" } else { "/" }, upstream_path, qs)
    } else {
        format!("{}{}{}", config.target_url, if upstream_path.starts_with('/') { "" } else { "/" }, upstream_path)
    };

    eprintln!("[LocalProxy] {} {} -> {}", method, uri, target_url);

    // Read the incoming body
    let body_bytes = match req.into_body().collect().await {
        Ok(collected) => collected.to_bytes(),
        Err(e) => {
            eprintln!("[LocalProxy] Failed to read body: {}", e);
            let resp = hyper::Response::builder()
                .status(502)
                .body(full_box(bytes::Bytes::from(format!("Failed to read request body: {}", e))))
                .unwrap();
            return Ok(resp);
        }
    };

    // Build outgoing request with Bearer token
    let mut outgoing = client
        .request(
            reqwest::Method::from_bytes(method.as_str().as_bytes()).unwrap_or(reqwest::Method::POST),
            &target_url,
        )
        .header("Authorization", format!("Bearer {}", config.bearer_token));

    // Copy relevant headers (skip host, connection, etc.)
    // We need content-type especially
    outgoing = outgoing.header("Content-Type", "application/json");
    outgoing = outgoing.header("User-Agent", "Antigravity-Client/1.0");

    // Send the request
    let response = match outgoing.body(body_bytes.to_vec()).send().await {
        Ok(r) => r,
        Err(e) => {
            eprintln!("[LocalProxy] Upstream error: {}", e);
            let resp = hyper::Response::builder()
                .status(502)
                .body(full_box(bytes::Bytes::from(format!("Upstream error: {}", e))))
                .unwrap();
            return Ok(resp);
        }
    };

    let status = response.status();
    eprintln!("[LocalProxy] Response: {}", status);

    // Build the hyper response
    let mut builder = hyper::Response::builder()
        .status(status.as_u16())
        .header("Access-Control-Allow-Origin", &cors_origin);

    // Copy response headers
    let is_streaming_endpoint = path.contains("loadCodeAssist") 
        || path.contains("chat") 
        || path.contains("streamGenerateContent");

    for (name, value) in response.headers() {
        let name_lower = name.as_str().to_lowercase();
        // Skip hop-by-hop headers
        if name_lower == "transfer-encoding" || name_lower == "connection" {
            continue;
        }
        // If we are streaming, skip content-length so hyper will use chunked encoding.
        // If we are buffering, we will calculate exact content-length later, so skip it here too.
        if name_lower == "content-length" {
            continue;
        }

        if let Ok(header_name) = hyper::header::HeaderName::from_bytes(name.as_str().as_bytes()) {
            if let Ok(header_value) = hyper::header::HeaderValue::from_bytes(value.as_bytes()) {
                builder = builder.header(header_name, header_value);
            }
        }
    }

    if is_streaming_endpoint {
        // Convert reqwest body to a Stream of hyper Frames
        let stream = response.bytes_stream()
            .map_ok(Frame::data)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e));
        let body = StreamBody::new(stream).boxed();

        let resp = builder
            .body(body)
            .unwrap_or_else(|_| {
                hyper::Response::new(full_box(bytes::Bytes::from("Internal error")))
            });

        Ok(resp)
    } else {
        // Buffer the full response body
        let resp_bytes = match response.bytes().await {
            Ok(b) => b,
            Err(e) => {
                eprintln!("[LocalProxy] Failed to read response: {}", e);
                bytes::Bytes::from(format!("Failed to read response: {}", e))
            }
        };

        builder = builder.header("Content-Length", resp_bytes.len().to_string());

        let resp = builder
            .body(full_box(resp_bytes))
            .unwrap_or_else(|_| {
                hyper::Response::new(full_box(bytes::Bytes::from("Internal error")))
            });

        Ok(resp)
    }
}
