# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

Canvasist is a Windows tray app (Tauri 2: Rust backend in `src-tauri/`, React 19 + TypeScript frontend in `src/`). It merges a student's Canvas and Gradescope assignments into one list and sends deadline reminders. It has no server, no telemetry and no access-token option: the only way in is the user's own Canvas login session, captured in an in-app login window.

## Commands

```sh
npm ci                  # .npmrc sets ignore-scripts=true and save-exact=true
npm run tauri dev       # app with hot reload (Vite on fixed port 1420)
npm run tauri build     # release app + NSIS installer in src-tauri/target/release/bundle/nsis/

# Frontend checks (all run in CI)
npm run typecheck && npm run lint && npm run format:check && npm test
npx vitest run src/grouping.test.ts     # one test file
npx vitest run -t "name of the test"    # tests matching a name

# Rust checks, run from src-tauri/ (all run in CI, clippy with -D warnings)
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
cargo test merge::                      # tests in one module
cargo test probe_all_terms -- --ignored --nocapture   # live Gradescope probe; needs a signed-in session
```

CI (`.github/workflows/ci.yml`) runs exactly these checks on `windows-latest`. TypeScript stays on 6.x because typescript-eslint does not support 7 yet.

## Architecture

### Backend (`src-tauri/src/`)

- **`lib.rs`** wires everything together. It registers the plugins (single-instance must stay first), the managed state (`SettingsStore`, `Account`, `MarkStore`, `ReminderLog`), the tray and the scheduler. Closing the window only prevents exit; the process ends only on the tray's Quit. `--autostart` launches straight to the tray.
- **`account.rs`** owns sign-in, the session and the refresh pipeline: Canvas fetch → `gradescope::refresh` → `merge::merge` → encrypted snapshot, then the `snapshot-updated` event. It also emits `auth-changed` and `canvas-login-cancelled`. Sign-out wipes the session, the snapshot, marks, the reminder log and the login window's WebView2 data. Files WebView2 still has locked are deleted on the next start (`finish_pending_wipe`).
- **Login window** (`canvas-login`) loads `{canvas}/login` in its own WebView2 data dir (`login-webview`). After each page load its cookies are copied and checked against `/api/v1/users/self`. `cookies_for_url` must run on the async runtime; calling it synchronously in the page-load handler deadlocks WebView2.
- **`canvas/`**:
  - `client.rs` calls the REST API with the session cookies (`cookies.rs` is a custom `reqwest` cookie store that also keeps session-only cookies).
  - It sends `Accept: application/json+canvas-string-ids`, strips the `while(1);` guard and follows pagination only on the same origin.
  - It never follows redirects (`http.rs`): a 401 or any redirect means the session expired.
  - `discovery.rs` handles school search and checks that an address is a Canvas site.
- **`gradescope/`**: Gradescope has no API.
  - `bridge.rs` opens a hidden window (`gradescope-bridge`, sharing `login-webview`) and clears its old Gradescope cookies. It injects the Canvas cookies with `set_cookie` (an explicit domain is required) and opens the course's Gradescope LTI tab.
  - `launch.js`, an initialization script, retargets Canvas's `tool_form` so the launch runs as the whole page rather than inside Canvas's frame. Cookies set inside the frame are third-party and can be blocked or kept separate on some machines.
  - Each new set of Gradescope cookies is checked against `/account` (no cookie names are assumed). Cookies that pass are copied into the jar.
  - `client.rs` and `parse.rs` then fetch and scrape the pages over plain HTTP with `scraper`.
  - Gradescope IDs carry a `gs:` prefix.
- **`merge.rs`** merges Canvas/Gradescope duplicates by normalized title plus a deadline within 24 h, or by Canvas's link to Gradescope. Gradescope's deadline and status win, and a differing Canvas deadline is kept in `canvas_due_at`. Ambiguous matches stay as two entries on purpose.
- **`scheduler.rs` + `reminders.rs`**: a single async task ticks every 30 s. It refreshes hourly, after a wake (a wall-clock jump), and 5 min after a failure. `reminders.rs` is pure logic: offsets come from settings, missed reminders catch up as a single one, and a late deadline gets one more reminder 3 h before it. The sent log is keyed by `id|deadline|slot`, so reminders re-arm when a deadline changes. `notify.rs` sends toasts; debug builds borrow PowerShell's AUMID.
- **Storage**:
  - `secure_store.rs` writes DPAPI-encrypted `<name>.bin` records in the app local data dir. Use it for the session, snapshot, marks and reminder log: anything personal.
  - `settings.rs` is plain JSON in the config dir and must hold only non-personal preferences (course IDs are fine; names are not).
- **`dpapi.rs`** is the only module allowed `unsafe`; the crate sets `unsafe_code = "deny"`.
- **`error.rs`**: `AppError` serializes to `{ kind, ... }` for the frontend.

### Frontend (`src/`)

- `harden.ts` freezes `Object.prototype` and must stay the first import in `main.tsx`. Do **not** use Tauri's `freezePrototype` option instead: Tauri injects it into every webview, and it breaks schools' SSO/Duo pages in the login window.
- `ipc.ts` has typed wrappers for every command and event; keep it in sync with `commands.rs` and `account.rs`. `errors.ts` maps `AppError.kind` to localized text.
- `App.tsx` holds the state machine: loading → connect → login pending → list or settings. It also filters hidden courses out of the snapshot. Pure logic lives in tested modules: `grouping.ts` (Overdue/Today/Tomorrow/7 days/Later plus folded Completed and Dismissed), `urgency.ts` and `format.ts`.
- i18n: `i18n/en.ts` is the source catalog. `zh-CN.ts` is typed as `Record<MessageKey, string>`, so a missing or extra key fails `typecheck`.

## Rules that span files

- **Adding an IPC command** takes five edits, or the window can't call it:
  1. Write the function in `commands.rs`.
  2. Register it in `generate_handler!` in `lib.rs`.
  3. Add it to the app manifest list in `build.rs`.
  4. Add `allow-<name>` to `capabilities/main-window.json`.
  5. Add a wrapper in `src/ipc.ts`.
- Only the bundled `main` window has a capability. Remote-content windows (login, Gradescope bridge) must never be given one. The CSP in `tauri.conf.json` is strict; keep it that way.
- Logs, test output and the live probe must never contain cookies, credentials, course names, assignment titles or grades. Log counts and status labels only. Test fixtures and placeholders use generic hosts such as `canvas.example.edu`, never a real school.
- `spikes/` holds throwaway experiments (Playwright scripts) and is not part of the app. Vitest excludes it.
- Code comments are in English.
