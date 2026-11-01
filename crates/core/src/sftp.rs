//! Credential-free direct SFTP source settings.
use serde::{Deserialize, Serialize};

/// OpenSSH connection settings. Omitted user/port use the user's SSH configuration.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SftpConnection {
    /// Hostname, IP address or trusted ~/.ssh/config Host alias.
    pub host: String,
    /// Optional login name; no password is stored.
    #[serde(default)]
    pub user: String,
    /// Optional port override; None uses SSH configuration (normally 22).
    #[serde(default)]
    pub port: Option<u16>,
}
impl SftpConnection {
    /// Reject option injection, URLs, whitespace, embedded credentials and oversized inputs.
    pub fn validate(&self) -> Result<(), String> {
        if self.host.is_empty()
            || self.host.len() > 253
            || self.host.starts_with('-')
            || !self.host.bytes().all(|b| b.is_ascii_alphanumeric() || b".-_:".contains(&b))
            || self.user.len() > 128
            || self.user.starts_with('-')
            || !self.user.bytes().all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
            || self.port == Some(0)
        {
            return Err("SFTP needs a hostname/IP/SSH alias, an optional username and port 1–65535".into());
        }
        Ok(())
    }
    /// Human-readable endpoint, never a process argument or an operation path.
    pub fn authority(&self) -> String {
        let host = if self.host.contains(':') { format!("[{}]", self.host) } else { self.host.clone() };
        format!(
            "{}{}{}",
            if self.user.is_empty() { String::new() } else { format!("{}@", self.user) },
            host,
            self.port.map_or_else(String::new, |port| format!(":{port}"))
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Source, SourceId, library::LibrarySettings};
    #[test]
    fn endpoints_reject_credentials_options_and_malformed_hosts() {
        let mut connection = SftpConnection { host: "nas-alias".into(), user: String::new(), port: None };
        assert!(connection.validate().is_ok());
        for host in ["", "-oProxyCommand=bad", "user@host", "sftp://host", "host\nother", "host/path", "$(command)"] {
            connection.host = host.into();
            assert!(connection.validate().is_err(), "accepted {host}");
        }
        connection.host = "2001:db8::1".into();
        assert!(connection.validate().is_ok());
        assert_eq!(connection.authority(), "[2001:db8::1]");
        connection.port = Some(0);
        assert!(connection.validate().is_err());
    }
    #[test]
    fn remote_namespaces_are_distinct_and_cannot_enable_writes_or_nfs() {
        let source = Source {
            id: SourceId(1),
            root: "/media".into(),
            label: "Remote".into(),
            nfs_remote: None,
            sftp: Some(SftpConnection { host: "first".into(), user: "reader".into(), port: None }),
        };
        let mut second = source.clone();
        second.id = SourceId(2);
        second.sftp.as_mut().unwrap().host = "second".into();
        let mut settings = LibrarySettings { sources: vec![source, second], ..Default::default() };
        settings.validate().unwrap();
        assert_eq!(settings.sources[0].display_location(&Default::default()), "sftp://reader@first/media");
        settings.writes.push(crate::actions::WritePolicy {
            source: SourceId(1),
            enabled: true,
            allowed: vec![],
            protected: vec![],
        });
        assert!(settings.validate().is_err());
        settings.writes.clear();
        settings.sources[0].nfs_remote = Some("host:/media".into());
        assert!(settings.validate().is_err());
    }
}
