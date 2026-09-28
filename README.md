# Argos

A native desktop monitor, written in Rust, for AI agents running as CLIs.

When you have Claude Code, Codex, Gemini and Antigravity open at once across
several Warp tabs and several projects, you lose track of who is doing what.
Argos answers one question, at a glance:

> **which agent, from which company, is working on which branch — and in what
> state.**

And it takes you to that terminal in one click.

![Argos](docs/capturas/argos.png)

## What it does

- **Four platforms.** Claude Code, Codex, Gemini CLI and Antigravity (`agy`),
  each with its own probe, reading the logs every CLI leaves on disk.
- **Four states**, carrying a symbol as well as a colour: *waiting* ◆,
  *working* ▶, *unknown* ?, *finished* ✓. The symbol is not optional — an
  indicator that depends on colour alone is not an indicator.
- **Git view**: history running left to right, branches as lanes, and the
  agents currently working on a branch hanging from its tip.
- **Agent view**: the hierarchy of sessions and subagents.
- **Jump to the terminal**: clicking an agent opens its Warp pane.
- **Attribution**: click a commit to see which conversations mention it, who
  signed as co-author, and the prompt that led to it — the *why*, which git
  does not record.
- **Time filter** (today / 7 days / 30 days / all) and state filter.

## What Argos does *not* do

It does not control the agents: it will not reply for you, pause them or kill
them. It watches, and it takes you there. Control is a later phase, on purpose.

It does not guess. When the correlation between a process and a session is
doubtful, it says so: every inference carries an explicit confidence level,
because none of these platforms keeps its session file open and the correlation
is necessarily heuristic.

## How it works

A background thread polls every 3 seconds and publishes a snapshot; the UI
never blocks on disk. Probes filter by project *before* parsing — without that,
a full scan of the machine cost 1.65 s and froze the window.

SQLite is a **derived index**, not the source of truth: if the schema changes,
it is rebuilt from the logs. The only table that cannot be reconstructed is the
list of watched projects.

```
crates/argos-core   the core, no UI: probes, correlation, state, git
crates/argos-gui    eframe/egui: the tree, the nodes, the theme
```

## Requirements

- macOS (it uses `warp://` and BSD `ps`)
- Rust stable, edition 2024
- [Warp](https://www.warp.dev/) for the jump-to-session feature

## Build and run

```bash
cargo run --release -p argos-gui
```

## Customising

- **Platform logos.** These are third-party trademarks and do not ship with the
  app. Drop PNGs into `~/.argos/logos/` (`claude.png`, `codex.png`,
  `gemini.png`, `agy.png`); without them, Argos falls back to an initial.
- **Mascot.** Drop a sprite atlas into `~/.argos/mascota/` along with its
  `atlas.json` and it will animate each agent's state.

## Design

`docs/diseno.md` (in Spanish) covers the mark, the palette, and why each colour
has exactly one job. The short version: copper is **identity only** and the
interactive chrome is achromatic bone, because amber and green already mean
*waiting* and *working*, and an accent that reads as a state is a misreading
waiting to happen. Tests hold that rule in place.

## Privacy

Argos **only reads**. It sends nothing anywhere: there is no networking in the
core.

That said, something worth knowing that was already true before you installed
it: the session logs these CLIs write are unencrypted plain text in your home
directory, and they can contain whatever you pasted into a conversation or
whatever an agent printed with `cat .env`. Argos reads them; it does not copy
or expose them.

## Licence

[MIT](LICENSE).

The **fonts are separate**: IBM Plex Sans, IBM Plex Mono and Instrument Serif
are under the [SIL Open Font License](https://openfontlicense.org/), and that
licence travels next to the files in `assets/fonts/`. MIT covers the code, not
them.

The **platform logos** (Anthropic, OpenAI, Google) are not in this repository —
they are third-party trademarks. If you want them, you supply them in
`~/.argos/logos/`.

## Status

Under active development. It works and gets daily use, but the surface changes.

One honest warning: the jump-to-terminal feature depends on `WARP_FOCUS_URL`
and the `warp://session/<uuid>` scheme, which are an **observed** interface of
a closed product — neither documented nor stable. Warp can change it in any
release. It is isolated in `jump.rs` and the process probe so that when that
happens, there is one place to fix.

---

Source comments, commit messages and design docs are in Spanish.
