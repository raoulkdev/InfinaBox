# InfinaBox product spec: the AI game studio

**Status:** product direction, approved at the decision level (including the follow-up decisions in §2), pending review of this document
**Date:** 2026-09-25
**Supersedes:**
- The product positioning in `CLAUDE.md` ("a workspace cockpit around the user's own terminal-based coding agent … explicitly not a chat-panel product").
- `docs/superpowers/specs/2026-09-12-workspace-redesign-design.md` (Agent panel + Project window). That spec already describes a superseded UI; this one changes *who the product is for*, not just its layout.
- The "no chat panel" rule in the `agent_auth_model` project memory. The part of that decision that still holds (InfinaBox does not sell or broker AI access) is kept, see "Bring your own AI".

**Not superseded:** "Every number or status shown is real. No fabricated activity, no invented stats." This carries forward unchanged and matters *more* for a novice audience.

---

## 1. Summary

InfinaBox becomes **an AI game studio on your desktop**: one place where someone with a game idea and little or no development knowledge can design, build, create assets for, playtest, ship, and market a real game, with an AI team that does the heavy lifting and explains every step.

The output is always a **real Godot project the user owns**: not a locked-in web toy, not a proprietary format. Users can outgrow InfinaBox and open their project in Godot directly at any point.

The AI is **the user's own** (their Claude/ChatGPT subscription or API key, or a local model). InfinaBox makes connecting it painless but never resells it.

## 2. Decisions this spec is built on

| Decision | Choice | Consequence |
|---|---|---|
| How users get AI | Bring your own AI, made easy | Guided connect flow, pluggable agent runtimes, no InfinaBox billing or model hosting |
| Engine | Godot (4.x) only | Deep integration instead of lowest-common-denominator; InfinaBox manages the Godot install |
| Scope | 2D **and** 3D | Templates, previews, and asset pipeline must handle both; 3D gets sequenced carefully (see §12) |
| Chat panel | Yes: it is the primary interface | The terminal moves to Advanced mode; it is kept, not deleted |
| Chat history | Committed to the project's repo | Conversations are part of the project record; secrets are redacted before commit (§9) |
| GitHub backup | Later, not in the first phases | History is local git only until Phase D or after |
| Godot addon | Installed automatically | Every InfinaBox project gets the addon; removable, but on by default (§10.3) |
| Generation providers | Cloudflare Workers AI (images), Fish Audio (voice), ElevenLabs (sound effects and music) | BYO keys via the connect flow; see §7.3 for what each covers |
| InfinaBox pricing | Subscription with a free tier, billed through Stripe | Pays for the app, never for AI usage; tiers and prices in §15 |
| Existing projects | None need migration | No legacy `.ibproject/` import path is built |
| Naming | The project knowledge model is called **Context** | Used in the UI, the sidebar, and the on-disk folder (`.ibproject/context/`) |

## 3. The problem

Making a game today requires someone to simultaneously be a designer, programmer, artist, sound designer, producer, QA tester, and marketer, and to know which tools do each job. AI tools help with pieces (code completion, image generation) but:

- **They assume expertise.** A coding agent in a terminal is powerful only if you know what to ask for, how to read errors, and how a game project is structured.
- **They have no memory of the game.** Each tool sees a slice. Your code assistant has never read your design doc; your image generator doesn't know your art style.
- **They stop at "code".** Nothing guides you from idea → fun prototype → finished game → store page → players.
- **"Generate a game" products produce toys.** Browser-only, closed formats, hard ceilings, nothing you can grow with.

## 4. Who it's for

### Primary: "The Idea Person" (novice)
- Has a game idea and enthusiasm, has maybe tried a tutorial and bounced off.
- Can describe what they want in plain language; cannot write code or navigate an engine editor.
- Needs: a clear path, results fast, confidence they won't break things, and to learn as they go.

### Secondary: "The Solo Dev" (some experience)
- Can code a bit or has shipped a jam game; drowning in everything that isn't code (art, marketing, release, scope).
- Needs: a producer, an asset pipeline, a launch checklist, and an agent that knows their game.
- Uses Advanced mode (terminal, code editor, history) freely.

### Not the target (for now)
- Studios with teams (no multiplayer collaboration in v1).
- Developers committed to Unity/Unreal.

## 5. Product principles

1. **Real, not fabricated.** Every number, status, and progress indicator is derived from the actual project: files, scenes, Context, git history, real run results. If we can't measure it, we don't show it.
2. **Plain language first, depth on demand.** Default views never require knowing what a node, signal, or commit is. Every surface has a way down into the real thing (Advanced mode, "show me the code", open in Godot).
3. **Nothing is ever lost.** Every AI change is a snapshot the user can view and undo. Fear of breaking things is the #1 novice blocker; we remove it completely.
4. **The user is the director.** The AI proposes, the user approves. No large change happens without a readable plan first (with an opt-in "just do it" mode for small changes).
5. **Teach, don't hide.** Every change comes with a short explanation of what was done and why. Users should get *more* capable over time, not more dependent.
6. **You own everything.** Standard Godot project, standard files, standard git underneath. No lock-in; no cloud requirement beyond whatever AI provider the user picks.
7. **One game, one model.** Everything InfinaBox knows about a game lives in one linked structure (Context), readable by both the user and the AI.

## 6. The experience

### 6.1 The core loop

> **Describe → Plan → Approve → Build → Play → Feedback → repeat**

1. The user describes what they want ("the player should be able to double jump").
2. The Director agent proposes a short plain-language plan, citing Context entries it will touch.
3. The user approves (or edits the plan in conversation).
4. The agent builds: edits scripts/scenes, imports assets, updates Context.
5. The game relaunches automatically in the Play panel.
6. Runtime errors are captured and fed back to the agent automatically; the user just sees "Something broke — fixing it" with a way to watch.
7. A snapshot is recorded with a human-readable title ("Added double jump").

Every other feature exists to make this loop smarter (better context), safer (snapshots), or reach further (assets, playtests, launch).

### 6.2 First run: "idea to playable in 30 minutes"

This is the moment the product lives or dies by.

1. **Connect your AI** (see §8). One screen, detects what's already installed, guided setup for what isn't.
2. **Set up Godot**: InfinaBox downloads and manages a pinned Godot version automatically. No user decision.
3. **Tell us about your game**: a short conversational interview (≈5–8 questions): the idea, genre, 2D or 3D, how it should *feel*, reference games, target session length.
4. **Context drafted**: the interview produces a starter Context (concept, pillars, core mechanics, a first milestone) the user can review and tweak.
5. **Template chosen**: the Director picks the closest starter template (§7.4) and explains why.
6. **First build**: the agent customizes the template toward the concept (player, core mechanic, placeholder art in the chosen style).
7. **Play**: the game runs. The user has something that moves, on screen, that is *theirs*.

Exit criterion for the product: a novice with no prior knowledge reaches step 7 in under 30 minutes, on their own, in moderated testing.

### 6.3 Ongoing: the Producer
After the first run, Home shows a **journey** from Idea → Prototype → Vertical Slice → Alpha → Beta → Launch. Each stage has a checklist of real, checkable criteria (e.g. "core loop playable start to finish", "3 external playtests collected", "store page drafted"). The Producer role reads the project state and suggests the next best step. It never invents progress; a checklist item is checked only by a real signal or by the user.

## 7. Feature pillars

### 7.1 Studio (chat + live game)
The primary screen: **conversation on one side, the running game on the other.**

- **Director chat**: one conversation per project (with history and named threads, e.g. "Boss fight", "Main menu"). Renders agent output natively: plans as approvable checklists, file changes as collapsible summaries, errors as friendly cards.
- **Roles**: the Director delegates to specialist roles, shown in the chat as who's working: Designer, Programmer, Artist, Sound, QA, Producer, Marketer. Roles are system prompts + tool sets over the same agent runtime, not separate products or separate billing.
- **Plan / approve**: any multi-step change shows a plan first. Per-project setting for auto-approving small changes.
- **Explain toggle**: every change gets a one-paragraph "what I did and why". "Teach me" mode expands this into a short lesson with links into the actual code/scene.
- **Play panel**: Run / Stop / Restart, window-embedded where feasible, otherwise a managed separate window (see §10.3). Shows real FPS and errors from the running game.
- **Point-and-say**: take a screenshot of the running game and annotate it ("make this enemy bigger"); the screenshot is attached to the conversation.
- **Attach chips**: the user can pin Context cards, assets, or files into the conversation ("@Boss", "@level-2").

### 7.2 Context (the one model of the game)
Replaces the current 13 folder-per-discipline sidebar sections with **typed, linked cards**.

Naming: in the UI, **Context** (capitalized) always means this model of the game. Avoid using the lowercase word for other things in UI copy (say "attached to the chat", not "added to context") so users never confuse it with an AI model's context window.


| Card type | Holds | Links to |
|---|---|---|
| Concept | pitch, pillars, target feel, references | everything |
| Mechanic | description, tuning values, status | scripts/scenes implementing it, tasks |
| Character / Enemy | description, behavior, stats | scenes, assets, mechanics |
| Level / Area | layout notes, flow, difficulty | scenes, assets, story beats |
| Story / Dialogue | beats and branching flow (graph view) | characters, levels |
| Asset | file, license/source, style notes | whatever uses it |
| Style Guide | art direction, palette, audio mood | all asset generation |
| Task / Bug | what, why, status | cards + snapshots that resolved it |
| Playtest | session notes, feedback | tasks created from it |

- Cards are Markdown files with YAML front-matter stored in the project (§9), so they are readable, diffable, and survive leaving InfinaBox.
- **The AI reads and writes Context.** When it builds a mechanic, it links the mechanic card to the code; when code changes, it updates the card's status. "Make the boss harder" resolves to a specific card, specific scripts, and specific tuning values.
- **Views, not sections**: the former disciplines (Art, Audio, UI/UX, Business, Marketing, Community, Release, QA) become filtered views and board layouts over this one model.
- **Graph views** (reusing `GraphEditor`) for story/dialogue flows, level connections, and state machines.

### 7.3 Assets
- **Browse**: built-in search across free, clearly-licensed libraries (e.g. Kenney, Quaternius, Poly Haven, OpenGameArt): 2D sprites, tilesets, 3D models (glTF), textures, SFX, music.
- **Generate (bring your own)**: optional generation through the user's own provider accounts, connected with the same guided flow as §8 and stored in the OS keychain. Generation prompts automatically include the Style Guide card.
  - **Images: Cloudflare Workers AI.** The user connects their Cloudflare account (account ID + API token). Used for sprites, concept art, textures, UI elements, and store/capsule art drafts. Output goes through a post-processing step (background removal, resizing, palette snapping for pixel art) before import. Model choice is configurable from what Workers AI offers at the time.
  - **Voice: Fish Audio.** The user connects a Fish Audio API key. Fish Audio is text-to-speech and voice, so it covers character voices, dialogue lines, narration, and announcer barks; each Character card can hold a chosen voice so lines stay consistent.
  - **Sound effects and music: ElevenLabs.** The user connects an ElevenLabs API key. Used for sound effects (jumps, hits, UI clicks, ambience) and music (menu themes, level loops). Prompts include the Style Guide's audio mood; music requests ask for loopable tracks where the API supports it, and InfinaBox checks and trims loop points before import. Library assets remain the free, no-key alternative.
- **Preview**: images, sprite sheets (with animation playback), tilesets, audio (waveform + playback), 3D models (orbit viewer).
- **Import**: one click imports into the Godot project in the right folder with correct import settings (pixel-art filtering, etc.) and creates/links an Asset card.
- **License tracking**: every asset records its source and license; a generated credits screen and a "license check" before launch.
- **Health**: real checks such as unused assets, missing references, and oversized textures.

### 7.4 Templates
Starter projects, each a clean, well-commented Godot project with a matching starter Context:

- **2D**: platformer, top-down adventure, twin-stick shooter, puzzle, visual novel, roguelite.
- **3D**: third-person explorer, first-person, top-down/isometric action, kart/racing sandbox.

Templates are the biggest quality lever for AI output: the agent extends known-good structure instead of inventing architecture. They are versioned and maintained as first-class product surface, not samples.

### 7.5 History (snapshots, never "git")
- Every approved AI change and every manual save point becomes a **snapshot** with a plain title and the chat turn that produced it.
- Visual timeline with screenshots of the game at each point.
- "Go back to before X" restores safely (implemented as new commits, never destructive rewrites).
- Git is the storage underneath and is fully visible in Advanced mode.
- GitHub (remote) backup is deliberately **later**, not part of Phases A–C. Until then, history lives in the local repo only, and the UI should say so plainly.

### 7.6 Playtest
- **Share a build**: one-click web export to a private itch.io page (via `butler`), or a downloadable desktop build.
- **Collect feedback**: a simple feedback form link; responses come back as Playtest cards and the QA role proposes tasks from them.
- **Self-playtest notes**: while playing in the Play panel, a hotkey drops a note plus screenshot into Context.
- **Automated smoke tests**: the QA role can write and run headless Godot tests (the game boots, scenes load, no script errors) before each snapshot.

### 7.7 Launch & Grow
- **Release builds**: export presets for Windows / macOS / Linux / Web managed by InfinaBox (export templates downloaded automatically).
- **Publish**: itch.io via `butler`; Steam via `steamcmd` with a guided Steamworks checklist (Steam requires the user's own partner account; we guide, not automate, the account parts).
- **Launch kit**: store page copy, capsule-art size guides, screenshot capture from the running game, trailer clip capture, press kit page, credits (from license tracking).
- **Devlog**: drafts devlog posts from real snapshot history ("this week: added double jump, 3 new enemies").
- **Community & business**: kept as Context views (docs, plans), with real integrations deferred.

### 7.8 Advanced mode
Everything that exists today is kept and moved here for users who grow into it:
- The real PTY terminal (`TerminalPanel`): the user's own `claude`/`codex` CLI still works exactly as today.
- The CodeMirror code editor and full file browser (`FileBrowser` `codeEditor` mode).
- Raw git history, "Open in Godot editor", project settings.

## 8. Bring your own AI, made easy

InfinaBox never sells, proxies, or meters AI usage. It makes connecting the user's own AI as easy as possible.

### 8.1 Supported connections (in priority order)

| Connection | Who it's for | How it works | Credentials |
|---|---|---|---|
| **Claude Code CLI** (Claude subscription) | Most users; flat subscription cost | InfinaBox drives the user's installed `claude` CLI in headless/streaming mode | Held by the CLI; InfinaBox stores none |
| **Codex CLI** (ChatGPT subscription) | ChatGPT subscribers | Same pattern, driving the user's `codex` CLI non-interactively | Held by the CLI; InfinaBox stores none |
| **API key** (Anthropic / OpenAI) | Pay-as-you-go users | InfinaBox runs the agent loop itself (e.g. via the Claude Agent SDK) | Stored in the OS keychain only, never in project files or plain-text config |
| **Local model** (Ollama / LM Studio) | Privacy/cost-sensitive users | OpenAI-compatible local endpoint | None |

The exact CLI flags and streaming formats must be verified against each CLI's current docs at implementation time; they change.

### 8.2 The connect flow ("made easy")
1. **Detect**: reuse `check_cli_tools` (`src-tauri/src/commands/environment.rs`) to find installed CLIs, extended to check login state where the CLI exposes it.
2. **Recommend**: one recommended option based on what's detected, with a plain comparison ("Already have Claude Pro? Use this. No subscription? Here's what each option costs").
3. **Install for them**: if a CLI is missing, run the official installer inside a visible, managed terminal with a progress UI. The user watches it happen; nothing hidden.
4. **Sign in for them**: launch the CLI's own login flow (browser-based OAuth) from a button; detect completion.
5. **Test**: a one-click "say hello" round-trip proving the connection works before the user proceeds.
6. **Usage awareness**: show real usage signals where the provider exposes them (e.g. rate-limit messages surfaced plainly: "Your Claude plan's limit resets at 3pm"). Never estimated or invented.

### 8.3 The agent runtime abstraction
A single internal interface, `AgentRuntime`, that every connection implements:

- `start_session(project, role, context) → session`
- `send(session, message) → stream of events` (text, plan, tool call, file change, error, done)
- `cancel(session)`

The Studio UI renders the event stream and never cares which provider is underneath. Roles, plans, and tools are defined once in InfinaBox.

### 8.4 Tools given to the agent
Exposed through an **InfinaBox MCP server** (the roadmap's long-standing "real MCP server" item, now load-bearing), so CLI-based runtimes and the API runtime get the same tools:

- Context: read, search, create, update, link cards.
- Godot: run game, stop, read runtime log/errors, run headless tests, take screenshot, list scenes/nodes, validate project.
- Assets: search libraries, import asset, generate (if configured).
- History: create snapshot, list snapshots, diff.
- Project Q&A: the existing `ask_question` / `refresh_project_graph` (grounded answers over git history).

The agent's normal file-editing ability (built into Claude Code / Codex, or provided in the API runtime) handles scripts and scenes.

## 9. Project format

A normal Godot project, with InfinaBox data in its existing `.ibproject/` directory (continuing the `.ibproject/.ibx` marker from `src/lib/project-picker.ts`):

```
my-game/
  project.godot            # standard Godot project
  scenes/ scripts/ assets/ # standard Godot content (template-defined layout)
  .ibproject/
    .ibx                   # project marker + InfinaBox format version
    context/               # Context cards: *.md with YAML front-matter
      concept.md
      mechanics/double-jump.md
      characters/boss.md
      ...
    graphs/                # *.graph.json flows (existing format)
    playtests/
    launch/
    chat/                  # conversation threads (JSONL), committed with the project
  AGENTS.md / CLAUDE.md    # generated agent instructions pointing at Context + MCP tools
```

- Everything is plain text and committed to the project's git repo, **including `chat/`**. Conversations are part of the project's record: a snapshot links to the chat turn that produced it, and the agent can read past decisions.
- Because chat is committed, InfinaBox **redacts secrets before writing chat files** (API keys, tokens, anything matching known credential formats) and warns before the first push to any remote. Users can still delete a thread; the deletion is itself a commit.
- No migration path: there are no existing user projects in the old `.ibproject/docs|business|marketing|community|release` layout, so new projects start directly in this format.

## 10. Godot integration

### 10.1 Managed install
- InfinaBox downloads a pinned Godot 4.x version (standard or .NET-free build) plus export templates into its app data directory, verifies checksums, and upgrades only with user consent.
- Users can point to their own Godot install in Advanced settings.

### 10.2 Project understanding
- Parse `project.godot`, `.tscn`, `.tres` (text formats) to build a scene/node/resource index: which scenes exist, which scripts attach where, and broken `res://` references.
- This feeds Context links, asset health, and what the agent knows about the project.

### 10.3 Running the game
- Launch via the Godot binary with the project path; capture stdout/stderr and structured errors.
- **Phase 1:** separate managed game window, positioned beside InfinaBox.
- **Later:** investigate true embedding (window reparenting per-OS, or a web-export preview for fast iteration). Treat as a research item; do not block the core loop on it.
- A small InfinaBox **Godot addon** is **installed automatically** into every InfinaBox project (under `addons/infinabox/`, enabled in `project.godot`). It provides a debug channel: screenshots, scene tree dumps, and live-tuning values from the running game. It must be inert in exported release builds (debug-only), versioned with InfinaBox, and removable from Advanced settings.

### 10.4 Headless operations
- `--headless` runs for import, validation, smoke tests, and exports.

## 11. Information architecture

Sidebar shrinks from 13 entries to 6:

| Entry | Contents |
|---|---|
| **Home** | Projects, journey roadmap for the open project, AI connection status |
| **Studio** | Director chat + Play panel (default screen of an open project) |
| **Context** | Cards, boards (tasks/bugs), graph views, discipline filters |
| **Assets** | Browse, generate, preview, import, health |
| **Playtest & Launch** | Share builds, feedback, release builds, publish, launch kit, devlog |
| **Advanced** | Terminal, code editor, raw history, open in Godot, settings |

Studio and Advanced's terminal keep the existing "never unmount" treatment from `src/App.tsx` (live sessions and running processes must survive navigation).

## 12. Scope: 2D and 3D

Both are in scope, sequenced by risk:

- **2D is the reliability baseline.** AI-generated 2D gameplay code is markedly more reliable, 2D assets are easier to source/generate, and novices finish 2D games more often. 2D templates ship first and are held to the 30-minute bar.
- **3D follows with narrower templates.** 3D templates are more constrained (fixed camera rigs, provided controllers) so the agent customizes rather than architects. 3D asset flow leans on libraries (glTF) rather than generation at first.
- The Context, chat, history, playtest, and launch features are dimension-agnostic from day one.

## 13. Reuse of today's code

| Existing | Becomes |
|---|---|
| `TerminalPanel` + `commands/terminal.rs` (PTY) | Advanced-mode terminal; also the visible installer/login surface in §8.2 |
| `commands/environment.rs` (`check_cli_tools`) | AI connection detection; extended to Godot, `butler`, `steamcmd` |
| `FileBrowser`, `CodeEditor`, `MarkdownEditor` | Advanced file browsing; card body editing; "show me the code" |
| `GraphEditor` (`.graph.json`) | Story/dialogue/level-flow views in Context |
| `commands/watcher.rs` + `src/lib/fs-watch.ts` | Live refresh when the agent edits files, cards, or scenes |
| `crates/core` git indexer + project graph + `ask.rs` | Snapshot timeline and grounded Q&A tool exposed via the MCP server |
| `crates/core/src/mcp_client.rs` spike | Superseded by a real MCP server crate |
| `ResizablePanelGroup`, motion conventions, `src/lib/*` | Unchanged, used everywhere |
| Discipline `*Section.tsx` files + `NotBuiltYetSection` | Retired in favor of Context views (no data migration needed) |

## 14. Roadmap

Each phase has an exit criterion that must be demonstrated with real users, not asserted.

### Phase A: Foundations
- `AgentRuntime` with the Claude Code CLI runtime first; streaming event model.
- InfinaBox MCP server crate with Context + Godot run/log tools.
- Managed Godot install; run game; capture errors.
- Snapshot-on-change over git.
- **Exit:** from the Studio chat, a user can ask for a change, see it built, watch the game relaunch, and undo it.

### Phase B: The first-run experience
- Connect-your-AI flow (§8.2) for Claude Code and Codex.
- Onboarding interview → starter Context.
- 3 polished 2D templates.
- Plan/approve UI, explanations, auto error-fix loop.
- New 6-entry navigation; old discipline sections retired.
- **Exit:** 5 of 8 novice testers reach a playable, personalized prototype in under 30 minutes unassisted.

### Phase C: Make the whole game
- Full Context (typed cards, links, boards, graph views), roles.
- Assets: library browse, preview (2D + audio + 3D viewer), import, license tracking.
- Producer journey + checklists.
- 2 more 2D templates, first 2 3D templates.
- API-key and local-model runtimes.
- Cloudflare Workers AI image generation, Fish Audio voice generation, and ElevenLabs sound effect and music generation.
- **Exit:** a tester takes a prototype to a complete short game (start screen → gameplay → ending) inside InfinaBox.

### Phase D: Ship it
- Playtest sharing (itch.io via `butler`), feedback → cards.
- GitHub backup (the earliest point it is considered).
- Release exports, itch.io publishing, Steam checklist + `steamcmd` upload.
- Launch kit and devlog drafts.
- Godot addon's full feature set (screenshots, live tuning), headless smoke tests. (The addon itself is installed from Phase A so runtime errors can be captured.)
- **Exit:** a tester publishes a game to itch.io without leaving InfinaBox.

## 15. Business model

InfinaBox is sold as a **subscription with a free tier**. Because users bring their own AI, the subscription pays for the app and InfinaBox's own services, never for model usage. That has to be obvious on the pricing page: "Your AI costs are with your AI provider; InfinaBox never marks them up."

### 15.1 Tiers

Prices are a starting hypothesis to validate with users, not a commitment.

| | **Free** | **Pro** |
|---|---|---|
| Price | $0 | $12/month, or $99/year |
| Projects | 1 active project | Unlimited |
| Studio chat, plan/approve, Play, auto error-fix | Yes | Yes |
| All AI connections (Claude Code, Codex, API key, local) | Yes | Yes |
| Context (cards, boards, graph views) | Yes | Yes |
| Snapshots and undo | Yes | Yes |
| Templates | 3 starter 2D templates | Full library, including 3D |
| Assets | Library browse, preview, import | Plus generation (Cloudflare, Fish Audio, ElevenLabs), asset health checks |
| Playtest | Manual export | One-click itch.io sharing, feedback collection |
| Launch | Manual export for desktop and web | One-click publishing (itch.io, Steam), launch kit, devlog drafts |
| Producer journey and checklists | Basic (stage + checklist) | Full, with next-step suggestions |

Why this split:
- **Free is genuinely useful.** A novice can go from idea to a finished, exported game on Free. That keeps the first-run promise (§6.2) available to everyone and makes Free the main way people find InfinaBox.
- **Pro is for people getting serious.** More than one game, 3D, generated assets, and the shipping/marketing tools are the points where someone is invested and the value is clear.
- **Generation stays BYO on Pro.** Pro unlocks the integrations; the user's own provider accounts still pay for the generations.
- **$12/month** sits below the AI subscription most users will already pay (~$20/month), so the combined cost stays reasonable. The annual price gives roughly two months free.

### 15.2 Trial
- Every new account gets a **14-day Pro trial, no card required**, long enough to try 3D templates, generation, and a playtest share.
- When the trial ends, the account drops to Free automatically. Nothing is deleted.

### 15.3 Downgrades and cancellation
- **You keep your game.** Projects are standard Godot + git + plain text (§9), and always open in Godot directly.
- Dropping to Free with more than one project: the user picks which project stays active; the others open read-only in InfinaBox (still browsable, still exportable manually) until the user upgrades or switches which one is active.
- Pro-only content already in a project (a 3D template, generated assets) stays in the project. Only the Pro *actions* stop.

### 15.4 Billing and licensing with Stripe
InfinaBox has no accounts, payments, or license checks today. The plan uses Stripe for everything payment-related and keeps InfinaBox's own backend as small as possible:

- **Accounts:** email sign-in (magic link) on a small InfinaBox backend. The desktop app signs in through the system browser and returns via a deep link (Tauri deep-link plugin).
- **Checkout:** Stripe Checkout hosted pages for upgrading, opened in the system browser. No card details ever touch the desktop app.
- **Managing billing:** Stripe Customer Portal for plan changes, invoices, payment methods, and cancellation.
- **Subscriptions:** Stripe Billing products and prices for Pro monthly and annual; trials handled by Stripe without collecting a card.
- **Taxes:** Stripe Tax for VAT/sales tax.
- **Source of truth:** Stripe webhooks update the backend's record of each account's plan. The backend issues a **signed entitlement token** (plan + expiry) that the app verifies locally.
- **Offline use:** the app caches the token and keeps Pro features working offline for a grace period (e.g. 7 days past token expiry) before falling back to Free. Free features never require being online, apart from connecting the user's AI.
- **Privacy:** the backend stores only the account email, Stripe customer ID, and plan. No project content, chat, or code is ever sent to InfinaBox servers.

Scheduling: accounts, Stripe integration, and entitlement checks are built alongside Phase B so they are ready before public release. Tier gating is added to each feature as it ships.

## 16. Success measures

InfinaBox has no telemetry today; any measurement must be **opt-in**, and the product must work fully without it. Measured in user studies and opt-in reporting:

- Time from install to first playable prototype.
- % of started projects reaching each journey stage (Prototype, Vertical Slice, Launch).
- Auto-fix success rate (runtime errors resolved without user intervention).
- Undo rate on AI changes (a proxy for AI output quality).
- Games published to itch.io/Steam.

## 17. Risks and mitigations

| Risk | Mitigation |
|---|---|
| BYO AI is friction for novices | The §8.2 guided flow is a Phase B deliverable, tested as hard as gameplay; recommend the path with the least setup |
| Driving third-party CLIs is brittle (flags, output formats, ToS changes) | `AgentRuntime` isolates each; API-key runtime as a fallback; pin and test against known CLI versions |
| AI-generated game code quality, especially 3D | Opinionated templates, headless smoke tests before each snapshot, error auto-fix loop, 2D-first sequencing |
| Scope: "everything about game dev" is enormous | Phase exit criteria gate expansion; Context views let disciplines exist without bespoke tools |
| User's usage limits hit mid-task | Surface provider limit messages plainly, save state, resume cleanly; never lose work |
| Embedding the Godot window is hard cross-platform | Phase 1 uses a managed separate window; embedding is a research item |
| Committed chat history leaks secrets or private text | Redaction before write, warning before first push to a remote, deletable threads |
| Subscription feels unfair on top of paying for AI | Clear "we never touch your AI bill" messaging, a useful Free tier, a 14-day no-card Pro trial, no lock-in of project files |
| Licensing breaks offline or blocks work | Signed entitlement token cached locally with an offline grace period; Free features never need a server |
| Asset licensing mistakes | License recorded per asset, pre-launch license check, generated credits |
| Competition from engine-native AI and web "game generators" | Differentiate on ownership (real Godot project), full lifecycle, and teaching |

## 18. Non-goals (v1)

- Selling, hosting, or proxying AI access (including image, voice, sound, and music generation: always the user's own Cloudflare, Fish Audio, and ElevenLabs accounts).
- Engines other than Godot.
- Real-time multi-user collaboration.
- Mobile/console export (desktop + web only in v1).
- A replacement for the Godot editor: deep manual scene editing stays in Godot ("Open in Godot" is always one click away).
- Live-ops analytics and crash telemetry for shipped games.

## 19. Resolved decisions and remaining open questions

Resolved (2026-09-25):
1. Chat history is committed to the project's repo (§9).
2. GitHub backup comes later, no earlier than Phase D (§7.5, §14).
3. The InfinaBox Godot addon installs automatically (§10.3).
4. Generation providers: Cloudflare Workers AI for images, Fish Audio for voice, ElevenLabs for sound effects and music (§7.3).
5. InfinaBox is priced as a subscription with a free tier (§15.1), a 14-day Pro trial (§15.2), and billing through Stripe (§15.4).
6. No existing user projects need migration (§9).
7. The project knowledge model is named **Context** (§7.2).

Still open:
1. **Pricing validation**: the $12/month and $99/year Pro prices and the Free/Pro split in §15.1 are hypotheses to test with real users before launch.
2. **Backend hosting**: where the small accounts/entitlements backend runs.
