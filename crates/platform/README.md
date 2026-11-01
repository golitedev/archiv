# platform

Linux descriptor-relative enumeration, mounted-NFS identity checks, bounded permanent deletion, no-overwrite rename/copy, atomic settings and structured process launch. See docs/file-actions.md for concurrency and durability limits.

Direct read-only SFTP v3 uses the system OpenSSH client. See docs/sftp.md for setup and bounds.
Protocol integration tests need the OpenSSH SFTP server executable (Debian/Ubuntu openssh-sftp-server;
Arch openssh). They start the read-only subsystem over sockets without a listener or credentials.
