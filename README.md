# gemini-remote-linux

[한국어 문서 (Korean)](./README.ko.md)

An unofficial Linux desktop daemon, CLI, and native Rust GUI (`gemini-ui`) that brings the **Gemini Desktop** experience and remote PC execution to Linux (Ubuntu, Arch, Fedora, Wayland/Hyprland, and X11).

Connect your Linux PC or workstation to Gemini (Mobile, Web, and the included native Linux GUI) for remote tool execution, local workspace access, shell commands, MCP servers, and GUI Computer Use.

> **Disclaimer**: This is an unofficial community client using Gemini desktop protocols. No OAuth secrets, tokens, or proprietary assets are bundled — you must supply your own OAuth client credentials and sign in with your Google account.

## Key Features

- **Native Linux Desktop GUI (`ui/`, Rust + `iced 0.14`)**
  - **Chat & Tasks**: Streaming Markdown & thinking view, Chat/Task mode switching with remote PC targeting, temporary chat, model picker with thinking level (`Low` / `Medium` / `High`), `+` tools popover, and `@` connectors/MCP mentions
  - **Workspace Sections**: Library (Canvas docs & generated image thumbnails), Image Studio, Scheduled Actions (cloud + local + recommended), Student learning tools, Notebooks, Gems hub, Video Studio, and Personalize
  - **Appearance & Language**: Automatic system Light/Dark theme following (plus manual override) and **English / Korean UI localization** (auto-detected from system locale, switchable anytime in Settings)
  - **Remote PC Control**: Toggle remote execution, manage allowed workspace folders, inspect connected MCP servers, and monitor live remote tool execution
- **Remote Desktop Daemon & CLI (`gemini-remote`, Go)**
  - Device registration, presence reporting, bidirectional streaming tool execution with automatic multi-turn session renewal, and local Unix socket IPC (`0600`)
  - **Shell / Code**: `run_command`, `command_status`, `send_command_input`, `run_python`
  - **File System & Search**: `read_file`, `view_file`, `write_file`, `replace_file_content`, `multi_replace_file_content`, `list_directory`, `find_by_name`, `grep_search`, `find_folders`, `connect_folders`
  - **Skills / MCP**: `list_skills`, `ReadSkill`, `list_mcp`, `execute_mcp` (built-in `git`, `obsidian`, plus external stdio MCP server bridge)
  - **GUI / Computer Use**: `ui_control` for **Wayland / Hyprland** (`hyprctl`, `grim`, `wtype`, `ydotool`) and **X11** (`xdotool`, `import`)

## Requirements

- **Daemon**: Go 1.22+
- **Desktop GUI (`ui/`)**: Rust 1.85+ (`edition = "2024"`)
- **Optional runtime tools** (for full GUI convenience):
  - Native file/folder picker: `xdg-desktop-portal`
  - Screen capture: `slurp` + `grim` (Wayland) or `import` / `scrot` (X11)
  - Voice input: `pw-record` (PipeWire) or `arecord` (ALSA)
  - Korean text rendering: `Noto Sans CJK KR` (or any CJK sans-serif system font)

## Build & Quick Start

```bash
# 1. Build the Go daemon
go build -o gemini-remote ./cmd/gemini-remote

# 2. Configure OAuth client credentials (from an existing Gemini desktop installation or ~/.config/gemini-remote/config.json)
./gemini-remote extract-keys /path/to/Gemini.app

# 3. Sign in with Google OAuth 2.0
./gemini-remote login

# 4. Allow local workspace folders
./gemini-remote folders add ~/dev/git-repo
./gemini-remote folders add ~/Downloads

# 5. Register your Linux machine and start the daemon
./gemini-remote register
./gemini-remote service install   # or `./gemini-remote serve` in the foreground

# 6. Build and launch the native Rust GUI
cargo build --release --manifest-path ui/Cargo.toml
./ui/target/release/gemini-ui
```

## License

[MIT](LICENSE)
