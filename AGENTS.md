# TuckNotes - Agent Guidelines

## Project Overview

TuckNotes is a macOS desktop app that captures meeting audio (system + microphone) for local AI-powered note-taking. Built with Tauri (Rust backend, React/TypeScript frontend). Audio capture uses Apple's ScreenCaptureKit via the `screencapturekit` crate.

## Rust Backend Architecture (`src-tauri/src/`)

```
src-tauri/src/
├── main.rs               # Entry point — do not modify
├── lib.rs                # Module declarations + Tauri builder (run())
├── errors.rs             # Centralized AppError enum
├── commands/             # Thin #[tauri::command] handlers
├── models/               # Serializable structs, shared types, app state
└── services/             # Business logic, FFI, capture pipeline
```

### Layer responsibilities

- **`commands/`** — Tauri command handlers. These are thin: validate input, call a service, return a result. No business logic here. Each command must be `pub` and registered in `lib.rs` via `tauri::generate_handler![]`.
- **`models/`** — Data structs that cross boundaries (Rust-to-JS serialization, shared between commands and services). All models derive `serde::Serialize`. App state structs (e.g., `RecordingState`) also live here.
- **`services/`** — Where the real work happens. Audio capture, permission FFI, and any future processing logic. Services should not depend on Tauri types (`AppHandle`, `State`) directly — those are passed in by commands.
- **`errors.rs`** — Single `AppError` enum for all command errors. Do not use `Result<T, String>` in commands.

### Adding a new feature

1. Define data types in `models/`
2. Implement logic in `services/`
3. Create a thin command in `commands/` that calls the service
4. Register the command in `lib.rs`
5. Add any new error variants to `AppError` in `errors.rs`

## Error Handling

All `#[tauri::command]` functions return `Result<T, AppError>`. Never use `Result<T, String>`.

`AppError` uses tagged serde serialization so the frontend receives structured errors:

```rust
#[derive(Debug, Serialize)]
#[serde(tag = "kind", content = "message")]
pub enum AppError {
    CaptureFailed(String),
    LockPoisoned,
    NotSupported,
}
```

On the frontend, catch blocks receive `{ kind: string; message?: string }`:

```typescript
try {
  await invoke("start_recording");
} catch (error) {
  const err = error as { kind: string; message?: string };
  switch (err.kind) {
    case "CaptureFailed": // err.message has details
    case "LockPoisoned": // internal state error
    case "NotSupported": // non-macOS platform
  }
}
```

When adding new error variants, add them to the `AppError` enum with appropriate `Display` formatting.

## macOS Platform Guards

All macOS-specific code (ScreenCaptureKit, CoreGraphics FFI, AVFoundation FFI) must be gated with `#[cfg(target_os = "macos")]`. Non-macOS paths should return `AppError::NotSupported`. The `services/` module declarations in `services/mod.rs` are already cfg-gated.

## Frontend (`src/`)

**Layout**

- **`features/`** — Domain modules: `recording` (providers, transcript state, `AudioVisualizer`), `meetings`, `settings`, `models` (Whisper/LLM catalog + `useModelManager`), `onboarding`, `theme`. Co-locate types, hooks, and feature UI; use `index.ts` barrels where helpful.
- **`layout/`** — App shell (`AppLayout`, `MeetingOverlay`).
- **`editor/`** — TipTap editor subtree: `templates/`, `ui/`, `primitives/`, `icons/`, `extensions/`, `nodes/`, plus `tiptap-utils.ts` and editor-only hooks (`use-tiptap-editor`, `use-cursor-visibility`, `use-menu-navigation`).
- **`components/ui/`** — Shared design-system primitives (buttons, sidebar, etc.).
- **`hooks/`** — Generic reusable hooks only (measurement, throttling, refs) — **no** domain or Tauri-specific recording logic.
- **`lib/`** — Pure utilities: `utils.ts` (`cn`), `audio-level.ts`, `format-time.ts`. Tests live next to sources as `*.test.ts`.
- **Entry points** — `App.tsx`, `main.tsx`, `overlay-main.tsx` at the `src/` root; global styles in `App.css` / `overlay.css`.

- React + TypeScript + Vite
- Tauri commands are called via `invoke()` from `@tauri-apps/api/core`
- Backend-to-frontend events: use the `useTauriEvent` hook (`src/hooks/use-tauri-event.ts`) for component-lifetime subscriptions — it handles async registration, StrictMode double-mount, and cleanup, and its handler is a fresh closure each render (no `useCallback` needed). For imperative per-request listener groups (e.g. a chat send), combine `listen()` registrations with `listenBatch` (`src/lib/tauri-events.ts`). Do not hand-roll `listen()` + mounted-flag effects.
- **Tailwind CSS v4** for all app UI styling — no custom CSS files per component. Exception: the vendored `src/editor/` subtree keeps its own SCSS (`primitives/`, `nodes/`, etc.); don't add new SCSS outside it.
- Audio level meters use dB-scale conversion with peak-hold smoothing

### Linting & formatting

- ESLint (flat config, `eslint.config.js`): typescript-eslint recommended + `react-hooks/rules-of-hooks` (error) / `react-hooks/exhaustive-deps` (warn). The react-hooks v7 compiler rules are deliberately not enabled — the codebase uses render-time latest-ref patterns.
- Prettier with default options (`.prettierrc`).
- `npm run lint`, `npm run lint:fix`, `npm run format`, `npm run typecheck`. CI (`.github/workflows/ci.yml`) runs lint, frontend tests, and the build on every PR.

### Styling with Tailwind CSS v4

- All styles use Tailwind utility classes directly in JSX `className` props. Do not create separate `.css` files for components.
- Tailwind is loaded via `@tailwindcss/vite` plugin — no PostCSS config or `tailwind.config.js`.
- Theme customization (brand colors, fonts) is in `src/App.css` using `@theme { }` blocks, not a config file.
- Custom brand tokens: `primary` (#4361ee), `primary-hover` (#3a56d4), `success` (#06d6a0), `danger` (#e53e3e). Use as `bg-primary`, `text-danger`, etc.
- Dark mode uses Tailwind's `dark:` variant, which follows `prefers-color-scheme` automatically. Base dark/light styles are set on `<html>` in `index.html`.
- Custom keyframe animations (e.g., `pulse-ring`) live in `src/App.css` alongside the `@theme` block.

## Key Dependencies

- `screencapturekit` — Rust bindings for Apple ScreenCaptureKit (macOS only)
- `objc2` / `objc2-foundation` / `block2` — Objective-C FFI for AVFoundation permissions
- `bytemuck` — Zero-copy casting for PCM audio data
- `tokio` — Async runtime for the audio chunk processing pipeline
- `tailwindcss` / `@tailwindcss/vite` — Utility-first CSS framework (v4) with Vite integration
