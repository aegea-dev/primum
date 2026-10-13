---
title: "Desktop window around the board"
status: open
impact: 6
urgency: 2
risk: 2
effort: 6
needs: []
source: "primum"
---

## Why

The board is a page in the browser today. A thin native window around the same pages (wry and tao, not a Tauri app) would give primum a dock icon and its own window, without a second UI to maintain.

## Shape

- `primum app` opens the window on the same axum server, bound to a free port.
- The pages stay exactly as served; no build step is added.
- Packaged per platform only if the window proves worth it.
