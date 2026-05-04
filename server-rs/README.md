# FreeLLMAPI Rust Backend (`server-rs`)

A high-performance, low-memory implementation of the FreeLLMAPI proxy, rewritten from Node.js to Rust.

## Features
- **Low Footprint:** Uses ~10MB RAM (vs ~40-100MB in Node.js).
- **Instant Boot:** Starts in <50ms.
- **Dynamic Providers:** Add any OpenAI-compatible provider via the database without code changes.
- **Android Ready:** Optimized for Termux and resource-constrained environments.

## Running the Binary

When you run the compiled binary, it needs to know where the React frontend files and the database are.

### Environment Variables
- `DATABASE_URL`: Path to your sqlite db (e.g. `sqlite://freeapi.db`).
- `ENCRYPTION_KEY`: Your 64-char hex key.
- `CLIENT_DIST`: Path to the frontend `dist` folder (defaults to `../client/dist`).

### Example (if running from `target/release`)
```bash
cd target/release
CLIENT_DIST="../../../client/dist" DATABASE_URL="sqlite://../../../server/data/freeapi.db" ./server-rs
```

### Production Setup (Recommended)
1. Create a folder for your deployment (e.g., `~/freellmapi`).
2. Copy the `server-rs` binary there.
3. Copy the `client/dist` folder there as well.
4. Create a `.env` file:
   ```env
   DATABASE_URL="sqlite://freeapi.db"
   CLIENT_DIST="./dist"
   ENCRYPTION_KEY="your-key-here"
   ```
5. Run `./server-rs`.

## Deployment & Production Build

To build a highly optimized local binary:
```bash
cargo build --release
```

## Cross-Compilation for Android (Termux)

The backend is configured with `rustls` and `sqlite-bundled` to ensure a static binary with no external C dependencies, making it highly portable for Android.

### Using `cargo-ndk` (Recommended)

1.  **Install NDK:** Install the Android NDK on your host machine.
2.  **Add Target:**
    ```bash
    rustup target add aarch64-linux-android
    ```
3.  **Install Helper:**
    ```bash
    cargo install cargo-ndk
    ```
4.  **Build:**
    ```bash
    cargo ndk -t aarch64-linux-android build --release
    ```

### Using `cross` (Docker)

If you have Docker installed, you can build without manually configuring the NDK:
```bash
cargo install cross
cross build --target aarch64-linux-android --release
```

### Installation on Phone

1.  Copy the resulting binary from `target/aarch64-linux-android/release/server-rs` to your Termux home directory.
2.  Give execution permissions:
    ```bash
    chmod +x server-rs
    ```
3.  Ensure your `.env` and `.db` files are present.
4.  Run:
    ```bash
    ./server-rs
    ```

## Dynamic Provider Configuration

You can add new OpenAI-compatible providers (like DeepSeek, Together, etc.) purely via the UI or by inserting into the `models` table.

**SQL Example:**
```sql
INSERT INTO models (platform, model_id, display_name, base_url, intelligence_rank, speed_rank)
VALUES ('deepseek', 'deepseek-chat', 'DeepSeek V3', 'https://api.deepseek.com/v1', 1, 5);
```

The system will automatically route requests for `deepseek-chat` to the specified `base_url`.
