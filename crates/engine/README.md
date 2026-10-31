# archiv-engine

Shared headless commands for physical/merged browsing, lazy expansion, bounded recursive search,
application launch, library settings and deny-by-default recovery/restore. All IO uses two bounded
workers; events carry view generations and optional branch identities. Frontend command IDs prevent
replayed effects. Recovery previews are single-use and name explicit physical targets. Children are
bounded/reaped without waiting in rendering. See ../../docs/architecture.md, ../../docs/safety.md
and ../../docs/file-actions.md. No transfer, purge or persistent index implementation exists.
