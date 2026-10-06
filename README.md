# Canvasist

> Never miss an assignment deadline again.

Canvasist is an open-source desktop app that gathers every assignment from **Canvas** and **Gradescope** into one minimal list, shows each deadline with its submission status, and reminds you before anything is due.

**Status:** early development — not usable yet.

## Why

Gradescope assignments often don't appear in Canvas's assignment list, so checking Canvas alone makes it easy to miss work. Canvasist pulls from both, merges duplicates, and keeps everything in one place.

## Planned features

- Works with most Canvas instances — just point it at your school's Canvas URL
- Gradescope access through your Canvas LTI integration (no separate Gradescope login)
- Current-term assignments, quizzes and graded discussions
- Detailed status: not submitted, submitted, late, graded, missing, dismissed
- Duplicate Canvas/Gradescope entries merged into one
- Customizable desktop notifications before deadlines
- Runs quietly in the system tray with a small footprint; optional start on login
- English and Chinese interface

## Privacy

Canvasist runs entirely on your computer. There is no Canvasist backend, telemetry or analytics.

- You sign in on your school's own Canvas login page inside a separate window; Canvasist never sees your password.
- The resulting session and any cached assignments are encrypted with Windows DPAPI, so only your Windows account can read them, and they are only ever sent to your own Canvas site (and, later, Gradescope).
- School search sends the name you type to Instructure's public school directory (the same one the official Canvas apps use). Entering your Canvas address directly skips this.
- Signing out deletes the saved session, cached assignments and the login window's browser data.

## Platform support

Windows first. macOS and Linux may follow.

## Development

**Prerequisites (Windows):** Node.js 24+, the Rust stable toolchain (MSVC), Visual Studio C++ Build Tools, and WebView2 (preinstalled on Windows 11).

```sh
npm ci                 # install frontend dependencies (install scripts are disabled by .npmrc)
npm run tauri dev      # run the app with hot reload
npm run tauri build    # build the release app and installer
```

Checks run in CI on every pull request:

```sh
npm run typecheck && npm run lint && npm run format:check && npm test
cd src-tauri && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
```

### Security model

- The main window only loads the bundled UI. A strict Content Security Policy blocks remote scripts, frames and connections.
- Every Rust command is permission-gated: the main window's capability (`src-tauri/capabilities/main-window.json`) lists exactly the commands it may call. Remote pages get no capabilities.
- Logs never contain credentials, cookies, or personal data such as course names, assignment titles or grades.

## License

[MIT](LICENSE)
