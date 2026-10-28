# Docker (macOS, Windows, or Linux without a desktop)

**Untested.** Written but never built. If you try it, please open an issue with what worked or broke.

macOS and Windows have no Wayland, so claude-desk runs inside a Linux container there. The agent drives it with `docker exec`.

    ./docker/run.sh                                   # build image, start container "claude-desk"
    docker/desk-docker start
    docker/desk-docker chromium https://example.com
    docker/desk-docker shot /out/a.png                # appears in ./out/ on your machine

- Everything runs inside the container, so the CDP and VNC ports stay inside it. Read page text with `docker exec -u desk claude-desk node /opt/claude-desk/cdp-eval.mjs <url-substring> "<js>"`.
- The Chromium profile lives in the `claude-desk-profile` volume.
- `docker rm -f claude-desk` removes the container; `docker volume rm claude-desk-profile` removes the profile.
- On Apple Silicon the image is arm64. Nothing here needs a GPU (`WLR_RENDERER=pixman`).
- Known unknowns: Chromium sandbox inside containers, headless labwc in a container, whether `/run/user/1000` as tmpfs is enough for D-Bus.
