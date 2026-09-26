# End-to-end tests

Drives the real InfinaBox desktop app, not a mock: the built Tauri binary is
launched by `tauri-driver`, and the tests talk to its webview over
WebDriver. Godot, git, and the file system are all real, and the tests check
results on disk (with `git`) as well as in the UI.

Linux only for now, because `tauri-driver` has no macOS support; on Linux it
wraps WebKitGTK's `WebKitWebDriver`.

## What it covers

The Phase A exit criterion (`docs/superpowers/plans/2026-09-25-phase-a-foundations.md`),
except the AI chat turn itself:

| Step | Checks |
|---|---|
| 1 | App starts on Home; `claude` and `git` show as installed |
| 2 | New Project (through the real native folder dialog) creates `project.godot`, `.ibproject/.ibx`, the addon, and exactly one snapshot, "New project" (checked with `git log`); the app lands on Studio |
| 3 | Play panel shows Godot installed; Play → running, output shows `[infinabox] ready 1`, a Godot window exists (screenshot); Stop → stopped and the process is gone |
| 4 | A script error is written to disk and snapshotted through the app's `snapshot_create` command (as an AI turn would); Play → the error shows with `res://player.gd, line N`; "Ask AI to fix" puts the request in the chat |
| 5a | History "Undo last change" restores the file on disk, the list updates, the running game restarts (new process) without errors |
| 5b | History "Go back" (through its confirm dialog) puts the broken version back, and the game restarts with the error again |
| 6a–c | Fresh app data and no `INFINABOX_GODOT`: the Play panel offers "Install Godot"; installing shows real byte progress and ends installed; the managed Godot runs the game |

Scenario 6 downloads the real Godot release (~75 MB) from GitHub.

## Prerequisites

- An X display. On a headless machine: `Xvfb :99 -screen 0 1600x1000x24 &` and `export DISPLAY=:99`.
- `webkit2gtk-driver` (provides `WebKitWebDriver`): `apt install webkit2gtk-driver`.
- `tauri-driver`: `cargo install tauri-driver --locked`.
- `xdotool` and ImageMagick (`import`, for screenshots): `apt install xdotool imagemagick`.
- Node 20+.
- A debug build of the app with the frontend embedded (a `tauri dev` build
  loads the frontend from the dev server instead, which these tests don't start):

  ```
  npx tauri build --debug --no-bundle    # from the repo root → target/debug/tauri-app
  ```

- For scenario 1–5: a Godot 4 binary, passed as `INFINABOX_GODOT`.

## Running

```
cd e2e
npm install
INFINABOX_GODOT=/path/to/Godot_v4.7.2-stable_linux.x86_64 npm test          # all scenarios
INFINABOX_GODOT=... node run.mjs --scenario core                              # steps 1–5
node run.mjs --scenario install                                               # step 6
```

Options (environment):

| Variable | Default |
|---|---|
| `INFINABOX_APP` | `../target/debug/tauri-app` |
| `TAURI_DRIVER` | `tauri-driver` on `PATH`, else `~/.cargo/bin/tauri-driver` |
| `WEBKIT_DRIVER` | `WebKitWebDriver` on `PATH` |
| `INFINABOX_GODOT` | none (required for `core`) |
| `DISPLAY` | `:99` |

Flags: `--scenario core|install|all`, `--artifacts <dir>`, `--keep` (keep the
per-launch temp home instead of deleting it).

Output goes to `e2e/artifacts/<timestamp>/` (git-ignored): a screenshot after
every step plus extra ones at key moments (the game window, the error list,
the install progress), the Play panel's game log after each game step, the
app's own stdout/stderr (`<scenario>.app.log`), and `report.json` with each
step's result and notes. The exit code is 0 only when every step passed.

## How it works, and the non-obvious parts

- **Isolation.** Each app launch gets its own temp `HOME` and `XDG_*`
  dirs, so app data (where a managed Godot goes), the webview's
  `localStorage` (recent projects, layouts) and GTK settings start empty
  and nothing persists. `PATH` is kept, so tool detection sees the
  machine's real `claude` and `git`. Projects are created under the system
  temp dir, never inside this repository (core refuses a project inside
  another git repository).
- **The native folder dialog.** "New Project → Choose…" opens a GTK file
  chooser that WebDriver can't see. `lib/x11.mjs` drives it with `xdotool`:
  focus it, Ctrl+L, type the folder path with a trailing slash, Enter. GTK
  ignores Enter while the chooser shows its default "Recent" view, so each
  launch sets the chooser's `startup-mode` to `cwd` through the keyfile
  GSettings backend (`GSETTINGS_BACKEND=keyfile` and a keyfile under the
  temp `XDG_CONFIG_HOME`).
- **Sound.** Machines without a sound card make Godot print a real engine
  `ERROR` (ALSA `ERR_CANT_OPEN`) on every run, which the Play panel lists as
  a game error. Each launch's temp home gets an `.asoundrc` with a null
  default device so the only errors left are the game's own.
- **Stable selectors.** Tests find elements by `data-testid` attributes
  (Home's New Project dialog and tool rows, the Play panel, its install
  card, error rows and output log, the History panel), and read state from
  `data-*` attributes (`data-game-state`, `data-godot`, `data-list-status`)
  rather than from display text. The chat composer is found by its
  `aria-label`.
- **Waiting, not sleeping.** Every check polls the real DOM, disk, or
  process table until it holds or times out. The only fixed pauses are a
  short settle before each screenshot (the X picture lags the DOM under
  Xvfb) and letting a just-started game draw frames or report errors.
- **Game processes** are found by their real command line (`ps`), matched
  on the project's path, so a restart is proven by a new process id, and
  the game window by `xdotool search --pid`.
- **Standing in for the AI.** Step 4 edits a script on disk and then calls
  the app's own `snapshot_create` command through the webview's IPC bridge
  (`window.__TAURI_INTERNALS__.invoke`), the same call the frontend makes.
  When the chat backend lands, a real chat turn should replace this.
