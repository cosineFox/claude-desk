# UNTESTED. For macOS/Windows (via Docker Desktop, Colima or OrbStack) and anywhere without a Linux desktop.
# Debian trixie ships labwc >= 0.7, wlrctl, wtype, grim, wlr-randr, foot, wayvnc and a non-snap chromium.
FROM debian:trixie-slim
RUN apt-get update && apt-get install -y --no-install-recommends \
      labwc wlr-randr wtype grim wlrctl foot wayvnc chromium \
      dbus dbus-bin procps curl util-linux ca-certificates nodejs fonts-dejavu-core \
    && rm -rf /var/lib/apt/lists/*
RUN useradd -m -u 1000 -s /bin/bash desk
COPY --chown=desk:desk . /opt/claude-desk
RUN ln -s /opt/claude-desk/desk /usr/local/bin/desk
USER desk
ENV XDG_RUNTIME_DIR=/run/user/1000 WLR_RENDERER=pixman
# Chromium's sandbox needs user namespaces; if the container forbids them, run with --security-opt seccomp=unconfined or set DESK_CHROMIUM to a wrapper that adds --no-sandbox.
CMD ["sleep", "infinity"]
