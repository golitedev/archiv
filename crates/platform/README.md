# archiv-platform

Linux filesystem/process and settings boundaries. Directory reads use held descriptors and
openat2; guarded NFS roots are verified against mountinfo. Recovery/restore uses no-follow,
no-cross-mount resolution, private prepared/completed manifests, directory syncing and atomic
no-replace rename. No purge or recursive unlink exists. External programs receive structured
arguments. Settings persist atomically. See ../../docs/file-actions.md for concurrency/durability
limits and ../../docs/safety.md for requirements. Tests use temporary synthetic data only.
