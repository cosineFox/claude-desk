# Setting up claude-desk (instructions for an AI agent)

You have access to this computer. Set up claude-desk end to end yourself, verify it, and report. The desk is a private nested
desktop: nothing below moves the user's pointer, steals focus, or touches their real windows. Keep it that way.

## Rules
- Installing system packages needs the user's go-ahead. Tell them once, in one message, what you will install and why; then do it with their package manager (their sudo / permission prompt is the approval). If you can't run sudo, give them the exact command and wait.
- Don't log in to any account in the desk's browser. Don't open ports beyond 127.0.0.1.
- Don't replace an existing `desk` command on the machine. If `install.sh` refuses, stop and ask.

## Steps
0. **Choose where things go.** Nothing is fixed; follow the user's own layout.
   - *Source folder:* where they keep tools/projects (look for an existing `~/tools`, `~/src`, `~/.local/share`, or what their notes/CLAUDE.md say). If you can't tell, ask once and offer your best guess. Clone (or move) the repo there, and keep it: `desk` runs from that folder.
   - *Command link:* `install.sh` links `desk` into `~/.local/bin`. Set `DESK_BIN_DIR` for another PATH directory.
   - *State and browser profile:* default `~/.local/state/claude-desk` and `~/.local/share/claude-desk-chromium`; override with `DESK_STATE` / `DESK_PROFILE` if the user wants them elsewhere (add to their shell profile only if they agree).
1. **Platform check:** Linux with systemd-style user session (`echo $XDG_RUNTIME_DIR` is set). Note the distro from `/etc/os-release`.
2. **Install the tool:** from this folder, run `./install.sh`. It links `desk` into `~/.local/bin` and runs `desk doctor`.
3. **Install what's missing:** `desk doctor` lists MISSING tools. Install them with the distro's package manager (`install.sh` prints a suggested command). `wlrctl` is the hard one: on Arch it's in the AUR; on Fedora build from source (git.sr.ht/~brocellous/wlrctl); on Debian/Ubuntu try apt. Chromium: on Ubuntu `apt install chromium` is a snap that can't use a profile under `~/.local`, so get a non-snap Chromium or Chrome and set `DESK_CHROMIUM`. Re-run `desk doctor` until it passes.
   - `labwc` must be 0.7 or newer (check with `labwc --version`). If the distro's package is older or missing, tell the user and build labwc from source (github.com/labwc/labwc) instead of continuing.
4. **Verify:** use a throwaway state so you never disturb a desk already running:
   `export DESK_STATE=$(mktemp -d) DESK_CDP_PORT=9333 DESK_VNC_PORT=5911`
   - `desk start` prints `desk up: ...`
   - `desk run foot` (any app), wait about a second, `desk windows` lists it
   - `desk shot` prints a PNG path; look at the image and confirm it shows the app
   - `desk chromium https://example.com`, then `desk shot` shows the page; run it a second time with another URL: it should open a tab, not fail
   - `desk stop`, then `unset DESK_STATE DESK_CDP_PORT DESK_VNC_PORT`
   Logs: `$DESK_STATE/log`, `$DESK_STATE/apps.log`. If something fails, read the log, fix the cause, retry. Don't report success you didn't see.
5. **Optional, ask first:** give the user's agent browser tools with `claude mcp add playwright -s user -- <path printed by install.sh>/desk mcp` (needs `node`/`npx`).
6. **Report** to the user in a few lines: what you installed, what passed and failed (with the error text for failures), how to use it (below), and any step you skipped.

## Using the desk
- Launch apps only with `desk run <cmd>` (clean environment); Chromium only with `desk chromium`.
- Loop: act (`click X Y`, `type`, `key`), then `desk shot` and look. Click coordinates are screenshot pixels.
- Prefer reading page text over CDP (`node <repo>/cdp-eval.mjs <url-substring> "<js>"`) to screenshots.
- The desk is shared by every session on the machine: one active desktop. `desk ws N` only briefly, then `desk ws 1`. Leave other sessions' windows alone.
- Close browser tabs you opened when done (`curl -s 127.0.0.1:9222/json/list`, then `/json/close/<id>`).
- `desk stop` ends the desk and everything in it.
