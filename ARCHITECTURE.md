# Antigravity Client Architecture

This document provides a technical overview of the **Antigravity Client** architecture, internal components, and network routing mechanisms.

---

## 1. High-Level Overview

Antigravity Client is a cross-platform desktop application built with **Tauri v2** (Rust) and **React 18** (TypeScript, Tailwind CSS). It serves as an intelligent local proxy bridge and lifecycle orchestrator for developer environments, including **Antigravity IDE**, **Antigravity 2.0**, and **Antigravity CLI** (`agy`).

```
+--------------------------------------------------------------------+
|                         Antigravity Client                         |
|                                                                    |
|  +------------------------+      +-------------------------------+ |
|  |    React Frontend      | <--> |     Tauri IPC Command Layer   | |
|  |  (UI, Logs, Dashboard) |      | (Process Manager, Auth State) | |
|  +------------------------+      +---------------+---------------+ |
|                                                  |                 |
|  +-----------------------------------------------+---------------+ |
|  |                      Core Engine (Rust)                       | |
|  |                                                               | |
|  |  +---------------------+           +------------------------+ | |
|  |  |  Embedded Proxy     |           | Encrypted Duplex       | | |
|  |  |  (Tokio / Hyper)    | <-------> | Tunnel Worker Pool     | | |
|  |  |  Port: 8047         |           | (Tokio Tungstenite WS) | | |
|  |  +----------+----------+           +-----------+------------+ | |
|  +-------------|----------------------------------|--------------+ |
+----------------|----------------------------------|----------------+
                 |                                  |
     Interception|                                  | WebSocket Bridge
                 v                                  v
   +---------------------------+        +-----------------------+
   |   Local Developer Tools   |        |  Antigravity Gateway  |
   | (Antigravity IDE / CLI)   |        |  & Cloud AI Services  |
   +---------------------------+        +-----------------------+
```

---

## 2. Core Components

### 2.1. Local Hyper Proxy Server (`local_proxy.rs`)
* **Framework**: Built on `tokio` and `hyper 1.0` / `hyper-util`.
* **Binding**: Listens exclusively on the local loopback interface (`127.0.0.1:8047`) to prevent external exposure.
* **Authentication Middleware**: Injects active session credentials (`Authorization: Bearer <token>`) into outgoing HTTP requests.
* **Streaming Protocol Handling**: Supports Server-Sent Events (SSE) and HTTP chunked transfer encoding for streaming AI completions (`streamGenerateContent`, `loadCodeAssist`).
* **CORS & Preflight**: Handles CORS headers (`OPTIONS`) for web-based IDE extensions and local tools.

### 2.2. Secure WebSocket Tunnel (`tunnel.rs`)
* **Transport**: Full-duplex WebSocket connection (`tokio-tungstenite`) to the configured Antigravity Gateway.
* **Keep-Alive**: Asynchronous heartbeat (`Ping`/`Pong`) loop with graceful reconnection backoff.
* **TCP Stream Bridging**: Efficient bidirectional byte-streaming between local sockets and remote endpoints with zero-copy buffers.

### 2.3. Process Orchestrator & IDE Bridge (`lib.rs`)
* **Lifecycle Management**: Automatically detects, launches, and monitors Antigravity IDE and CLI instances.
* **Proxy Configuration**:
  - Injects standard network environment variables: `HTTP_PROXY`, `HTTPS_PROXY`, `ALL_PROXY`, `NO_PROXY`.
  - Configures Chromium / Electron runtime flags: `--proxy-server=http://127.0.0.1:8047` and `--proxy-bypass-list=<-loopback>`.
  - Spawns terminal sessions with pre-configured proxy environments for CLI operations.
* **Graceful Restoration**: Upon application exit or proxy shutdown, restores original environment variables and proxy profiles.

### 2.4. Configuration & Session Store (`db.rs`)
* **Storage**: Embedded SQLite database (`rusqlite`) located in user application data (`~/.antigravity-client/client.db`).
* **Schema**:
  - `app_config`: Application preferences, active UI theme, and connection parameters.
  - `sessions`: Securely stores active session metadata, token lifetimes, and gateway endpoints.
* **Keyring Integration**: Interfaces with native OS credential stores (Windows Credential Manager, macOS Keychain, Linux Secret Service via DBus) for secure token caching.
* **IDE Settings Synchronizer**: Updates target IDE `settings.json` with local proxy configurations.

---

## 3. Request Flow

1. **Initialization**: User authenticates with an access key or token via the desktop dashboard.
2. **Proxy Start**: The local Hyper proxy starts on `127.0.0.1:8047` and establishes tunnel workers to the gateway.
3. **Tool Launch**: The client spawns the IDE or CLI with proxy routing flags and environment variables.
4. **Traffic Interception**: IDE requests route through `http://127.0.0.1:8047`.
5. **Gateway Forwarding**: The local proxy attaches session credentials, handles headers, and pipes the request to the upstream service.
6. **Response Streaming**: Responses are streamed back in real-time to the IDE interface.

---

## 4. Build & Distribution

* **Frontend**: `npm run build` (Vite + React + Tailwind)
* **Desktop Bundle**: `cargo tauri build` (Multi-platform: Windows `.msi`, macOS `.dmg`, Linux `.AppImage` / `.deb`)
* **CI/CD**: Automated GitHub Actions matrix build producing cryptographically signed releases.
