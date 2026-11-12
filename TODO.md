# Planned features

## Replay availability and playback

- [ ] Detect available replay data for each score without requiring a manual replay export.
- [ ] Check stable's local replay storage and lazer's score file references; resolve the matching score and map revision.
- [ ] Distinguish local replay available, online replay downloadable, not found, and not checked yet.
- [ ] Add a small accessible replay button to cards with a verified replay available. Prioritize local replays.
- [ ] Make **Export replay (.osr)** the primary action: save a usable replay file, similar to pressing F2 in osu!, without changing the original replay.
- [ ] Offer **Open in osu!** as a separate optional action; verify launch support for each client before choosing a default playback flow.
- [ ] Offer an explicit download action for an available online replay and handle authentication or unavailable downloads.
- [ ] Cache availability checks and refresh them when relevant source files change.
- [ ] Test older retained replays, imported copies across clients, missing maps, deleted replay files, and scores without online replay access.

## More exploration

- [ ] Saved searches and personal retry lists.
- [ ] Compare attempts on the same map.
- [ ] More dashboard statistics and milestone views.
- [ ] Optional online beatmap information.
- [ ] Collection editing.

## Calculation speed and responsiveness

- [ ] Add bounded parallel workers for first-run map difficulty, stars, actual PP, and PP-if-FC calculations. Keep CPU capacity available for the interface and other applications.
- [ ] Share difficulty results for identical map revisions, rulesets, mods, and custom rate settings so several plays do not repeat the same expensive work.
- [ ] Deduplicate calculations queued at the same time and persist completed results in batches through a single database writer.
- [ ] Prioritize visible scores, then finish the remaining library in the background.
- [ ] Add progress, cancellation, pause/resume, and restart recovery for long first imports and calculator updates.
- [ ] Profile large-library first runs and measure UI responsiveness while checking for updates, decoding maps, calculating stars, and calculating actual/FC PP.
- [ ] Move large search, filtering, aggregation, and paging workloads behind indexed queries or a worker if profiling shows the frontend needs it.

Imports and calculations already run outside the UI thread. Per-map calculations within an import are currently sequential; broader parallel calculation is planned rather than implemented.
