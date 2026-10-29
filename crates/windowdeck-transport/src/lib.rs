//! Authenticated and encrypted transport, device pairing and the per-user list
//! of paired devices. See `docs/adr/0022-pairing-and-encryption.md`.
use std::fmt;
use std::io;

pub mod pairing;
pub mod secure;
pub mod trust;

pub use secure::{Intent, Peer, Reject, SecureStream, ShutdownHandle};
pub use trust::{Device, Identity, PublicKey, Role, Trust};

#[derive(Debug)]
pub enum SecureError {
    Io(io::Error),
    /// Malformed, unexpected or unauthenticated data from the other end.
    Protocol(&'static str),
    /// The other end uses another version of the encrypted transport.
    Incompatible(u16),
    /// This device does not trust the host: pairing is required.
    HostNotPaired(Peer),
    /// The host refused the connection.
    Rejected(Reject),
}

impl SecureError {
    /// True when pairing both devices would allow the connection.
    pub fn needs_pairing(&self) -> bool {
        matches!(
            self,
            Self::HostNotPaired(_) | Self::Rejected(Reject::NotPaired)
        )
    }
}

impl fmt::Display for SecureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "{error}"),
            Self::Protocol(reason) => write!(f, "conexión cifrada inválida: {reason}"),
            Self::Incompatible(version) => write!(
                f,
                "el otro equipo usa la versión {version} del cifrado de WindowDeck; actualiza ambos equipos"
            ),
            Self::HostNotPaired(peer) => write!(
                f,
                "el PC «{}» (huella {}) no está emparejado con este dispositivo",
                peer.name,
                peer.key.fingerprint()
            ),
            Self::Rejected(reason) => f.write_str(reason.message()),
        }
    }
}

impl std::error::Error for SecureError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for SecureError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<snow::Error> for SecureError {
    fn from(_: snow::Error) -> Self {
        // Details would only help an attacker; failures are logged by kind.
        Self::Protocol("negociación cifrada rechazada")
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    use std::path::PathBuf;

    pub fn temporary_dir(name: &str) -> PathBuf {
        let mut random = [0; 8];
        getrandom::fill(&mut random).unwrap();
        let dir =
            std::env::temp_dir().join(format!("windowdeck-{name}-{}", crate::trust::hex(&random)));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
}
