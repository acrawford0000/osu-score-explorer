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

Player filtering uses `id:<online ID>` when available. Names are trimmed and compared case-insensitively. Stable names join an account only when ownership is unambiguous. Selected/default account profiles are checked in a background task and cached for seven days; public `previous_usernames` and `join_date` supply historical aliases and account creation dates. Only a few conflicting known username holders are checked, with paced requests, response limits, timeouts, and failure cooldowns. Existing cached matching works offline. No credentials or private profile fields are retained.

Legacy lazer imports can assign the current holder of an old replay username (see [ScoreImporter.populateUserDetails](https://github.com/ppy/osu/blob/974f9621c696cdc50ebf8886eeea79577f0e0c0c/osu.Game/Scoring/ScoreImporter.cs)). Such legacy IDs are rejected for plays predating that account's creation; profile history can then associate them with a unique older account. Genuine native lazer IDs remain intact. When both holders could own an unidentified play, it remains ambiguous. Settings also offers explicit saved name associations. Original score names and raw records remain unchanged. Exploration and progression share this identity filter.

Close lazer for an import of a changed database. The Windows snapshot reader requires a read handle that excludes active writers. Imports and calculations run outside the UI thread.

## Local data

App data is stored under `%LOCALAPPDATA%/dev.score-explorer.desktop`. Personal scores, game database snapshots, credentials, and build output must not be committed or included in public screenshots.

Source filters default to both stable and lazer. Missing values are shown as unavailable rather than zero, and chart averages count only available values while exposing their coverage.

## Search and replay checks

Filtering, sorting, whole-result summaries, and progression run in a Web Worker. The UI sends the library once per load and receives only the requested page IDs and aggregate values. Search input is coalesced for 120 ms; request IDs prevent stale results from replacing a newer query. Library objects are immutable raw Svelte state to avoid proxy overhead and allow structured cloning.

Local replay detection/export runs in Tauri blocking tasks. Stable replay filenames use map MD5 and Windows file time. Lazer replay paths come from `Score.Files` in a read-only snapshot, cached by source fingerprint. Exports validate the replay header and copy the original bytes, preserving newer metadata. Merged scores can use either retained source copy. The destination is a user-selected folder and new files are created exclusively, with numbered names on collisions.

For a 30,720-score browser UI fixture, open `/tests/preview.html` on the dev server. Add `?fresh` to clear the fixture's default export folder. It uses synthetic scores and simulated native dialogs/exports, records input delay and long tasks, and never reads game files. Rust tests and the private `inspect-library --replay-audit` diagnostic exercise real file export separately.

### Online replays

Settings connects an osu! API application through client-credentials/public scope. Client credentials are validated before being saved using current-user Windows DPAPI in the stable application database (`osu_api_connection_dpapi_v1`). Bearer tokens remain in memory. Startup reconnects in a blocking worker independently of library loading; no secret is returned to the frontend. Disconnect keeps saved details but disables automatic reconnection; Forget removes them. Failed replacement credentials preserve the previous saved and active connection. The friend whose scores are imported does not need to authenticate. Requests use the official `/api/v2/scores/{id}` and `/api/v2/scores/{id}/download` endpoints for unified IDs; stable legacy IDs use `/scores/{ruleset}/{legacy-id}` and its download route. Merged aliases retain alternative online IDs. Missing IDs are never guessed from usernames or beatmap titles. [osu! API documentation](https://osu.ppy.sh/docs/index.html) and [the server's download implementation](https://github.com/ppy/osu-web/blob/master/app/Http/Controllers/ScoresController.php) define these routes.

Only visible scores and the opened score are checked, after local replay detection and a short typing debounce. Checks run sequentially off the UI thread, share an official API request budget with beatmap ID lookups, and leave stale page results unused. Availability is cached per namespaced score ID and score-input fingerprint: positive results for 24 hours, missing results for six hours. Authentication errors, timeouts, server errors, and rate limits are not cached as missing. Retry-After pauses checks and the interface can retry later. API errors do not affect local exports.

The replay is downloaded only on an explicit export action. Responses are size bounded; downloaded bytes must parse as a replay and match a retained score's ruleset, map hash, timestamp, combo, judgments, and legacy total score where applicable. The original file, including lazer extensions, is preserved. Server filenames are ignored; exports share the sanitized local filename and collision-safe destination flow. Successful export paths are indexed and revalidated for future local availability; the app does not silently retain another replay binary. No beatmap or background is needed to export a replay.

`?replays=online&fresh` exercises online buttons, connection, the initial destination prompt, and reuse of the exported replay with a synthetic browser fixture. Native HTTP fixtures cover OAuth/public headers, namespaced IDs, cached availability, merged-ID fallback, rate limiting, missing replays, invalid content, score mismatches, preserved extensions, and collision handling. These tests simulate the API; a real authenticated server download requires user-provided API credentials.

## Local music playback

Score details resolve audio across configured libraries in blocking tasks, using a read-only cache connection. Playback requests never refresh the lazer snapshot or wait for its replay-index lock. Missing files return an unavailable message; the interface stops waiting after eight seconds and ignores late replies after a timeout or a change of score. Stable uses the shared beatmap index, with legacy catalog lookup only before an index exists; lazer audio references share the source-fingerprinted read-only media index. Paths must resolve inside the appropriate Songs/file-store root. Only the resolved file is granted access through Tauri's asset protocol, which supports ranged streaming for seeking. The interface retains one audio element across views, starts at time zero, and stores volume locally. Floating controls stay within the application; there is no separate OS window or online audio fetch.

The browser fixture provides a generated silent WAV for playback, seek, float/dock, and drag checks. `inspect-library --audio-audit <app-cache.sqlite>` checks installed audio resolution without playing music.

## Copying tables

Score table view copies the currently displayed page in its filtered/sorted order; progression copies the displayed metric and all displayed periods. Values use the same rounding, percentages, dates, and unavailable markers as the interface. Discord output uses aligned plain-text code blocks, with repeated headings in chunks at most 2,000 characters. The first click copies the first part; subsequent controls copy the remaining parts or the entire snapshot. Copying never posts to Discord. Long song labels are shortened with an ellipsis to keep columns readable, and line breaks/backticks in labels cannot escape the code block. Tests cover row preservation across parts, numeric alignment, Unicode widths, and formatting escapes.

Use `?audio=missing` or `?audio=hang` on the browser fixture to exercise missing-file feedback and timeout recovery.

## Shared beatmap index

Stable imports match exact MD5 revisions across every configured stable Songs folder and indexed lazer file store, including standalone score archives without their own beatmap catalog. The first scan indexes local `.osu` files; later scans reuse unchanged file stamps. Only hashes, metadata, and validated media paths are cached. Per-source index generations let an ordinary rescan repair previously unmatched scores without recalculating existing PP results. Removed files retain descriptive metadata but are never used for calculations or playback.

The sibling `osu!.db` catalog has its own fingerprint in each stable source configuration. Adding, replacing, or removing it refreshes descriptive metadata even when `scores.db` is unchanged, while reusing cached calculations. A catalog can identify a score's song and difficulty without containing the actual `.osu` file needed for PP calculations or any audio/artwork. `inspect-library --map-audit <stable-folder> [hashes...]` reports score-to-catalog coverage and selected exact-revision matches without modifying game files.

## Missing beatmap downloads

Import and Settings expose the same optional no-video downloader and destination. Recovery respects the current player/alias filter, persists jobs in SQLite, and can be resumed after restarting. Catalogs are indexed once per file stamp into an MD5-to-beatmap/set-ID cache. Unknown hashes use the official OAuth client-credentials public scope and checksum lookup. The shared Settings API connection supplies these credentials from the backend; download requests no longer accept separate frontend credentials. No official credentials are sent to mirrors. API lookup is paced to at most one request per 1.1 seconds. Transient failures and HTTP 429 use bounded retries and Retry-After/backoff.

Downloads use the MIT/Apache-2.0 `osu-downloader` library from DesireThings/osu-shelf-legacy, pinned at `17ee63688d6b29bc8dede70be92aac44534fdc18`. Up to three sets download concurrently through a shared scheduler. Eight no-video mirrors are configured: Nerinyan, osu.direct, Sayobot, Beatconnect, osudl, Catboy, Hinamizawa, and Nekoha. Rate limits, transient failures, and unavailable downloads trigger bounded retries and source failover; scheduler cooldowns survive batches within the queue. ZIP validation checks downloaded archives before a single coordinator installs files and writes SQLite. Extraction also strips video, validates paths and symlinks, enforces size/count limits, and uses a fresh folder without overwriting custom maps. Archives are temporary. The destination is a shared map library, with no synthetic game score database.

The global three-transfer limit covers both primary and fallback attempts. Nekoha remains the primary; normal request-spacing waits do not redirect otherwise healthy work onto slower mirrors. A failed or rate-limited primary routes that map through the remaining providers, retaining shared cooldowns. Connection establishment is limited to five seconds and response/read stalls to eight seconds per attempt; continuously progressing transfers have no whole-file time cap. The dependency's `test-helpers` feature exposes its HTTP-client override, used here to apply those production timeouts while preserving manual redirect handling. The UI reports active transfers and distinguishes downloaded sets from completed installations. Regression tests cover a server that never sends headers, three simultaneous transfers, avoiding cold fallbacks on healthy work, and 404/429 failover.

After each install, only the new `.osu` files are hashed and indexed. An expression index resolves affected raw score rows by MD5. Stable calculations reuse the same ruleset/completeness policy; native lazer calculations reconstruct retained judgment/mod/maximum statistics. Existing PP is preserved. Changes commit in one transaction and the interface polls lightweight status plus score patches; it preserves canonical source/player aliases and sends updated summaries to the query worker. Artwork can load when its availability changes. No complete stable/lazer database scan is triggered per download, and Explorer does not edit osu!.db. Stable's own song-select refresh is independent.

Unmatched historical revisions and API not-found results remain unavailable rather than repeatedly downloading the current set. Upgrading from the old single-mirror policy retries prior failures once. Manual retries and external filesystem watching remain roadmap items. Local HTTP tests exercise the production library's 404/429 failover and concurrent downloads, alongside authentication, archive validation, exact matching, score patches, and calculation persistence.

## Calculator selection

The desktop default is the Rust 2026 rules port (`rosu-pp-gemini` 5.0.4, revision `ba1741510d798c35f86b8aec6e026d5cb5399571`). This is a community port of lazer rules, not the official C# engine. The previous rosu-pp 4.0.1 calculator remains selectable. New results are keyed by engine and input fingerprint in `calculator_results`; original rosu results stay in their existing tables. Switching engines changes the projected view, without deleting either cache. Missing new-engine results are displayed as pending and computed by a background worker, grouped by exact map revision with shared difficulty attributes per ruleset/mod combination. Score patches are capped at 1,000 per poll and sent incrementally to the search worker.

Private validation against 20 public standard top plays found an average absolute PP difference of 0.0078 and a maximum of 0.071; ten taiko top plays differed by less than 0.0005 PP. These are close comparisons against those samples, not a guarantee of exact agreement for every map, mod, or future rules update. Profile JSON and personal reports are not distributed with the source.

## Other limitations

Some unsupported mod settings or incomplete/missing map revisions cannot be recalculated. Combined identity checks deliberately leave ambiguous records separate. The frontend and worker retain score summaries in memory; indexed SQLite queries remain an option for much larger libraries. Direct replay launch into osu! is not implemented.

Replay checks stay limited to the visible page and opened score. Individual lookup failures are recorded in the page status and do not terminate the remaining checks. Metadata mismatches try alternative stored online IDs and are persisted separately from unavailable replays, keyed by namespaced ID and input fingerprint, for 24 hours. Changing relevant imported score data invalidates that mismatch. Existing availability cache lifetimes remain 24 hours for available replays and six hours for unavailable replays. Temporary failures are not saved as unavailable. HTTP 429 pauses the page and schedules a retry after the cooldown. Progress shows checked, available and unverified counts.

Online replay verification compares map, ruleset, time, combo, primary judgments and legacy score. Standard's Geki/Katu combo-end bonus counters and taiko's unused auxiliary counters are not required to match reconstructed web headers. Catch's Katu and mania's Geki/Katu remain strict. The server reconstructs legacy replay headers from its score model: [LegacyReplayFile](https://github.com/ppy/osu-web/blob/master/app/Libraries/Score/LegacyReplayFile.php). Tests cover absent standard bonuses, byte-preserving export to the requested directory, and rejection of different meaningful judgments. Expanded scores link to the official site using unified IDs or the legacy ruleset namespace; only validated numeric IDs are passed to the system browser.
