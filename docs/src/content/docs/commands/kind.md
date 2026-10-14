---
title: primum kind
description: Move an open item to another lane.
---

```bash
primum kind TODO:87 BUG       # prints BUG:12
```

The item takes the next number in the new lane. Its file and documents are
renamed, and every `needs` and `docs` that named the old id is rewritten.
Only open items move; reopen a closed one first.
