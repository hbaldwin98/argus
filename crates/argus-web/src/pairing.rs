//! Who may connect: the one-time code a phone pairs with, and the device
//! tokens it is given for doing so.
//!
//! A server that types into agents is a shell on the machine, so nothing
//! reaches an agent without a token. A token is shown to its device once,
//! in a cookie, and kept here only as a hash; the file of hashes is what
//! `argus web devices` lists and `argus web revoke` edits, and a running
//! server reads it again rather than trusting what it read at startup.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// How long a pairing code stands.
pub const CODE_LIFETIME: Duration = Duration::from_secs(5 * 60);

/// How many wrong codes one code survives. Six digits are a million
/// guesses; five of them are not a meaningful dent.
pub const CODE_ATTEMPTS: u32 = 5;

/// The file device hashes live in, in the argus config directory.
pub const DEVICES_FILE: &str = "web-devices.json";

/// A paired device, as kept on disk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Device {
    /// What the person called it when pairing, and what `revoke` takes.
    pub name: String,
    /// SHA-256 of the token, in hex. The token itself is never kept.
    pub hash: String,
    /// When it paired, in seconds since the Unix epoch.
    pub paired: u64,
}

/// The devices file.
pub struct Devices {
    path: PathBuf,
}

/// Why a pairing attempt failed, in words for the page.
#[derive(Debug, PartialEq, Eq)]
pub enum Refusal {
    /// Wrong, used, or never issued.
    WrongCode,
    /// Past its lifetime, or out of attempts.
    Expired,
    /// The name is empty or already taken.
    BadName,
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Refusal::WrongCode => "that code is not the one argus web is showing",
            Refusal::Expired => "that code has expired; press Enter in argus web for a new one",
            Refusal::BadName => "give this device a name no other paired device has",
        })
    }
}

impl Devices {
    pub fn at(dir: &Path) -> Devices {
        Devices {
            path: dir.join(DEVICES_FILE),
        }
    }

    /// Every paired device. A missing or unreadable file is no devices:
    /// the worst it can do is make someone pair again.
    pub fn list(&self) -> Vec<Device> {
        std::fs::read(&self.path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    /// The device a token belongs to, if it still does.
    pub fn check(&self, token: &str) -> Option<Device> {
        let hash = hash(token);
        self.list().into_iter().find(|d| d.hash == hash)
    }

    /// Adds a device, returning the token it is to be given.
    pub fn add(&self, name: &str) -> anyhow::Result<String> {
        let mut devices = self.list();
        let token = random_hex(32)?;
        devices.push(Device {
            name: name.to_string(),
            hash: hash(&token),
            paired: now(),
        });
        self.save(&devices)?;
        Ok(token)
    }

    /// Removes a device by name. Says whether there was one.
    pub fn revoke(&self, name: &str) -> anyhow::Result<bool> {
        let mut devices = self.list();
        let before = devices.len();
        devices.retain(|d| d.name != name);
        if devices.len() == before {
            return Ok(false);
        }
        self.save(&devices)?;
        Ok(true)
    }

    fn name_is_free(&self, name: &str) -> bool {
        self.list().iter().all(|d| d.name != name)
    }

    /// Writes the whole file, readable by its owner alone: it is the list
    /// of who may type into this user's agents.
    fn save(&self, devices: &[Device]) -> anyhow::Result<()> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(devices)?)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))?;
        }
        std::fs::rename(&tmp, &self.path)?;
        Ok(())
    }
}

/// The code on show, and what is left of it.
pub struct Code {
    digits: String,
    issued: Instant,
    attempts: u32,
    used: bool,
}

impl Code {
    pub fn new() -> anyhow::Result<Code> {
        let mut bytes = [0u8; 4];
        getrandom::fill(&mut bytes).map_err(|e| anyhow::anyhow!("no randomness: {e}"))?;
        let n = u32::from_le_bytes(bytes) % 1_000_000;
        Ok(Code {
            digits: format!("{n:06}"),
            issued: Instant::now(),
            attempts: 0,
            used: false,
        })
    }

    pub fn digits(&self) -> &str {
        &self.digits
    }

    /// Trades the code for a device token. A wrong guess counts against
    /// the code; the right one uses it up.
    pub fn redeem(&mut self, guess: &str, name: &str, devices: &Devices) -> Result<String, Refusal> {
        self.redeem_at(guess, name, devices, Instant::now())
    }

    fn redeem_at(
        &mut self,
        guess: &str,
        name: &str,
        devices: &Devices,
        at: Instant,
    ) -> Result<String, Refusal> {
        if self.used {
            return Err(Refusal::WrongCode);
        }
        if at.duration_since(self.issued) > CODE_LIFETIME || self.attempts >= CODE_ATTEMPTS {
            return Err(Refusal::Expired);
        }
        if !constant_time_eq(guess.trim().as_bytes(), self.digits.as_bytes()) {
            self.attempts += 1;
            return Err(Refusal::WrongCode);
        }
        let name = name.trim();
        if name.is_empty() || name.len() > 64 || !devices.name_is_free(name) {
            return Err(Refusal::BadName);
        }
        let token = devices.add(name).map_err(|_| Refusal::BadName)?;
        self.used = true;
        Ok(token)
    }
}

fn hash(token: &str) -> String {
    hex(&Sha256::digest(token.as_bytes()))
}

fn random_hex(len: usize) -> anyhow::Result<String> {
    let mut bytes = vec![0u8; len];
    getrandom::fill(&mut bytes).map_err(|e| anyhow::anyhow!("no randomness: {e}"))?;
    Ok(hex(&bytes))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Compares without stopping at the first difference, so how long a wrong
/// code takes to refuse says nothing about how much of it was right.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh() -> (tempfile::TempDir, Devices, Code) {
        let dir = tempfile::tempdir().unwrap();
        let devices = Devices::at(dir.path());
        (dir, devices, Code::new().unwrap())
    }

    #[test]
    fn the_right_code_pairs_a_device_once() {
        let (_dir, devices, mut code) = fresh();
        let digits = code.digits().to_string();
        let token = code.redeem(&digits, "phone", &devices).unwrap();
        assert_eq!(devices.check(&token).unwrap().name, "phone");
        assert_eq!(code.redeem(&digits, "tablet", &devices), Err(Refusal::WrongCode));
    }

    #[test]
    fn only_a_hash_of_the_token_is_kept() {
        let (dir, devices, mut code) = fresh();
        let digits = code.digits().to_string();
        let token = code.redeem(&digits, "phone", &devices).unwrap();
        let on_disk = std::fs::read_to_string(dir.path().join(DEVICES_FILE)).unwrap();
        assert!(!on_disk.contains(&token));
        assert_eq!(token.len(), 64, "256 bits");
    }

    #[test]
    fn five_wrong_guesses_use_up_a_code() {
        let (_dir, devices, mut code) = fresh();
        let digits = code.digits().to_string();
        let wrong = if digits == "000000" { "111111" } else { "000000" };
        for _ in 0..CODE_ATTEMPTS {
            assert_eq!(code.redeem(wrong, "phone", &devices), Err(Refusal::WrongCode));
        }
        assert_eq!(code.redeem(&digits, "phone", &devices), Err(Refusal::Expired));
    }

    #[test]
    fn a_code_expires() {
        let (_dir, devices, mut code) = fresh();
        let digits = code.digits().to_string();
        let later = Instant::now() + CODE_LIFETIME + Duration::from_secs(1);
        assert_eq!(code.redeem_at(&digits, "phone", &devices, later), Err(Refusal::Expired));
    }

    #[test]
    fn a_revoked_device_is_refused() {
        let (_dir, devices, mut code) = fresh();
        let digits = code.digits().to_string();
        let token = code.redeem(&digits, "phone", &devices).unwrap();
        assert!(devices.revoke("phone").unwrap());
        assert!(devices.check(&token).is_none());
        assert!(!devices.revoke("phone").unwrap());
    }

    #[test]
    fn two_devices_cannot_share_a_name() {
        let (_dir, devices, mut code) = fresh();
        let digits = code.digits().to_string();
        code.redeem(&digits, "phone", &devices).unwrap();
        let mut second = Code::new().unwrap();
        let digits = second.digits().to_string();
        assert_eq!(second.redeem(&digits, "phone", &devices), Err(Refusal::BadName));
        assert_eq!(second.redeem(&digits, "  ", &devices), Err(Refusal::BadName));
    }
}
