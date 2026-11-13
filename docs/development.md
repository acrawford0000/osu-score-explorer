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

## Windows releases

After installing the runtime dependencies and copying a Windows x64 Node binary into `runtime/node.exe`, prepare the redistributable runtime and build the optimized installer:

```powershell
npm run release:prepare
npm run tauri -- build --config src-tauri/tauri.release.conf.json
```

The separate release configuration packages a per-user NSIS setup executable with `Score Explorer.runtime` beside the installed app. It can obtain WebView2 if missing. Staging excludes unused Android/Apple Realm libraries and includes Node and dependency license notices. The runtime preparation checks the actual bundled Node/Realm combination. Generated runtime files are ignored by Git. Release builds never fall back to a developer workspace for the lazer reader.

A portable package contains `Score Explorer.exe`, the complete `Score Explorer.runtime` directory, and license notices. Keep the runtime directory beside the executable after extraction. Portable users need WebView2 installed. Publish checksums with the Windows assets and verify both the setup archive and the standalone runtime before publishing.

## Checks

```powershell
npm run check
npm test
cargo test --manifest-path src-tauri/Cargo.toml --no-default-features
npm run tauri -- build --debug --no-bundle
```

## Source import and identity

Unchanged sources are skipped using source fingerprints. Changed stable databases are parsed once and compared against a hash set of known records before map resolution or calculation. Lazer uses source GUIDs and score-input fingerprints to avoid recalculating unchanged plays.

The configured folder list is persisted in the app database. Fingerprints are scoped to normalized client/folder identities; the original single-install settings are migrated automatically. Stable custom Songs paths remain attached to their own install. Artwork and replay references retain alternate copies across installs. Removing a folder stops future scans of that path and preserves imported history. Exact reimported lazer results can be linked across differing GUIDs only when player identity, timestamp, ruleset, gameplay semantics, judgments, score, combo, and mod settings match.

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

## Local music playback

Score details resolve audio across configured libraries in blocking tasks. Stable uses map metadata and each install's Songs root; lazer audio references share the source-fingerprinted read-only media index. Paths must resolve inside the appropriate Songs/file-store root. Only the resolved file is granted access through Tauri's asset protocol, which supports ranged streaming for seeking. The interface retains one audio element across views, starts at time zero, and stores volume locally. Floating controls stay within the application; there is no separate OS window or online audio fetch.

The browser fixture provides a generated silent WAV for playback, seek, float/dock, and drag checks. `inspect-library --audio-audit <app-cache.sqlite>` checks installed audio resolution without playing music.

## Current limitations

Some unsupported mod settings or incomplete/missing map revisions cannot be recalculated. Combined identity checks deliberately leave ambiguous records separate. The frontend and worker retain score summaries in memory; indexed SQLite queries remain an option for much larger libraries. Online replay fetching and direct launch into osu! are not implemented.
