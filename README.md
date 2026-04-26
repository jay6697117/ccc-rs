# ccc-rs

`ccc-rs` is a Rust rewrite of the Claude Code CLI. The goal is to provide a single native `ccc` binary with the core Claude Code workflows implemented through a modular Rust workspace.

## Status

This repository is under active development. The current codebase includes the foundation for configuration, authentication, API streaming, tools, MCP bootstrap, agent runtime, TUI integration, telemetry, session persistence, and headless output protocols.

## Workspace layout

```text
ccc-rs/
├── Cargo.toml
├── crates/
│   ├── ccc-core        # shared types, config schema, errors, traits
│   ├── ccc-platform    # platform detection and platform-specific helpers
│   ├── ccc-vim         # Vim mode state machine and editing logic
│   ├── ccc-auth        # OAuth, API key, AWS, and GCP authentication support
│   ├── ccc-api         # Anthropic API client and streaming protocol support
│   ├── ccc-tools       # tool implementations
│   ├── ccc-mcp         # MCP client/server protocol support
│   ├── ccc-agent       # agent loop, session runner, and runtime orchestration
│   ├── ccc-tui         # terminal UI integration
│   ├── ccc-telemetry   # tracing and telemetry bootstrap
│   └── ccc-cli         # `ccc` command-line entry point
├── docs/               # implementation plans and architecture notes
└── claude-copy-code/   # TypeScript source reference
```

## Features in progress

- Native Rust workspace for the `ccc` CLI.
- Interactive `ccc chat` runtime.
- Headless `ccc chat --print` mode with text, JSON, and stream JSON output.
- Session persistence and project-level session resume support.
- MCP server bootstrap and runtime integration.
- Configuration inspection through `ccc config show`.
- Telemetry setup through `tracing` and `tracing-subscriber`.

## Requirements

- Rust toolchain with Cargo.
- Network access and valid authentication for Claude API workflows.

## Build

```bash
cargo build
```

## Test

```bash
cargo test
```

## Run

Run the CLI from the workspace:

```bash
cargo run -p ccc-cli -- --help
```

Start an interactive chat session:

```bash
cargo run -p ccc-cli -- chat
```

Run a headless prompt:

```bash
cargo run -p ccc-cli -- chat --print "Explain this repository"
```

Use stream JSON output:

```bash
cargo run -p ccc-cli -- chat --print --output-format stream-json --include-partial-messages "Hello"
```

Show merged configuration:

```bash
cargo run -p ccc-cli -- config show
```

## Documentation

Architecture and phase planning documents are in [`docs/`](docs/), especially [`docs/plans/ARCHITECTURE.md`](docs/plans/ARCHITECTURE.md).

## License

This project is licensed under the MIT License. See [`LICENSE`](LICENSE) for details.
