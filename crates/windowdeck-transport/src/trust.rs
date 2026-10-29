//! Device identity and the list of paired peers, stored per user.
//!
//! The private key never leaves this directory and is never logged. Peers are
//! identified by their static public key; names are only labels for people.
use curve25519_dalek::montgomery::MontgomeryPoint;
use sha2::{Digest, Sha256};
use std::fmt;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const KEY_LEN: usize = 32;
/// Shown to people and accepted when revoking; long enough to avoid accidental matches.
const FINGERPRINT_BYTES: usize = 8;
const MIN_SELECTOR_DIGITS: usize = 4;
pub const MAX_NAME_CHARS: usize = 40;
const MAX_FILE: u64 = 1 << 20;
const IDENTITY_HEADER: &str = "windowdeck-identity 1";
const PEERS_HEADER: &str = "windowdeck-paired 1";

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct PublicKey([u8; KEY_LEN]);

impl PublicKey {
    pub fn from_slice(bytes: &[u8]) -> Option<Self> {
        bytes.try_into().ok().map(Self)
    }

    pub fn as_bytes(&self) -> &[u8; KEY_LEN] {
        &self.0
    }

    /// Short, stable identifier such as `1a2b-3c4d-5e6f-7a8b`.
    pub fn fingerprint(&self) -> String {
        let digest = Sha256::digest(self.0);
        digest[..FINGERPRINT_BYTES]
            .chunks(2)
            .map(hex)
            .collect::<Vec<_>>()
            .join("-")
    }
}

impl fmt::Debug for PublicKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PublicKey({})", self.fingerprint())
    }
}

/// A static X25519 key pair. The private half is redacted from `Debug`.
pub struct Identity {
    private: [u8; KEY_LEN],
    public: PublicKey,
}

impl Identity {
    pub fn generate() -> io::Result<Self> {
        let mut private = [0; KEY_LEN];
        getrandom::fill(&mut private).map_err(|error| io::Error::other(error.to_string()))?;
        Ok(Self::from_private(private))
    }

    fn from_private(private: [u8; KEY_LEN]) -> Self {
        // Noise clamps the scalar the same way when computing DH.
        let public = PublicKey(MontgomeryPoint::mul_base_clamped(private).to_bytes());
        Self { private, public }
    }

    pub fn public(&self) -> PublicKey {
        self.public
    }

    pub(crate) fn private(&self) -> &[u8; KEY_LEN] {
        &self.private
    }
}

impl fmt::Debug for Identity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Identity({})", self.public.fingerprint())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Host,
    Client,
}

impl Role {
    fn identity_file(self) -> &'static str {
        match self {
            Self::Host => "host-identity.txt",
            Self::Client => "client-identity.txt",
        }
    }

    fn peers_file(self) -> &'static str {
        match self {
            Self::Host => "paired-clients.txt",
            Self::Client => "paired-hosts.txt",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Device {
    pub key: PublicKey,
    pub name: String,
    /// Seconds since the Unix epoch.
    pub paired_at: u64,
}

impl Device {
    /// Pairing date (UTC) as `YYYY-MM-DD`.
    pub fn paired_on(&self) -> String {
        // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
        let days = (self.paired_at / 86_400) as i64 + 719_468;
        let era = days.div_euclid(146_097);
        let day_of_era = days.rem_euclid(146_097);
        let year_of_era =
            (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
        let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
        let month_index = (5 * day_of_year + 2) / 153;
        let day = day_of_year - (153 * month_index + 2) / 5 + 1;
        let month = if month_index < 10 {
            month_index + 3
        } else {
            month_index - 9
        };
        let year = year_of_era + era * 400 + i64::from(month <= 2);
        format!("{year:04}-{month:02}-{day:02}")
    }
}

/// Identity and paired peers of one role in one directory. Every query reads the
/// file again, so a revocation applies to the next connection of a running host.
#[derive(Clone, Debug)]
pub struct Trust {
    dir: PathBuf,
    role: Role,
}

impl Trust {
    pub fn open(dir: impl Into<PathBuf>, role: Role) -> io::Result<Self> {
        let dir = dir.into();
        create_private_dir(&dir)?;
        Ok(Self { dir, role })
    }

    /// `WINDOWDECK_DATA_DIR`, or the per-user data folder of the platform.
    pub fn open_default(role: Role) -> io::Result<Self> {
        Self::open(default_dir()?, role)
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Loads this device's identity, creating it on first use. A damaged file is
    /// an error rather than a silent replacement that would break every pairing.
    pub fn identity(&self) -> io::Result<Identity> {
        let path = self.dir.join(self.role.identity_file());
        match read_limited(&path) {
            Ok(text) => parse_identity(&text).ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "la identidad de WindowDeck está dañada: {}. Bórrala para crear otra; habrá que emparejar de nuevo",
                        path.display()
                    ),
                )
            }),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                let identity = Identity::generate()?;
                write_private(
                    &path,
                    &format!("{IDENTITY_HEADER}\n{}\n", hex(identity.private())),
                    false,
                )?;
                // Another process may have created it first: always use the stored key.
                self.identity()
            }
            Err(error) => Err(error),
        }
    }

    pub fn devices(&self) -> io::Result<Vec<Device>> {
        let path = self.peers_path();
        match read_limited(&path) {
            Ok(text) => parse_devices(&text).ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "la lista de dispositivos emparejados está dañada: {}",
                        path.display()
                    ),
                )
            }),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(error) => Err(error),
        }
    }

    pub fn is_paired(&self, key: &PublicKey) -> io::Result<bool> {
        Ok(self.devices()?.iter().any(|device| device.key == *key))
    }

    /// Adds or renews a peer. The name is sanitized because it comes from the peer.
    pub fn add(&self, key: PublicKey, name: &str) -> io::Result<Device> {
        let device = Device {
            key,
            name: sanitize_name(name),
            paired_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |elapsed| elapsed.as_secs()),
        };
        let mut devices = self.devices()?;
        devices.retain(|existing| existing.key != key);
        devices.push(device.clone());
        self.store(&devices)?;
        Ok(device)
    }

    /// Removes the peer whose fingerprint starts with `selector` (dashes and case
    /// ignored). Refuses ambiguous or too short selectors.
    pub fn remove(&self, selector: &str) -> io::Result<Device> {
        let wanted: String = selector
            .chars()
            .filter(|c| *c != '-')
            .map(|c| c.to_ascii_lowercase())
            .collect();
        if wanted.len() < MIN_SELECTOR_DIGITS || !wanted.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "indica al menos cuatro dígitos hexadecimales de la huella",
            ));
        }
        let mut devices = self.devices()?;
        let matches: Vec<usize> = devices
            .iter()
            .enumerate()
            .filter(|(_, device)| {
                device
                    .key
                    .fingerprint()
                    .replace('-', "")
                    .starts_with(&wanted)
            })
            .map(|(index, _)| index)
            .collect();
        match matches.as_slice() {
            [] => Err(io::Error::new(
                io::ErrorKind::NotFound,
                "no hay ningún dispositivo emparejado con esa huella",
            )),
            [index] => {
                let removed = devices.remove(*index);
                self.store(&devices)?;
                Ok(removed)
            }
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "la huella coincide con varios dispositivos; escribe más dígitos",
            )),
        }
    }

    pub fn remove_key(&self, key: &PublicKey) -> io::Result<bool> {
        let mut devices = self.devices()?;
        let before = devices.len();
        devices.retain(|device| device.key != *key);
        if devices.len() == before {
            return Ok(false);
        }
        self.store(&devices)?;
        Ok(true)
    }

    pub fn clear(&self) -> io::Result<usize> {
        let count = self.devices()?.len();
        self.store(&[])?;
        Ok(count)
    }

    fn peers_path(&self) -> PathBuf {
        self.dir.join(self.role.peers_file())
    }

    fn store(&self, devices: &[Device]) -> io::Result<()> {
        let mut text = format!("{PEERS_HEADER}\n");
        for device in devices {
            text.push_str(&format!(
                "{} {} {}\n",
                hex(device.key.as_bytes()),
                device.paired_at,
                device.name
            ));
        }
        write_private(&self.peers_path(), &text, true)
    }
}

pub fn default_dir() -> io::Result<PathBuf> {
    if let Some(dir) = std::env::var_os("WINDOWDECK_DATA_DIR") {
        return std::path::absolute(dir);
    }
    let missing = || {
        io::Error::new(
            io::ErrorKind::NotFound,
            "no se encuentra la carpeta de datos del usuario",
        )
    };
    if cfg!(windows) {
        let local = std::env::var_os("LOCALAPPDATA").ok_or_else(missing)?;
        return Ok(PathBuf::from(local).join("WindowDeck").join("security"));
    }
    let data = match std::env::var_os("XDG_DATA_HOME").filter(|dir| !dir.is_empty()) {
        Some(dir) => PathBuf::from(dir),
        None => PathBuf::from(std::env::var_os("HOME").ok_or_else(missing)?).join(".local/share"),
    };
    Ok(data.join("windowdeck").join("security"))
}

/// A readable name for this computer, offered to the peer during pairing.
pub fn local_name() -> String {
    let raw = if cfg!(windows) {
        std::env::var("COMPUTERNAME").ok()
    } else {
        fs::read_to_string("/proc/sys/kernel/hostname")
            .or_else(|_| fs::read_to_string("/etc/hostname"))
            .ok()
            .or_else(|| std::env::var("HOSTNAME").ok())
    };
    sanitize_name(raw.as_deref().unwrap_or_default())
}

/// Keeps printable characters on one line and bounds the length. Peer names are
/// untrusted and end up in files, logs and dialogs.
pub fn sanitize_name(raw: &str) -> String {
    let mut name = String::new();
    for word in raw
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>()
        .split_whitespace()
    {
        if !name.is_empty() {
            name.push(' ');
        }
        name.push_str(word);
    }
    let name: String = name.chars().take(MAX_NAME_CHARS).collect();
    let name = name.trim_end();
    if name.is_empty() {
        "Dispositivo sin nombre".into()
    } else {
        name.into()
    }
}

fn parse_identity(text: &str) -> Option<Identity> {
    let mut lines = text.lines();
    if lines.next()? != IDENTITY_HEADER {
        return None;
    }
    let private = parse_key(lines.next()?)?;
    if lines.any(|line| !line.trim().is_empty()) {
        return None;
    }
    Some(Identity::from_private(private))
}

fn parse_devices(text: &str) -> Option<Vec<Device>> {
    let mut lines = text.lines();
    if lines.next()? != PEERS_HEADER {
        return None;
    }
    let mut devices: Vec<Device> = Vec::new();
    for line in lines.filter(|line| !line.is_empty()) {
        let mut fields = line.splitn(3, ' ');
        let key = PublicKey(parse_key(fields.next()?)?);
        let paired_at = fields.next()?.parse().ok()?;
        let name = fields.next()?;
        if name != sanitize_name(name) || devices.iter().any(|device| device.key == key) {
            return None;
        }
        devices.push(Device {
            key,
            name: name.into(),
            paired_at,
        });
    }
    Some(devices)
}

fn parse_key(text: &str) -> Option<[u8; KEY_LEN]> {
    if text.len() != KEY_LEN * 2 || !text.is_ascii() {
        return None;
    }
    let mut key = [0; KEY_LEN];
    for (index, byte) in key.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&text[index * 2..index * 2 + 2], 16).ok()?;
    }
    Some(key)
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn read_limited(path: &Path) -> io::Result<String> {
    if fs::metadata(path)?.len() > MAX_FILE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{} es demasiado grande", path.display()),
        ));
    }
    fs::read_to_string(path)
}

fn create_private_dir(dir: &Path) -> io::Result<()> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
    builder.create(dir)
}

/// Writes through a temporary file so readers never observe a partial list.
/// On Windows the per-user data folder already restricts access to its owner.
fn write_private(path: &Path, text: &str, replace: bool) -> io::Result<()> {
    let temporary = path.with_extension(format!("tmp{}", std::process::id()));
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let _ = fs::remove_file(&temporary);
    let result = (|| {
        let mut file = options.open(&temporary)?;
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
        drop(file);
        if !replace && path.exists() {
            return Ok(());
        }
        fs::rename(&temporary, path)
    })();
    let _ = fs::remove_file(&temporary);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::temporary_dir;

    #[test]
    fn identity_is_created_once_and_reloaded() {
        let dir = temporary_dir("identity");
        let trust = Trust::open(&dir, Role::Host).unwrap();
        let first = trust.identity().unwrap();
        let second = trust.identity().unwrap();
        assert_eq!(first.public(), second.public());
        assert_eq!(first.private(), second.private());
        // Roles keep separate identities in a shared directory.
        let client = Trust::open(&dir, Role::Client).unwrap().identity().unwrap();
        assert_ne!(client.public(), first.public());
        assert!(!format!("{first:?}").contains(&hex(first.private())));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(dir.join("host-identity.txt"))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o077, 0, "identity readable by others: {mode:o}");
        }
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn damaged_identity_is_reported_instead_of_replaced() {
        let dir = temporary_dir("damaged");
        let trust = Trust::open(&dir, Role::Client).unwrap();
        for text in [
            "",
            "windowdeck-identity 1\nzz\n",
            "windowdeck-identity 2\n0000000000000000000000000000000000000000000000000000000000000000\n",
            "windowdeck-identity 1\n0000000000000000000000000000000000000000000000000000000000000000\nextra\n",
        ] {
            fs::write(dir.join("client-identity.txt"), text).unwrap();
            let error = trust.identity().unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::InvalidData);
            assert_eq!(
                fs::read_to_string(dir.join("client-identity.txt")).unwrap(),
                text
            );
        }
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn paired_devices_can_be_added_listed_and_revoked() {
        let dir = temporary_dir("peers");
        let trust = Trust::open(&dir, Role::Host).unwrap();
        let deck = Identity::generate().unwrap().public();
        let other = Identity::generate().unwrap().public();
        assert!(trust.devices().unwrap().is_empty());
        trust.add(deck, "steamdeck\n\u{1b}[31m").unwrap();
        trust.add(other, "  OLED   Deck  ").unwrap();
        trust.add(deck, "steamdeck").unwrap(); // Renewal keeps a single entry.
        let devices = trust.devices().unwrap();
        assert_eq!(devices.len(), 2);
        assert_eq!(devices[0].name, "OLED Deck");
        assert_eq!(devices[1].name, "steamdeck");
        assert!(trust.is_paired(&deck).unwrap());

        assert!(trust.remove("12").is_err());
        assert!(trust.remove("zzzz").is_err());
        let fingerprint = deck.fingerprint();
        let removed = trust.remove(&fingerprint[..9].to_uppercase()).unwrap();
        assert_eq!(removed.key, deck);
        assert!(!trust.is_paired(&deck).unwrap());
        assert!(trust.is_paired(&other).unwrap());
        assert_eq!(
            trust.remove(&fingerprint).unwrap_err().kind(),
            io::ErrorKind::NotFound
        );
        assert!(trust.remove_key(&other).unwrap());
        assert!(!trust.remove_key(&other).unwrap());
        trust.add(deck, "steamdeck").unwrap();
        assert_eq!(trust.clear().unwrap(), 1);
        assert!(trust.devices().unwrap().is_empty());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn damaged_or_tampered_peer_lists_are_rejected() {
        let dir = temporary_dir("tampered");
        let trust = Trust::open(&dir, Role::Client).unwrap();
        let key = hex(Identity::generate().unwrap().public().as_bytes());
        for text in [
            "otro formato\n".to_string(),
            format!("{PEERS_HEADER}\n{key}\n"),
            format!("{PEERS_HEADER}\n{key} ayer pc\n"),
            format!("{PEERS_HEADER}\n{key} 1 pc\n{key} 2 pc\n"),
            format!("{PEERS_HEADER}\n{key} 1 pc  con  espacios\n"),
            format!("{PEERS_HEADER}\n{} 1 pc\n", &key[2..]),
        ] {
            fs::write(dir.join("paired-hosts.txt"), &text).unwrap();
            assert_eq!(
                trust.devices().unwrap_err().kind(),
                io::ErrorKind::InvalidData,
                "{text}"
            );
        }
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn names_are_bounded_single_line_labels() {
        assert_eq!(sanitize_name(""), "Dispositivo sin nombre");
        assert_eq!(sanitize_name("\r\n\t"), "Dispositivo sin nombre");
        assert_eq!(sanitize_name("Mi\nPC\u{7f}"), "Mi PC");
        assert_eq!(sanitize_name("Equipo de Íñigo"), "Equipo de Íñigo");
        assert_eq!(
            sanitize_name(&"x".repeat(100)).chars().count(),
            MAX_NAME_CHARS
        );
        assert!(!local_name().is_empty());
    }

    #[test]
    fn pairing_dates_are_readable() {
        let device = |paired_at| Device {
            key: PublicKey([1; KEY_LEN]),
            name: "Deck".into(),
            paired_at,
        };
        assert_eq!(device(0).paired_on(), "1970-01-01");
        assert_eq!(device(951_782_400).paired_on(), "2000-02-29");
        assert_eq!(device(1_798_761_599).paired_on(), "2026-12-31");
        assert_eq!(device(1_798_761_600).paired_on(), "2027-01-01");
    }

    #[test]
    fn fingerprints_are_stable_and_short() {
        let key = PublicKey([7; KEY_LEN]);
        assert_eq!(key.fingerprint(), key.fingerprint());
        assert_eq!(key.fingerprint().len(), 19);
        assert_ne!(key.fingerprint(), PublicKey([8; KEY_LEN]).fingerprint());
        assert_eq!(
            format!("{key:?}"),
            format!("PublicKey({})", key.fingerprint())
        );
    }
}
