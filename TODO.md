# Planned features

## Replay availability and playback

- [x] Detect available local replay data without requiring a manual replay export.
- [x] Check stable's local replay storage and lazer's score file references; verify the matching score and map revision.
- [ ] Distinguish local replay available, online replay downloadable, not found, and not checked yet.
- [x] Add an accessible export button to cards and details with a verified local replay.
- [x] Export the complete original `.osr` file without changing the source, including lazer extensions.
- [x] Remember a default export folder, prompt on the first export if unset, and avoid overwriting existing exports.
- [ ] Offer **Open in osu!** as a separate optional action; verify launch support for each client before choosing a default playback flow.
- [ ] Offer an explicit download action for an available online replay and handle authentication or unavailable downloads.
- [x] Cache the lazer replay-reference index and refresh when the database changes; check visible scores in background threads and revalidate before export.
- [x] Test older retained replays, merged copies, missing map files, missing replay files, mismatched replay data, and preservation of lazer extensions.

## More exploration

- [ ] Saved searches and personal retry lists.
- [ ] Compare attempts on the same map.
- [ ] More dashboard statistics and milestone views.
- [ ] Optional online beatmap information.
- [ ] Collection editing.

## Connected libraries and music

- [x] Configure multiple stable and lazer folders, with a separate optional Songs path per stable install.
- [x] Retain independent scan fingerprints and file references; keep shared scores together across installations.
- [x] Add First, Last, −10 and +10 page navigation with boundary checks.
- [x] Play installed map audio from the beginning from score details.
- [x] Provide compact sidebar playback, expanded seek/volume controls, and a draggable in-app player.

## Calculation speed and responsiveness

- [ ] Add bounded parallel workers for first-run map difficulty, stars, actual PP, and PP-if-FC calculations. Keep CPU capacity available for the interface and other applications.
- [ ] Share difficulty results for identical map revisions, rulesets, mods, and custom rate settings so several plays do not repeat the same expensive work.
- [ ] Deduplicate calculations queued at the same time and persist completed results in batches through a single database writer.
- [ ] Prioritize visible scores, then finish the remaining library in the background.
- [ ] Add progress, cancellation, pause/resume, and restart recovery for long first imports and calculator updates.
- [ ] Profile large-library first runs and measure UI responsiveness while checking for updates, decoding maps, calculating stars, and calculating actual/FC PP.
- [x] Move search, filtering, sorting, summaries, progression aggregation, and paging into a background worker; debounce typing and ignore stale replies.
- [ ] Consider indexed SQLite queries for libraries too large to comfortably keep in memory.

Imports and calculations already run outside the UI thread. Per-map calculations within an import are currently sequential; broader parallel calculation is planned rather than implemented.
