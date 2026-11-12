# Development

The desktop application uses Tauri 2, Svelte 5, TypeScript, Rust, and an app-owned SQLite index. The lazer reader uses a bundled Node/Realm runtime and opens an immutable read-only snapshot. Source game databases are never migrated or modified.

## Requirements

- Node.js and npm.
- Rust with the MSVC toolchain on Windows.
- Visual Studio C++ Build Tools and Microsoft WebView2.

## Setup

```powershell
npm ci
npm ci --prefix runtime
Copy-Item (Get-Command node.exe).Source runtime/node.exe
npm run tauri -- dev
```

The Windows preview locates its lazer runtime in either a `Score Explorer.runtime` folder beside the executable or the project's `runtime` folder during development. Keep `node.exe`, the Realm dependencies, `import-lazer.cjs`, and `statistics.cjs` together. Runtime binaries and dependencies are not committed to the repository.

For browser-only UI work, use `npm run dev` and choose the sample library. Native imports require the desktop app.

## Checks

```powershell
npm run check
npm test
cargo test --manifest-path src-tauri/Cargo.toml --no-default-features
npm run tauri -- build --debug --no-bundle
```

## Source import and identity

Unchanged sources are skipped using source fingerprints. Changed stable databases are parsed once and compared against a hash set of known records before map resolution or calculation. Lazer uses source GUIDs and score-input fingerprints to avoid recalculating unchanged plays.

Cross-client links use namespaced legacy IDs and complete result checks, with conservative fingerprint matching when IDs are absent. Native lazer plays are never merged with stable merely because results resemble each other. Raw records are retained independently of canonical links. Verified duplicate pairs can establish player-name aliases for a shared online user ID. Placeholder/guest IDs (including 1) are excluded, including when reading older caches.

Lazer statistics can use older PascalCase or newer snake_case judgment names. Normalization and ruleset-specific mapping are tested independently. Full mod settings are retained for calculations.

Close lazer for an import of a changed database. The Windows snapshot reader requires a read handle that excludes active writers. Imports and calculations run outside the UI thread.

## Local data

App data is stored under `%LOCALAPPDATA%/dev.score-explorer.desktop`. Personal scores, game database snapshots, credentials, and build output must not be committed or included in public screenshots.

Source filters default to both stable and lazer. Missing values are shown as unavailable rather than zero, and chart averages count only available values while exposing their coverage.

## Search and replay checks

Filtering, sorting, whole-result summaries, and progression run in a Web Worker. The UI sends the library once per load and receives only the requested page IDs and aggregate values. Search input is coalesced for 120 ms; request IDs prevent stale results from replacing a newer query. Library objects are immutable raw Svelte state to avoid proxy overhead and allow structured cloning.

Local replay detection/export runs in Tauri blocking tasks. Stable replay filenames use map MD5 and Windows file time. Lazer replay paths come from `Score.Files` in a read-only snapshot, cached by source fingerprint. Exports validate the replay header and copy the original bytes, preserving newer metadata. Merged scores can use either retained source copy. The destination is a user-selected folder and new files are created exclusively, with numbered names on collisions.

For a 30,720-score browser UI fixture, open `/tests/preview.html` on the dev server. Add `?fresh` to clear the fixture's default export folder. It uses synthetic scores and simulated native dialogs/exports, records input delay and long tasks, and never reads game files. Rust tests and the private `inspect-library --replay-audit` diagnostic exercise real file export separately.

## Current limitations

Some unsupported mod settings or incomplete/missing map revisions cannot be recalculated. Combined identity checks deliberately leave ambiguous records separate. The frontend and worker retain score summaries in memory; indexed SQLite queries remain an option for much larger libraries. Online replay fetching and direct launch into osu! are not implemented.
