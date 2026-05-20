# Hephaestus

**Hephaestus** is a self-repairing AI agent built entirely in **100% pure, memory-safe Rust**.

Designed for zero-trust environments, Hephaestus wraps critical software execution layers, catches fatal panics in real-time, extracts forensic telemetry (Time-Travel Memory Capture), and repairs the underlying codebase asynchronously in the background—all without blocking the main event loops.

## 🚀 Key Capabilities

* **Error Interception:** Rust traits capture panics natively on execution bounds.
* **Time-Travel Telemetry:** Captures exact stack frames and execution state at the exact moment of the crash.
* **Native AST Analysis:** Leverages `tree-sitter-rs` directly to understand the code structure (no sub-processes, no external Python scripts).
* **Bifurcated Orchestration:** Repairs are executed on a separate, non-blocking asynchronous pipeline powered by Tokio, freeing the main system to continue operations.
* **Zero-Trust Linux Sandbox:** Validates mutated/repaired code safely inside a Linux namespace using `unshare` and absolute timeouts before committing changes.
* **In-Process Repair Genome Storage:** Utilizes a local SQLite database (`rusqlite`) to persist repair patterns securely.
* **7-Phase Investigation Protocol:** A rigorous state-machine evaluation process encoded with compile-time hard gates.

## 🛠️ Architecture

At its core, Hephaestus does not use external runtimes or polyglot microservices. It is a strictly controlled Monolithic Rust binary running on Tokio. 

1. **Skill Fails:** An execution fails, panicking inside the `HephaestusInterceptor`.
2. **Telemetry Extraction:** The state, stack, and environment variables are snapshotted.
3. **Bifurcation:** The main loop immediately drops the failed process safely without crashing the main application thread.
4. **Background Repair (The 7 Phases):**
    * *Problem Definition & Extraction*
    * *Reproduction Attempt*
    * *Evidence Collection (AST parsing)*
    * *Hypothesis Formulation*
    * *Sandbox Validation (Zero-Trust Mutation)*
    * *Repair Execution & Fallbacks*
    * *Storing the "Repair Genome" locally via SQLite with Evo-Genome enhancements (semantic clustering, wing-based organization, AAAK compression)

## 📦 Installation

Since Hephaestus is just pure Rust, simply install safely via Cargo.

```bash
git clone https://github.com/WilgnerLucas111/hephaestus.git
cd hephaestus
cargo build --release
```

## 🧪 Testing the 7-Phase Protocol

Hephaestus strictly enforces zero compiler warnings and heavily utilizes the type-state pattern to ensure hard compilation validation of repair integrity.

Run the test suite, which includes automated mocking of SQLite Genomes and time-travel memory extraction:

```bash
cargo test -- --nocapture
```

## 🛡️ Best Practices & Security

- **Strict Permissions:** Sandbox configurations provide granular control over mutations (`ReadOnly`, `DangerFullAccess`, etc.).
- **Memory Safety:** 100% Rust architecture guarantees absence of manual memory leaks, Data Races, and use-after-free bugs.

## 📄 License

This project is licensed under the terms integrated within the repository (see [LICENSE](LICENSE)).
