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
command -v cargo >/dev/null && echo "optional desk manager: (cd $here/manager && cargo build --release)" || echo "optional desk manager needs cargo (rust) to build"
echo; echo "Next: desk start && desk chromium https://example.com && desk shot"
echo "Claude Code MCP (optional): claude mcp add playwright -s user -- $dest mcp"
