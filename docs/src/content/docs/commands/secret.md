---
title: primum secret
description: Secrets for lifecycles, kept in the OS secret store.
---

```bash
primum secret set API_KEY       # asks for the value (not shown as typed)
echo "$TOKEN" | primum secret set API_KEY   # or reads it from stdin
primum secret list              # the names, never the values
primum secret unset API_KEY
```

A lifecycle writes `${env.API_KEY}` where a secret goes (an HTTP header, an
actionable's `environment`). When the [daemon](/reference/daemon/) starts
the action's worker, it fills the value in from the OS secret store, else
from its own environment. The value reaches only the worker: never the
journal, the events, a log or a file.

| Platform | Store |
|---|---|
| macOS | the Keychain |
| Windows | the Credential Manager |
| Linux | the Secret Service (GNOME Keyring, KWallet) |

Secrets are kept under the service `primum`, one per name, for the person
running primum; a name is like an environment variable's (`API_KEY`,
`smtp_password`). A value is never taken from the command line, which other
users of the machine can see.

`PRIMUM_NO_SECRET_STORE` (any value) keeps primum away from the store:
secrets then come from the environment only, and `set`, `unset` and `list`
say the store is off. On a machine without a store (a Linux server without
the Secret Service), the daemon simply uses its environment.
