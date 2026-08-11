# Hephaestus

**Hephaestus** is an experimental self-repair framework for Rust projects, exploring failure capture, isolated patch validation, and persistent repair memory.

> ⚠️ **Status:** Early proof of concept / research prototype. Not suitable for production or untrusted-code execution.

---

## 🚀 Overview & Key Features

Hephaestus wraps software execution, captures detailed panic telemetry at failure time, executes isolated background repair cycles on disposable workspace copies, and persists successful fixes into a SQLite "Repair Genome".

* **Non-Blocking Asynchronous Pipeline:** Background repairs run on an isolated Tokio `JoinSet` pipeline without stalling the main execution thread.
* **Disposable Workspace Isolation:** All candidate patches are compiled and tested inside temporary disposable workspace copies. Original source code files are never mutated directly during repair validation.
* **Real End-to-End Repair Engine:** Automatically reproduces failures (`cargo test`), generates patch candidates (Heuristic or optional OpenRouter LLM), applies patches, validates (`cargo check`, `cargo clippy`, `cargo test`), and produces unified diffs.
* **Secret Redaction & Telemetry Filtering:** Filters environment variables and redacts sensitive credentials (`*_TOKEN`, `*_SECRET`, `DATABASE_URL`, etc.).
* **Persistent Repair Genome:** Stores original code, telemetry triggers, patch diffs, and narrative summaries in SQLite via `rusqlite`.
* **Tribunal Architecture:** Organizes repair validation into specialized roles (`WildMonkey`, `NeutralJudge`, `AngryMaster`, `NarrativeAgent`).

---

## 🛠️ Architecture

```
Crash Intercepted -> Telemetry Snapshot -> Non-blocking Bifurcation
                                                │
                                                ▼
  ┌────────────────────────────────────────────────────────────────────────┐
  │                           The Courtroom / Tribunal                      │
  │                                                                        │
  │ 1. WildMonkey     -> Generates Patch Candidates (Heuristic / LLM)       │
  │ 2. NeutralJudge   -> Validates in Disposable Workspace (cargo test)   │
  │ 3. AngryMaster    -> Enforces Static Analysis & Safety Policies        │
  │ 4. NarrativeAgent -> Records Ledger into SQLite Repair Genome           │
  └────────────────────────────────────────────────────────────────────────┘
```

---

## 📦 Building & Testing

```bash
# Clone the repository
git clone https://github.com/WilgnerLucas111/hephaestus.git
cd hephaestus

# Run code formatting and clippy checks
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings

# Run all unit & integration tests
cargo test --all-targets
```

---

## 🔑 Optional OpenRouter LLM Integration

By default, Hephaestus uses a deterministic heuristic patch generator. You can optionally enable AI-driven patch generation via OpenRouter:

```bash
export OPENROUTER_API_KEY="sk-or-v1-..."
```

If no key is present or the API is unreachable, Hephaestus seamlessly falls back to deterministic heuristic patch generation.

---

## 📄 License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.
