# Third party acknowledgements

The application uses Tauri, Svelte, Lucide, osu-db, rosu-pp, rosu-mods, SQLite and rusqlite, reqwest/rustls, zip, and the Realm JavaScript SDK. Their copyright notices and license terms remain with their respective projects and packages.

Concurrent beatmap downloads use [osu-downloader](https://github.com/DesireThings/osu-shelf-legacy/tree/17ee63688d6b29bc8dede70be92aac44534fdc18/osu-downloader), derived from uwuclxdy/osu-downloader, under MIT OR Apache-2.0. The included MIT notice is in `licenses/osu-downloader-MIT.txt`. Public mirrors are independent services. Optional map identification uses osu! API v2.

The 2026 calculator uses [Raregendary/rosu-pp-gemini](https://github.com/Raregendary/rosu-pp-gemini/tree/ba1741510d798c35f86b8aec6e026d5cb5399571), derived from MaxOhn/rosu-pp, under MIT. Its included notice is in `licenses/rosu-pp-MIT.txt`. This community Rust port is independent of the official lazer C# engine.

The osu! Web Dark and Discord Dark themes use original control styling with colors inspired by those services. No logos or application components are bundled.

The additional Bento, Serika Dark, Nord, Catppuccin, and Paper color references are adapted from [Monkeytype's theme constants](https://github.com/monkeytypegame/monkeytype/blob/master/frontend/src/ts/constants/themes.ts). Monkeytype is distributed under GPL-3.0. Score Explorer's surface styling is adapted for this application's controls and readability; no Monkeytype application components are included.

The bundled Windows lazer reader runtime includes Node.js and Realm dependencies. Preserve their included license files when redistributing the runtime. Node.js is distributed under its included license, and Realm JavaScript/Core under Apache-2.0, with additional third-party notices in their packages.

osu! artwork and map metadata belong to their respective creators. They are read from the user's local installation and are not included in this source repository. The project is not affiliated with osu! or Monkeytype.
