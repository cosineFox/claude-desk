#!/usr/bin/env bash
# Install claude-desk for the current user. Never uses sudo: it prints the package command for you to run.
set -eu
here=$(dirname "$(readlink -f "$0")")
chmod +x "$here/desk"
dest=${DESK_BIN_DIR:-$HOME/.local/bin}/desk
mkdir -p "$(dirname "$dest")"
if [ -e "$dest" ] || [ -L "$dest" ]; then
  if [ "$(readlink -f "$dest")" = "$here/desk" ]; then echo "already linked: $dest"
  else echo "refusing to replace existing $dest (-> $(readlink -f "$dest")); set DESK_BIN_DIR to install elsewhere" >&2; exit 1; fi
else ln -s "$here/desk" "$dest"; echo "linked $dest -> $here/desk"; fi
case ":$PATH:" in *":$(dirname "$dest"):"*) ;; *) echo "note: $(dirname "$dest") is not on your PATH" ;; esac

# suggest packages for missing tools
. /etc/os-release 2>/dev/null || true
case "${ID:-}${ID_LIKE:-}" in
  *arch*)   cmd="sudo pacman -S labwc wlr-randr wtype grim chromium foot wayvnc tigervnc nodejs npm  # wlrctl: AUR (paru -S wlrctl)" ;;
  *debian*|*ubuntu*) cmd="sudo apt install labwc wlr-randr wtype grim chromium foot wayvnc tigervnc-viewer nodejs npm wlrctl dbus-daemon" ;;
  *fedora*) cmd="sudo dnf install labwc wlr-randr wtype grim chromium foot wayvnc tigervnc nodejs npm  # wlrctl: build from source (git.sr.ht/~brocellous/wlrctl)" ;;
  *)        cmd="install: labwc wlr-randr wtype grim wlrctl chromium dbus (optional: foot wayvnc a-vnc-viewer nodejs)" ;;
esac
echo; echo "Checking tools:"
if "$dest" doctor; then echo; echo "All required tools found."; else echo; echo "Install the missing ones (package names may differ):"; echo "  $cmd"; fi
if command -v cargo >/dev/null; then
  echo "building desk manager (cargo)..."
  if (cd "$here/manager" && cargo build --release -q 2>&1 | tail -3); then
    (cd "$here/manager/target" && rm -rf debug release/deps release/build release/incremental release/.fingerprint release/examples release/*.d)
    echo "built manager/target/release/desk-manager"
  else echo "manager build failed (optional)"; fi
else echo "optional desk manager needs cargo (rust) to build; skipping"; fi
echo; echo "Next: desk start && desk chromium https://example.com && desk shot"
echo "Claude Code MCP (optional): claude mcp add playwright -s user -- $dest mcp"
