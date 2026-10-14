---
title: primum serve
description: Open the board in the browser.
---

```bash
primum serve
primum serve --port 9000 --no-open
```

Serves every listed project as a tab at `http://127.0.0.1:8777/` and opens
the browser. It listens on 127.0.0.1 only. With a remote set, it syncs each
project every minute and soon after each change.

| Option | Effect |
|---|---|
| `--port N` | The port; default 8777 |
| `--no-open` | Do not open a browser |

See [the board](/reference/board/).
