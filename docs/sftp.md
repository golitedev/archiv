# Direct SFTP browsing

Archiv connects directly to an SSH server's SFTP subsystem. No SSHFS/NFS mount, server helper,
KDE service or administrator privileges are needed. The PC running Archiv needs the OpenSSH
`ssh` client, a working SSH key/agent and an already verified server key in `known_hosts`.

## Connection setup

On the graphical PC running Archiv, configure an SSH alias in `~/.ssh/config` if desired:

```sshconfig
Host archive-nas
    HostName nas.example
    User reader
    Port 22
    IdentityFile ~/.ssh/id_ed25519
    IdentitiesOnly yes
```

This is an example, not an installed configuration. Keep private keys outside Archiv's settings.
Use your normal SSH tooling to verify the host fingerprint through a trusted channel and test
`sftp archive-nas`. Encrypted keys should be unlocked in your SSH agent. Archiv uses batch
(noninteractive) authentication and strict host-key checking; it never accepts unknown/changed
host keys, prompts for passwords, or stores credentials. The SSH configuration is trusted local
configuration: aliases, identity files and proxy/jump settings are handled by OpenSSH, whose
configuration can itself invoke external proxy commands. Archiv disables agent/X11 forwarding,
configured port forwarding, local commands and connection multiplexing for its own session.

In **Preferences → Sources**, add a source and enable **Direct SFTP (browse only)**. Enter:

- A source name and absolute **Remote root folder**, such as `/srv/media` (the server's path,
  or its absolute path within an SFTP chroot).
- **SFTP host or SSH alias**, such as `archive-nas`; enter the user separately, not `user@host`.
- An optional username and port. Blank username and port 0 use SSH configuration defaults.

Save the library, close Preferences, then choose the source in the sidebar. Saving/restoring
settings never connects automatically. Connections originate from the machine running Archiv,
not the machine hosting this source repository. This implementation session has not configured
the user's PC or NAS.

Settings remain in version-1 `library.json`, using an optional `sftp` object on a source; `root`
is remote for such a source. Local/NFS source definitions continue to work without that object.
See [the complete example](../examples/archiv.sftp.example.json). NFS and SFTP cannot be enabled
on the same source. Separate hosts may use identical remote roots and retain separate identities.

## Available behavior

Browse folders, expand inline, filter names, explicitly search subfolders, refresh, navigate
Back/Forward and inspect file kind, size, modification time and observed mode. Configured
collections can combine remote and local folders visually while preserving source identities.
The location and Details show an `sftp://` endpoint; these are display labels. Ctrl+L edits the
absolute remote folder within the currently configured remote root, without switching to local
storage. Use Sources settings to change the endpoint/root.

Remote files are listed only: no external applications, file downloads, previews of file contents,
Copy/Paste, Copy path, rename, deletion, uploads or permission changes. File actions fail in the
engine and platform even if invoked directly; remote write policies cannot be enabled. Local
sources keep their existing opt-in file actions. Symbolic links are listed but not traversed.

## Bounds and limits

Two shared engine workers bound simultaneous directory connections. Each directory opens a fresh
OpenSSH session; there is no persistent connection pool. Requests use binary SFTP v3 (not parsed
`ls` output or an interactive SFTP prompt), with one outstanding request, 256 KiB maximum packets,
16 KiB strings, 4,096 entries per server batch, 50,000 entries per directory, 128-entry UI batches
and at most 64 path components. Incompatible or oversized responses fail with incomplete-results
errors. Recursive search retains the engine's existing depth/queue/entry limits. Per-directory
work including connection setup has a 30-second deadline, socket waits check cancellation every
100 ms, and the OpenSSH child is killed/reaped on completion, cancellation or error. Cancellation
may retain partial displayed rows; no remote contents have been changed.

SFTP v3 lacks Linux descriptor-relative `openat2` semantics and inode/change-time identities.
Archiv checks each path ancestor using LSTAT before opening the directory and refuses observed
symlinks; a concurrent remote rename/symlink replacement can race those observations. The configured
root is a navigation boundary, not a server security sandbox. Server permissions or an SFTP chroot
must enforce actual access restrictions. There are no remote mutations or data reads in this slice.
Failures never fall back to the same-named local directory. SSH stderr is discarded rather than
captured without bounds; connection errors direct you to verify SSH configuration/authentication
with your normal SFTP client. Real NAS and full encrypted SSH connection acceptance remain open.

Protocol references: [OpenSSH invocation](https://man.openbsd.org/ssh.1),
[SFTP version 3 wire format](https://www.ietf.org/archive/id/draft-ietf-secsh-filexfer-02.txt).
The platform adapter implements only the small bounded read-only packet subset; SSH encryption,
authentication and host verification remain in maintained system OpenSSH. It adds no dependency
or copied implementation.
