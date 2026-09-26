# End-to-end tests

Drives the real InfinaBox desktop app, not a mock: the built Tauri binary is
launched by `tauri-driver`, and the tests talk to its webview over
WebDriver. Godot, git, and the file system are all real, and the tests check
results on disk (with `git`) as well as in the UI.

Linux only for now, because `tauri-driver` has no macOS support; on Linux it
wraps WebKitGTK's `WebKitWebDriver`.

## What it covers

The Phase A exit criterion (`docs/superpowers/plans/2026-09-25-phase-a-foundations.md`).
Steps 1–3 always run. After them, the core scenario goes one of two ways:
with `--real-ai`, real chat turns through the user's own `claude` CLI
(the full exit criterion, see "With the real AI" below); without it, the
AI's edit and snapshot are stood in for (steps 4–5c here).

| Step | Checks |
|---|---|
| 1 | App starts on Home; `claude` and `git` show as installed |
| 2 | New Project (through the real native folder dialog) creates `project.godot`, `.ibproject/.ibx`, the addon, and exactly one snapshot, "New project" (checked with `git log`); the app lands on Studio; no panel reorder grip covers a header control |
| 3 | Play panel shows Godot installed; Play → running, output shows `[infinabox] ready 1`, a Godot window exists (screenshot) and, when the screen has room beside the app, doesn't overlap it; Stop → stopped and the process is gone |
| 4 | A script error is written to disk and snapshotted through the app's `snapshot_create` command (as an AI turn would); Play → the error shows as one row with `res://player.gd, line N` (Godot's follow-up messages grouped under it); "Ask AI to fix" puts the request in the chat, and its label says truthfully whether it was sent or only added to the chat box |
| 5a | History "Undo last change" restores the file on disk, the list updates, the running game restarts (new process) without errors |
| 5b | History "Go back" (through its confirm dialog) puts the broken version back, and the game restarts with the error again |
| 5c | Without the null sound device (see Sound below), Godot's real engine error (ALSA, no file in the project) shows without "Ask AI to fix", while the script error keeps it; on a machine with a sound card only the script error is checked |
| 6a–c | Fresh app data and no `INFINABOX_GODOT`: the Play panel offers "Install Godot"; installing shows real byte progress and ends installed; the managed Godot runs the game |

Scenario 6 downloads the real Godot release (~75 MB) from GitHub.

### With the real AI (`--real-ai`)

| Step | Checks |
|---|---|
| 4 | With the game running, the chat gets "make the background dark blue and add a label that says Hello"; the turn finishes (with reply text, no error card, up to 5 minutes); History lists a new snapshot titled from the message; the snapshot's commit carries the chat file and nothing under `.ibproject/chat/` is left uncommitted; the change on disk adds a `Label` saying Hello and a colour; the game restarts (new process id) — screenshot of the game window with its average colour noted |
| 5 | "Undo last change": the project (chat aside) matches the state before the AI's change, the working tree is clean, the chat still holds the request, the game restarts without errors — screenshot |
| 6 | A script error made by hand shows as a `res://player.gd, line N` row; "Ask AI to fix" is **sent** (not left as a draft); the AI's turn finishes; the game restarts and no error about the game's own files remains |
| 5b | "Go back" (through the confirm dialog) to the AI's snapshot restores it exactly (chat aside) and the game restarts |
| mcp | The saved chat shows the AI calling `mcp__infinabox__*` tools, including at least one game tool (`run_game`, `get_game_errors`, …) that succeeded — which only works if `<app> --mcp-server` started and reached the app's bridge |
| stop | While a turn runs, History's "Undo last change" and every "Go back" are disabled with "Wait for the AI to finish" shown; pressing Stop keeps the composer on "Stopping…" until the backend's `agent-turn-finished`, and a message sent straight after is taken (not refused as "still working") and answered |

The tools each turn used are saved as `*-ai-turn-tools.txt` /
`*-fix-turn-tools.txt`, the diffs as `*-ai-change-diff.txt` /
`*-fix-diff.txt`, and the whole saved chat as `*-chat-<thread>.jsonl.txt`.

## Prerequisites

- An X display. On a headless machine: `Xvfb :99 -screen 0 2560x1440x24 &` and `export DISPLAY=:99`.
  The app window is 1280x800, so a screen under ~1810px wide has no room
  for the game beside it, and step 3 can only note (not check) placement.
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

Flags: `--scenario core|install|all`, `--artifacts <dir>`, `--real-ai` (or
`E2E_REAL_AI=1`: run steps 4–6 with real AI turns), `--keep` (keep this
run's temp folder — app homes and projects — instead of deleting it).

`--real-ai` needs a `claude` on `PATH` that can answer from the app's
environment, and it spends real usage (three turns, a few minutes). The app
is launched with a clean environment: `PATH` and a few session basics
(`SHELL`, `USER`, `LANG`, …), the temp `HOME`/`XDG_*` dirs, and only
`ANTHROPIC_BASE_URL`, `HTTPS_PROXY`, `HTTP_PROXY`, `NO_PROXY` passed through
(plus `SSL_CERT_FILE`/`NODE_EXTRA_CA_CERTS` pointing at
`/root/.ccr/ca-bundle.crt` when that file exists, for the cloud container's
proxy). Nothing named `CLAUDE_*`/`CCR_*` reaches it, so the turn behaves
like a normal install rather than like whatever session runs the harness.
Because `HOME` is the temp one, a `claude` whose login lives in your real
home directory won't find it; that's untested on a developer machine
(`tauri-driver` has no macOS support anyway), where the app should simply
be run by hand with your own logged-in `claude`.

Output goes to `e2e/artifacts/<timestamp>/` (git-ignored): a screenshot after
every step plus extra ones at key moments (the game window, the error list,
the install progress), the Play panel's game log after each game step, the
app's own stdout/stderr (`<scenario>.app.log`), and `report.json` with each
step's result and notes. The exit code is 0 only when every step passed.

## How it works, and the non-obvious parts

- **Isolation.** Each run makes one temp folder of its own
  (`ibx-e2e-run-*` under the system temp dir) and puts everything it
  creates there, removed at the end unless `--keep`; so two runs at once
  never delete each other's files. Each app launch gets its own `HOME` and
  `XDG_*` dirs inside it, so app data (where a managed Godot goes), the webview's
  `localStorage` (recent projects, layouts) and GTK settings start empty
  and nothing persists. `PATH` is kept, so tool detection sees the
  machine's real `claude` and `git`. Projects are created in the run's temp
  folder, never inside this repository (core refuses a project inside
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
- **Standing in for the AI** (without `--real-ai`). Step 4 edits a script
  on disk and then calls the app's own `snapshot_create` command through
  the webview's IPC bridge (`window.__TAURI_INTERNALS__.invoke`), the same
  call the frontend makes after a turn.
- **Waiting for an AI turn** (`--real-ai`). The chat panel exposes
  `data-busy` (true from sending until the backend's `agent-turn-finished`,
  which comes after the turn's snapshot) and `data-ready`; transcript rows
  carry `data-testid="chat-item"` and `data-kind` (user, assistant, work,
  error). What the AI actually did is read from the saved chat
  (`.ibproject/chat/*.jsonl`) and `git`, not from its reply text.
