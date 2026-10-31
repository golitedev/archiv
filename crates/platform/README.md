# archiv-platform

Read-only Linux directory adapter. Uses root directory descriptors, openat2 BENEATH/NO_SYMLINKS
and descriptor-backed enumeration. Emits up to 128 entries per batch and 50,000 per listing;
links are listed without traversal. NFS sources verify their expected mount/export against a held root descriptor. Atomic, bounded
application-settings IO is separate from browsing; no library-content mutation API exists.

Read the [architecture](../../docs/architecture.md), [safety design](../../docs/safety.md),
[development guide](../../docs/development.md) and [instructions](../../AGENTS.md) before changes.
