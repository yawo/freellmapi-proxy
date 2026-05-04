# Rust Migration Plan: FreeLLMAPI

## Goal
Convert the existing Node.js `server/` implementation of FreeLLMAPI into a high-performance, low-memory Rust application suitable for environments like Termux on Android. The `client/` (React SPA) will remain untouched and will be served by the new Rust backend.

## Tech Stack (Decided)
*   **Web Framework:** Axum (with `tokio` for async runtime).
*   **Database:** SQLite via SeaORM.
*   **HTTP Client:** `reqwest` for making outgoing requests to LLM providers.
*   **Serialization:** `serde` and `serde_json`.
*   **Encryption:** `aes-gcm` crate for AES-256-GCM.
*   **Event Streams (SSE):** `axum-extra` (for Server-Sent Events).

## Architecture Mapping

| Node.js Component | Rust Equivalent |
| :--- | :--- |
| `express` | `axum` |
| `better-sqlite3` + `drizzle-orm` | `sea-orm` + `sqlx-sqlite` |
| `server/src/providers/*.ts` | `src/providers/*.rs` (Implementing a shared Trait) |
| `server/src/services/router.ts` | `src/services/router.rs` |
| `server/src/services/ratelimit.ts` | `src/services/ratelimit.rs` (Using `RwLock` or `DashMap`) |
| `server/src/lib/crypto.ts` | `src/crypto.rs` (using `aes-gcm`) |

## Phase 1: Project Setup and Foundation

1.  **Initialize Rust Project:**
    *   Create a new Cargo workspace or just a new binary crate alongside the `server` directory (e.g., `server-rs`).
    *   Add core dependencies to `Cargo.toml`: `axum`, `tokio`, `serde`, `reqwest`, `sea-orm`, `dotenvy`.

2.  **Define Core Types:**
    *   Port types from `@freellmapi/shared/types.ts` into Rust structs using `serde`.
    *   Crucially, define the models for OpenAI requests/responses (`ChatCompletionRequest`, `ChatCompletionResponse`, `ChatCompletionChunk`, etc.).

3.  **Database Layer (SeaORM):**
    *   Define SeaORM entities mirroring the existing SQLite schema (tables: `api_keys`, `models`, `fallback_config`, `analytics`).
    *   Create connection management and initialize the database connection.

4.  **Cryptography utility:**
    *   Implement `encrypt` and `decrypt` functions using the `aes-gcm` crate to match the Node.js implementation exactly (so existing keys aren't broken).

## Phase 2: Core Services

1.  **Rate Limiter:**
    *   Implement the sliding-window rate limiter.
    *   In Rust, this requires thread-safe shared state. We will use `dashmap` (a concurrent hashmap) or `Arc<RwLock<HashMap>>` to store the rate limit windows.
    *   Implement penalty tracking for `429` responses (the decay logic).

2.  **Provider Trait and Adapters:**
    *   Define an async `Provider` trait:
        ```rust
        #[async_trait]
        pub trait Provider: Send + Sync {
            fn platform(&self) -> String;
            async fn chat_completion(&self, key: &str, req: &ChatCompletionRequest) -> Result<ChatCompletionResponse, Error>;
            async fn stream_chat_completion(&self, key: &str, req: &ChatCompletionRequest) -> Result<BoxStream<'static, Result<Event, Error>>, Error>;
            async fn validate_key(&self, key: &str) -> Result<bool, Error>;
        }
        ```
    *   **Implement `OpenAICompatProvider`**: This will cover Groq, Cerebras, Mistral, OpenRouter, etc. It needs to handle streaming (`reqwest` + `tokio_util::codec::LinesCodec`).
    *   **Implement `GoogleProvider`**: Port the specific translation logic for Gemini's API format.

3.  **Router Service:**
    *   Port `server/src/services/router.ts`.
    *   Implement the logic to query the DB for the fallback chain, check rate limits, apply penalties, pick a key, and select the correct `Provider` implementation.

## Phase 3: API Endpoints (Axum)

1.  **Proxy Endpoint (`/v1/chat/completions`):**
    *   Create the main handler.
    *   If `stream: true`, return an SSE stream using `axum::response::sse::Sse`.
    *   If `stream: false`, return JSON.

2.  **Dashboard API Endpoints:**
    *   `/api/keys` (CRUD operations for keys, handle encryption on insert).
    *   `/api/models` (List models).
    *   `/api/fallback` (Get/update fallback chain).
    *   `/api/analytics` (Query analytics).

3.  **Static File Serving:**
    *   Configure Axum to serve the static files from `../client/dist`.
    *   Add the SPA fallback (return `index.html` for unknown routes not starting with `/api/` or `/v1/`).

## Phase 4: Background Tasks & Polish

1.  **Health Checker:**
    *   Implement a background task (`tokio::spawn`) that runs periodically to check the health of API keys, mirroring `server/src/services/health.ts`.

2.  **Analytics Recording:**
    *   Ensure analytics are written to the database asynchronously after requests complete so as not to block the response.

3.  **Cross-Compilation Setup:**
    *   Add documentation/scripts for cross-compiling the binary for `aarch64-linux-android` (Termux).
