# osu! Score Explorer

Rediscover your osu! scores. Find the plays you're proud of, the chokes worth another try, and the progress you've made along the way.

osu! Score Explorer brings local **stable and lazer scores** into one desktop score browser, with support for **osu!standard, taiko, catch, and mania**. Matching imported copies of a play stay together so they count once in your combined history.

![Score Explorer in the Bento theme with a sample library](docs/images/explorer-bento.jpg)

## Explore your scores

- Browse artwork cards or switch to a detailed score table.
- Search by song, artist, difficulty, or mapper.
- Filter by player, gamemode, stable/lazer, mods, date, star rating, and misses.
- Keep both clients selected or explore either history on its own.
- Sort by PP, stars, accuracy, most recent, or potential FC PP gain.
- See achieved combo alongside the map's maximum combo.

## Find your next goal

- **One miss chokes:** the plays that came close.
- **SS chokes:** find near-perfect plays and choose your own **Max non-300s** limit. One miss, two 50s, or three 100s can all be worth revisiting.
- **Combo chokes:** zero-miss plays that still fell short of a full combo.
- **Full combos and SS scores:** revisit your achievements.
- **PP if FC:** compare your result with an estimated full-combo result.

Your filters work together, so you can search for something as specific as recent HD one-miss plays above 6 stars, or older SS opportunities with at most three non-300 judgments.

## See your progression

Explore average **star rating, PP, and accuracy** by month or year. Hover or focus a chart point to see its exact average and how many scores contributed to it.

Choose a player and gamemode, narrow the history with your score filters, and compare all attempts with the best result per map and mod combination in each period.

## Make it yours

Keep the original Explorer theme, switch to light mode, or try **Bento, Serika Dark, Nord, Catppuccin, and Paper**. Your theme, score filters, sort order, and card/table preference are remembered between sessions.

## Project status

This is an early desktop preview under active development. Replay discovery and export, saved views, retry lists, and more ways to compare attempts are on the [roadmap](TODO.md).

[Development and running from source](docs/development.md)

## Credits

Built with [Tauri](https://tauri.app/) and [Svelte](https://svelte.dev/). Score reading uses [osu-db](https://github.com/kovaxis/osu-db), and PP calculations use [rosu-pp](https://github.com/MaxOhn/rosu-pp). Additional color palettes draw from [Monkeytype's themes](https://github.com/monkeytypegame/monkeytype/blob/master/frontend/src/ts/constants/themes.ts).

An independent community project, unaffiliated with the osu! team.

Licensed under [GPL-3.0](LICENSE). See [third party acknowledgements](THIRD_PARTY_NOTICES.md).
