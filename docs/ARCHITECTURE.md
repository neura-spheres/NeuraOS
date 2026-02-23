# NeuraOS System Architecture

## High-Level Layer Diagram

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│                          NEURAOS — AI-Native CLI OS                            │
│                                                                                 │
│  ┌───────────────────────────────────────────────────────────────────────────┐  │
│  │                        TUI DESKTOP ENVIRONMENT                           │  │
│  │  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────────┐  │  │
│  │  │ Window   │ │ Status   │ │ Command  │ │ Notif    │ │ Workspace    │  │  │
│  │  │ Manager  │ │ Bar      │ │ Palette  │ │ Center   │ │ Switcher     │  │  │
│  │  └──────────┘ └──────────┘ └──────────┘ └──────────┘ └──────────────┘  │  │
│  └───────────────────────────────────────────────────────────────────────────┘  │
│                                      │                                          │
│  ┌───────────────────────────────────┼───────────────────────────────────────┐  │
│  │                     APPLICATION FRAMEWORK LAYER                           │  │
│  │                                                                           │  │
│  │  ┌─────────┐ ┌─────────┐ ┌─────────┐ ┌─────────┐ ┌─────────┐           │  │
│  │  │NeuraDoc │ │NeuraMail│ │NeuraCalc│ │NeuraDev │ │NeuraChat│  ...       │  │
│  │  └────┬────┘ └────┬────┘ └────┬────┘ └────┬────┘ └────┬────┘           │  │
│  │       │           │           │           │           │                  │  │
│  │  ┌────┴───────────┴───────────┴───────────┴───────────┴──────────────┐  │  │
│  │  │              App Trait  ·  Lifecycle  ·  Sandbox  ·  Config        │  │  │
│  │  └───────────────────────────────────────────────────────────────────┘  │  │
│  └───────────────────────────────────────────────────────────────────────────┘  │
│                                      │                                          │
│  ┌───────────────┐  ┌────────────────┼──────────────────┐  ┌─────────────────┐ │
│  │  SHELL LAYER  │  │         AI AGENT CORE             │  │  PLUGIN SYSTEM  │ │
│  │               │  │                                    │  │                 │ │
│  │ ┌───────────┐ │  │  ┌──────────┐  ┌──────────────┐  │  │ ┌─────────────┐ │ │
│  │ │ Parser    │ │  │  │ Gemini   │  │ Agent Engine  │  │  │ │ Extension   │ │ │
│  │ │ Executor  │ │  │  │ Client   │  │              │  │  │ │ Registry    │ │ │
│  │ │ Builtins  │ │  │  │ Streaming│  │ Planner      │  │  │ │ Loader      │ │ │
│  │ │ Pipes     │ │  │  │ Tools    │  │ Tool Registry│  │  │ │ Sandboxed   │ │ │
│  │ │ Aliases   │ │  │  │ Retry    │  │ Executor     │  │  │ │ AI Inject   │ │ │
│  │ │ Scripting │ │  │  └──────────┘  │ Guardrails   │  │  │ └─────────────┘ │ │
│  │ └───────────┘ │  │               │ Memory Mgr   │  │  │                 │ │
│  └───────────────┘  │               └──────────────┘  │  └─────────────────┘ │
│                      │                                    │                     │
│                      │  ┌──────────────────────────────┐  │                     │
│                      │  │       MEMORY SYSTEM           │  │                     │
│                      │  │  Short │  Long  │  System    │  │                     │
│                      │  │  Term  │  Term  │  Memory    │  │                     │
│                      │  └──────────────────────────────┘  │                     │
│                      └────────────────────────────────────┘                     │
│                                      │                                          │
│  ┌───────────────────────────────────┼───────────────────────────────────────┐  │
│  │                      CORE SERVICES LAYER                                  │  │
│  │                                                                           │  │
│  │  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────────┐  │  │
│  │  │ Process  │ │ Service  │ │ Security │ │ Logging  │ │ Networking   │  │  │
│  │  │ Manager  │ │ Manager  │ │ Engine   │ │ Engine   │ │ Stack        │  │  │
│  │  │          │ │          │ │          │ │          │ │              │  │  │
│  │  │ Tasks    │ │ Daemons  │ │ Auth     │ │ Tracing  │ │ HTTP/WS     │  │  │
│  │  │ Scheduler│ │ Registry │ │ RBAC     │ │ Metrics  │ │ SMTP/IMAP   │  │  │
│  │  │ Signals  │ │ Deps     │ │ Sandbox  │ │ Aggreg.  │ │ TCP/UDP     │  │  │
│  │  │ Lifecycle│ │ Restart  │ │ Crypto   │ │ Rotation │ │ Sync Engine │  │  │
│  │  └──────────┘ └──────────┘ └──────────┘ └──────────┘ └──────────────┘  │  │
│  └───────────────────────────────────────────────────────────────────────────┘  │
│                                      │                                          │
│  ┌───────────────────────────────────┼───────────────────────────────────────┐  │
│  │                    STORAGE & PERSISTENCE LAYER                            │  │
│  │                                                                           │  │
│  │  ┌───────────────────────┐  ┌────────────────────┐  ┌─────────────────┐  │  │
│  │  │  Virtual Filesystem   │  │   Database Layer   │  │  Config Engine  │  │  │
│  │  │                       │  │                    │  │                 │  │  │
│  │  │  Dirs / Files / Meta  │  │  SQLite Wrapper    │  │  TOML Registry  │  │  │
│  │  │  Permissions          │  │  Migrations        │  │  Per-User       │  │  │
│  │  │  Locking / Journal    │  │  Schema Versioning │  │  Per-App        │  │  │
│  │  │  Transactions         │  │  Query Abstraction │  │  Hot Reload     │  │  │
│  │  └───────────────────────┘  └────────────────────┘  └─────────────────┘  │  │
│  └───────────────────────────────────────────────────────────────────────────┘  │
│                                      │                                          │
│  ┌───────────────────────────────────┼───────────────────────────────────────┐  │
│  │               KERNEL INTERFACE LAYER (Linux Abstraction)                  │  │
│  │                                                                           │  │
│  │  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────────┐  │  │
│  │  │ Syscall  │ │ Signal   │ │ PTY      │ │ FS Ops   │ │ Network Ops  │  │  │
│  │  │ Wrapper  │ │ Handler  │ │ Manager  │ │ Wrapper  │ │ Wrapper      │  │  │
│  │  └──────────┘ └──────────┘ └──────────┘ └──────────┘ └──────────────┘  │  │
│  └───────────────────────────────────────────────────────────────────────────┘  │
│                                      │                                          │
│  ┌───────────────────────────────────┴───────────────────────────────────────┐  │
│  │                         HOST LINUX KERNEL                                 │  │
│  └───────────────────────────────────────────────────────────────────────────┘  │
│                                                                                 │
│  ┌──────────────────────────────────────────────────────────────────────────┐   │
│  │                      PACKAGE MANAGER (neurapkg)                          │   │
│  │  install · remove · update · search · deps · sign · registry · cache    │   │
│  └──────────────────────────────────────────────────────────────────────────┘   │
│                                                                                 │
│  ┌──────────────────────────────────────────────────────────────────────────┐   │
│  │                       MULTI-USER SYSTEM                                  │   │
│  │  Accounts · Auth · Password Hash · RBAC · Home Dirs · AI Isolation      │   │
│  └──────────────────────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────────────────────┘
```

## Data Flow Diagram

```
User Input (keyboard)
       │
       ▼
┌──────────────┐     ┌──────────────┐
│  TUI Desktop │────▶│ Command      │
│  (ratatui)   │     │ Palette      │
└──────┬───────┘     └──────┬───────┘
       │                     │
       ▼                     ▼
┌──────────────┐     ┌──────────────┐
│  Active App  │     │    Shell     │
│  (focused)   │     │  (neura-sh)  │
└──────┬───────┘     └──────┬───────┘
       │                     │
       ├─────────┬───────────┤
       ▼         ▼           ▼
┌──────────┐ ┌──────────┐ ┌──────────┐
│ AI Agent │ │ Storage  │ │ Services │
│ Core     │ │ Layer    │ │ Layer    │
└──────┬───┘ └──────┬───┘ └──────┬───┘
       │            │            │
       ▼            ▼            ▼
┌──────────────────────────────────────┐
│        Kernel Interface Layer        │
└──────────────────┬───────────────────┘
                   ▼
           Linux Kernel
```

## Inter-Crate Dependency Graph

```
neura-kernel ◄────────────────────────────────────────────────┐
     ▲                                                         │
     │                                                         │
neura-storage ◄──────────────────────┬────────────────────┐   │
     ▲                                │                    │   │
     │                                │                    │   │
neura-users ◄───────────────┐        │                    │   │
     ▲                       │        │                    │   │
     │                       │        │                    │   │
neura-security ◄────────┐   │        │                    │   │
     ▲                   │   │        │                    │   │
     │                   │   │        │                    │   │
neura-process ◄──────┐  │   │        │                    │   │
neura-services ◄─┐   │  │   │        │                    │   │
neura-logging    │   │  │   │        │                    │   │
     ▲           │   │  │   │        │                    │   │
     │           │   │  │   │        │                    │   │
neura-network    │   │  │   │        │                    │   │
     ▲           │   │  │   │        │                    │   │
     │           │   │  │   │        │                    │   │
neura-ai-core ◄──┤   │  │   │        │                    │   │
     ▲           │   │  │   │        │                    │   │
     │           │   │  │   │        │                    │   │
neura-shell      │   │  │   │        │                    │   │
     ▲           │   │  │   │        │                    │   │
     │           │   │  │   │        │                    │   │
neura-desktop ◄──┘   │  │   │        │                    │   │
     ▲               │  │   │        │                    │   │
     │               │  │   │        │                    │   │
neura-app-framework  │  │   │        │                    │   │
     ▲               │  │   │        │                    │   │
     ├───────────────┘  │   │        │                    │   │
     │                  │   │        │                    │   │
neura-apps ◄────────────┘   │        │                    │   │
     ▲                      │        │                    │   │
     │                      │        │                    │   │
neura-pkg ◄─────────────────┘        │                    │   │
     ▲                               │                    │   │
     │                               │                    │   │
neura-config ◄───────────────────────┘                    │   │
     ▲                                                    │   │
     │                                                    │   │
neura-plugins ◄───────────────────────────────────────────┘   │
     ▲                                                        │
     │                                                        │
neuraos (binary) ◄────────────────────────────────────────────┘
```

## Crate Manifest

| Crate                  | Purpose                                          |
|------------------------|--------------------------------------------------|
| `neura-kernel`         | Linux syscall abstraction, signal handling, PTY   |
| `neura-storage`        | VFS, SQLite wrapper, migrations, journaling       |
| `neura-users`          | User accounts, auth, password hashing, RBAC       |
| `neura-security`       | Permission engine, sandboxing, crypto, signing     |
| `neura-process`        | Process abstraction, task scheduler, signals       |
| `neura-services`       | Daemon supervisor, dependency resolution, restart  |
| `neura-logging`        | Tracing integration, log aggregation, rotation     |
| `neura-network`        | HTTP, WebSocket, SMTP, IMAP, TCP abstractions      |
| `neura-ai-core`        | Gemini client, agent engine, memory system         |
| `neura-shell`          | Shell parser, executor, builtins, pipes, scripting |
| `neura-desktop`        | TUI window manager, status bar, workspaces         |
| `neura-app-framework`  | App trait, lifecycle, sandbox, config per app      |
| `neura-apps`           | All built-in applications                          |
| `neura-pkg`            | Package manager, registry, dependency resolution   |
| `neura-config`         | TOML config engine, hot reload, per-user/per-app   |
| `neura-plugins`        | Plugin loader, extension registry, runtime loading |
| `neuraos`              | Main binary, boot sequence, orchestration          |

## Directory Layout

```
neuraos/
├── Cargo.toml                    # Workspace root
├── docs/
│   └── ARCHITECTURE.md           # This file
├── crates/
│   ├── neura-kernel/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── syscall.rs
│   │       ├── signal.rs
│   │       ├── pty.rs
│   │       └── fs_ops.rs
│   ├── neura-storage/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── vfs/
│   │       │   ├── mod.rs
│   │       │   ├── node.rs
│   │       │   ├── permissions.rs
│   │       │   ├── journal.rs
│   │       │   └── transaction.rs
│   │       ├── db/
│   │       │   ├── mod.rs
│   │       │   ├── connection.rs
│   │       │   ├── migration.rs
│   │       │   └── query.rs
│   │       └── paths.rs
│   ├── neura-users/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── account.rs
│   │       ├── auth.rs
│   │       ├── password.rs
│   │       └── roles.rs
│   ├── neura-security/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── permissions.rs
│   │       ├── sandbox.rs
│   │       ├── crypto.rs
│   │       └── signing.rs
│   ├── neura-process/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── task.rs
│   │       ├── scheduler.rs
│   │       └── lifecycle.rs
│   ├── neura-services/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── daemon.rs
│   │       ├── registry.rs
│   │       └── dependency.rs
│   ├── neura-logging/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── subscriber.rs
│   │       ├── aggregator.rs
│   │       └── rotation.rs
│   ├── neura-network/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── http.rs
│   │       ├── websocket.rs
│   │       ├── smtp.rs
│   │       ├── imap.rs
│   │       └── tcp.rs
│   ├── neura-ai-core/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── gemini/
│   │       │   ├── mod.rs
│   │       │   ├── client.rs
│   │       │   ├── streaming.rs
│   │       │   ├── tools.rs
│   │       │   └── retry.rs
│   │       ├── agent/
│   │       │   ├── mod.rs
│   │       │   ├── engine.rs
│   │       │   ├── planner.rs
│   │       │   ├── tool_registry.rs
│   │       │   ├── executor.rs
│   │       │   └── guardrails.rs
│   │       └── memory/
│   │           ├── mod.rs
│   │           ├── short_term.rs
│   │           ├── long_term.rs
│   │           └── system.rs
│   ├── neura-shell/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── parser.rs
│   │       ├── executor.rs
│   │       ├── builtins.rs
│   │       ├── pipes.rs
│   │       ├── aliases.rs
│   │       └── scripting.rs
│   ├── neura-desktop/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── window.rs
│   │       ├── statusbar.rs
│   │       ├── palette.rs
│   │       ├── notifications.rs
│   │       ├── workspaces.rs
│   │       └── theme.rs
│   ├── neura-app-framework/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── app_trait.rs
│   │       ├── lifecycle.rs
│   │       ├── sandbox.rs
│   │       └── config.rs
│   ├── neura-apps/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── docs/
│   │       ├── mail/
│   │       ├── calc/
│   │       ├── files/
│   │       ├── settings/
│   │       ├── terminal/
│   │       ├── monitor/
│   │       ├── notes/
│   │       ├── tasks/
│   │       ├── chat/
│   │       ├── dev/
│   │       ├── calendar/
│   │       ├── contacts/
│   │       ├── ssh_client/
│   │       ├── ftp_client/
│   │       ├── weather/
│   │       ├── clock/
│   │       ├── sysinfo/
│   │       ├── logs/
│   │       ├── sync/
│   │       ├── backup/
│   │       ├── media/
│   │       ├── db_browser/
│   │       ├── store/
│   │       ├── browse/
│   │       └── sheets/
│   ├── neura-pkg/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── install.rs
│   │       ├── remove.rs
│   │       ├── update.rs
│   │       ├── deps.rs
│   │       ├── registry.rs
│   │       └── cache.rs
│   ├── neura-config/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── loader.rs
│   │       ├── watcher.rs
│   │       └── schema.rs
│   └── neura-plugins/
│       ├── Cargo.toml
│       └── src/
│           ├── lib.rs
│           ├── loader.rs
│           ├── registry.rs
│           └── sandbox.rs
└── src/
    └── main.rs                   # neuraos binary entry point
```

## Boot Sequence

```
1. neuraos binary starts
2. Initialize tracing/logging (neura-logging)
3. Load system config (neura-config)
4. Initialize kernel abstraction (neura-kernel)
5. Initialize storage layer (neura-storage)
   ├── Open/create SQLite database
   ├── Run migrations
   └── Mount virtual filesystem
6. Initialize user system (neura-users)
   ├── Load user database
   └── Prompt for login
7. Authenticate user
8. Initialize security context (neura-security)
9. Start service manager (neura-services)
   └── Launch registered daemons
10. Initialize process manager (neura-process)
11. Initialize AI core (neura-ai-core)
    ├── Load API credentials
    ├── Restore memory state
    └── Register system tools
12. Initialize shell (neura-shell)
13. Start TUI desktop (neura-desktop)
    ├── Render window manager
    ├── Restore previous session
    └── Launch default apps
14. Enter main event loop
    └── Poll: keyboard → TUI → apps → AI → services
```
