# Purge

**A neon-grade Windows junk cleaner, rebuilt from scratch in Rust + egui.**

Purge is the evolution of a PowerShell junk-cleaning script into a full desktop
application: parallel scanning, a curated whitelist catalog, a conservative
orphan detector that actually proves a folder belongs to an uninstalled app,
package-manager management (cache clean **and** full removal), and one-click
browser update pausing — all wrapped in an animated dark-neon UI.

![Screenshot](assets/icon_256.png)

---

## Features

### Deep Clean
- **Whitelist-only catalog** (~50 categories, 130+ concrete cache locations):
  temp folders, GPU shader caches (NVIDIA / AMD / Intel / DirectX), Windows
  Update downloads, Delivery Optimization cache, crash dumps, WER reports,
  CBS/Panther logs, thumbnail & font caches, browser caches (Chrome, Edge,
  Brave, Opera, Vivaldi, Firefox, Thunderbird) and per-app caches
  (Discord, Teams, Slack, Spotify, Adobe, Steam, CapCut, VS Code, OBS, Zoom…).
- **Risk tiers**: `SAFE` targets start selected, `MODERATE` targets
  (Spotify offline music, Adobe media cache, Windows.old…) start unselected.
- Every delete passes through a **safety guard** that refuses anything inside
  Documents, Desktop, Downloads, Pictures, Videos, Music, OneDrive or the user
  profile root.

### Accurate orphan scan (AppData / ProgramData leftovers)
A folder is only proposed as a leftover of an uninstalled app when **every**
identity source agrees it is unknown:

- Add/Remove Programs registry (both hives + WOW6432Node),
- Microsoft Store / Appx packages,
- Program Files folders and Start Menu shortcuts,
- running processes,
- every registered service.

…**and** it has been idle longer than the configured age (default 90 days,
adjustable 14–365). Each candidate shows its evidence; nothing is bulk-deleted.

### Package managers (25 supported)
pip, uv, Poetry, pipx, Conda, npm, Yarn, pnpm, Bun, Deno, Cargo, rustup, Go,
NuGet, Maven, Gradle, sbt/Ivy, Composer, Chocolatey, Scoop, vcpkg, Cabal,
HuggingFace, PyTorch Hub, Playwright/Patchright.

For each manager: detected size, cache-only cleaning, **full data removal**
(with explicit warnings when the tool itself is removed), and a one-click
**official uninstaller** launcher when one is registered (e.g. Anaconda).

### Browser update control
Pause automatic updates for Chrome, Edge, Brave, Opera, Vivaldi and Firefox by
combining all three official mechanisms:

1. updater **scheduled tasks** disabled,
2. updater **services** stopped and disabled (original start types are saved),
3. vendor **update policies** written to the registry
   (`UpdateDefault`, `AutoUpdateCheckPeriodMinutes`, `DisableAppUpdate`).

Resume restores everything exactly. Browser caches can be cleaned separately —
cache only: passwords, history, bookmarks and logins are never touched.

### Safety model
- Admin manifest (`requireAdministrator`) — Purge launches elevated via UAC.
- **System restore point** before every clean (optional, on by default).
- Deletion mode is **permanent** by default; an optional Recycle-Bin mode is
  available in Settings.
- Confirmation dialog listing exactly what will happen before every run.
- Session log file + live activity log page.

## UI / animations

Built with egui; everything is drawn with the egui painter — no HTML, no CSS:

- neon gradient logo and section underlines,
- animated circular progress ring with pulse glow,
- shimmer progress bars with a moving highlight band,
- iOS-style sliding toggles and neon checkboxes,
- hover glow on cards and buttons,
- page fade transitions,
- particle burst when a clean completes.

## Building

Requirements: Windows 10/11, Rust 1.85+ (stable).

```powershell
git clone https://github.com/<you>/purge
cd purge
cargo run            # dev build with console
cargo build --release
```

The release binary is `target\release\purge.exe` (single file, no runtime
dependencies).

### Build on GitHub (no local toolchain needed)

1. Push this repository to GitHub.
2. Go to **Actions → Release → Run workflow** for an instant artifact, or
   push a tag `v1.0.0` to get a GitHub Release with `Purge-windows-x64.zip`.
3. CI runs `cargo check` + `cargo clippy` + `cargo fmt --check` on every push.

The manifest (`purge.manifest`) makes Windows show the UAC prompt and marks the
exe as DPI-aware / long-path aware. An icon is embedded from
`assets/icon.ico`; replace that file to rebrand.

## Project structure

```
src/
├── main.rs             entry point, elevation fallback
├── app.rs              state machine, navigation, modals, worker wiring
├── theme.rs            dark-neon palette + egui style
├── guard.rs            personal-folder safety guard (every delete passes here)
├── log.rs              session log (memory + file)
├── settings.rs         persisted settings (%APPDATA%\Purge\settings.json)
├── platform/
│   ├── win.rs          registry, schtasks, services, elevation, PATH scan
│   └── fallback.rs     no-op stubs for non-Windows hosts (dev only)
├── engine/
│   ├── paths.rs        curated clean-target catalog
│   ├── scan.rs         parallel scanner + orphan identity engine
│   ├── clean.rs        deletion core, restore point, service handling
│   ├── packages.rs     25 package managers: detect / cache / remove
│   └── browsers.rs     update pause/resume + cache cleaning
└── ui/
    ├── widgets.rs      toggles, cards, neon ring, shimmer bars, bursts
    ├── dashboard.rs, deep_clean.rs, packages.rs, browsers.rs,
    ├── settings.rs, log_view.rs
```

## Extending

- **Add a clean target**: append a `TargetDef` to `TARGETS` in
  `src/engine/paths.rs` (paths, tier, kind — `Contents` keeps the folder).
- **Add a package manager**: append a `PkgDef` to `MANAGERS` in
  `src/engine/packages.rs`.
- **Add a browser**: append a `BrowserDef` to `BROWSERS` in
  `src/engine/browsers.rs`.
- The orphan protected-name list lives in `src/engine/scan.rs`
  (`PROTECTED_NAMES`).

## License

MIT — see [LICENSE](LICENSE).

## Disclaimer

Purge deletes files. Cache targets are whitelist-only and orphan deletion
requires per-item review, but you are responsible for what you confirm.
Use the preview of the log page if anything looks surprising, and keep the
restore-point option enabled.
