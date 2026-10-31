# archiv-platform

Read-only Linux directory adapter. Uses root directory descriptors, openat2 BENEATH/NO_SYMLINKS
and descriptor-backed enumeration. Emits up to 128 entries per batch and 50,000 per listing;
links are listed without traversal. There is no file mutation API.

Read the [architecture](../../docs/architecture.md), [safety design](../../docs/safety.md),
[development guide](../../docs/development.md) and [instructions](../../AGENTS.md) before changes.
