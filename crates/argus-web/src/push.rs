//! Telling a phone an agent needs it, through its browser's push service.
//!
//! Web Push is the one thing `argus web` sends off the machine: an HTTPS
//! request to whichever push service the phone's browser uses (Apple's,
//! Google's, Mozilla's). What it carries is encrypted end to end for that
//! browser (RFC 8291), so the service relays bytes it cannot read, and it
//! names only an agent's title and status, never anything from its
//! conversation. The request is signed with a key pair kept in the config
//! directory (VAPID, RFC 8292), which is what lets a service tell this
//! server's pushes from anyone else's. A browser only subscribes from a page
//! served over HTTPS, so none of this happens without Tailscale Serve or
//! something like it in front.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::Aes128Gcm;
use argus_protocol::PaneStatus;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use hkdf::Hkdf;
use p256::ecdsa::signature::Signer;
use p256::ecdsa::{Signature, SigningKey};
use p256::elliptic_curve::sec1::ToSec1Point;
use p256::{PublicKey, SecretKey};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

/// The file the signing key lives in, in the argus config directory.
pub const KEY_FILE: &str = "web-push.json";

/// How long a push service holds a push for a phone that is off.
const TTL: Duration = Duration::from_secs(12 * 60 * 60);

/// Where a browser takes pushes: the service's URL for it, and the keys
/// what is sent must be encrypted to. As the browser's `PushSubscription`
/// names them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Subscription {
    pub endpoint: String,
    pub keys: SubscriptionKeys,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubscriptionKeys {
    /// The browser's P-256 public key, base64url.
    pub p256dh: String,
    /// The shared secret, base64url.
    pub auth: String,
}

/// What one push says. Only what a lock screen shows, and where tapping it
/// goes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Notice {
    pub title: String,
    pub body: String,
    pub pane: u64,
    pub url: String,
}

/// Whether a pane going from `before` to `after` is worth waking a phone
/// for, and in what words: a person being waited on, work to look at, a
/// failure, or a turn ending.
pub fn worth_telling(before: PaneStatus, after: PaneStatus) -> Option<&'static str> {
    if before == after {
        return None;
    }
    match after {
        PaneStatus::Waiting => Some("is waiting for you"),
        PaneStatus::NeedsReview => Some("wants a review"),
        PaneStatus::Failed => Some("failed"),
        PaneStatus::Idle if before == PaneStatus::Working => Some("finished its turn"),
        _ => None,
    }
}

/// This server's signing key.
pub struct Vapid {
    key: SigningKey,
    public: Vec<u8>,
}

#[derive(Serialize, Deserialize)]
struct StoredKey {
    /// The P-256 private scalar, base64url.
    private: String,
}

impl Vapid {
    /// The key kept in `dir`, made on first use.
    pub fn load_or_create(dir: &Path) -> anyhow::Result<Vapid> {
        let path = dir.join(KEY_FILE);
        if let Ok(bytes) = std::fs::read(&path) {
            let stored: StoredKey = serde_json::from_slice(&bytes)?;
            let raw = URL_SAFE_NO_PAD.decode(stored.private)?;
            return Vapid::from_secret(&SecretKey::from_slice(&raw)?);
        }
        let secret = random_secret()?;
        let stored = StoredKey {
            private: URL_SAFE_NO_PAD.encode(secret.to_bytes()),
        };
        write_private(&path, &serde_json::to_vec_pretty(&stored)?)?;
        Vapid::from_secret(&secret)
    }

    fn from_secret(secret: &SecretKey) -> anyhow::Result<Vapid> {
        Ok(Vapid {
            key: SigningKey::from(secret),
            public: uncompressed(&secret.public_key()),
        })
    }

    /// The public key a browser subscribes with, base64url.
    pub fn public_key(&self) -> String {
        URL_SAFE_NO_PAD.encode(&self.public)
    }

    /// The `Authorization` header for a push to `endpoint`: a token only
    /// this key could have signed, for that service, for twelve hours.
    fn authorization(&self, endpoint: &str, subject: &str) -> anyhow::Result<String> {
        let audience = origin(endpoint).ok_or_else(|| anyhow::anyhow!("not an https endpoint"))?;
        let expires = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() + TTL.as_secs();
        let header = URL_SAFE_NO_PAD.encode(r#"{"typ":"JWT","alg":"ES256"}"#);
        let claims = URL_SAFE_NO_PAD.encode(
            serde_json::json!({ "aud": audience, "exp": expires, "sub": subject }).to_string(),
        );
        let signed = format!("{header}.{claims}");
        let signature: Signature = self.key.sign(signed.as_bytes());
        let token = format!("{signed}.{}", URL_SAFE_NO_PAD.encode(signature.to_bytes()));
        Ok(format!("vapid t={token}, k={}", self.public_key()))
    }

    /// Sends one push. Blocking: call it off the runtime's threads. The
    /// status says what became of it; 404 and 410 mean the browser has let
    /// the subscription go.
    pub fn send(&self, subscription: &Subscription, notice: &Notice, subject: &str) -> anyhow::Result<u16> {
        let body = encrypt(&serde_json::to_vec(notice)?, subscription)?;
        let authorization = self.authorization(&subscription.endpoint, subject)?;
        let response = ureq::post(&subscription.endpoint)
            .config()
            .http_status_as_error(false)
            .build()
            .header("TTL", TTL.as_secs().to_string())
            .header("Urgency", "high")
            .header("Content-Encoding", "aes128gcm")
            .header("Content-Type", "application/octet-stream")
            .header("Authorization", authorization)
            .send(&body[..])?;
        Ok(response.status().as_u16())
    }
}

/// `payload` encrypted for one browser, as RFC 8291 lays it out: a key
/// agreed with the browser's, mixed with its secret, sealing one record.
pub fn encrypt(payload: &[u8], subscription: &Subscription) -> anyhow::Result<Vec<u8>> {
    let browser_bytes = URL_SAFE_NO_PAD.decode(&subscription.keys.p256dh)?;
    let browser = PublicKey::from_sec1_bytes(&browser_bytes)?;
    let auth = URL_SAFE_NO_PAD.decode(&subscription.keys.auth)?;
    let ours = random_secret()?;
    let our_public = uncompressed(&ours.public_key());
    let mut salt = [0u8; 16];
    getrandom::fill(&mut salt).map_err(|e| anyhow::anyhow!("no randomness: {e}"))?;
    seal(payload, &browser_bytes, &browser, &auth, &ours, &our_public, &salt)
}

fn seal(
    payload: &[u8],
    browser_bytes: &[u8],
    browser: &PublicKey,
    auth: &[u8],
    ours: &SecretKey,
    our_public: &[u8],
    salt: &[u8; 16],
) -> anyhow::Result<Vec<u8>> {
    let shared = p256::ecdh::diffie_hellman(ours.to_nonzero_scalar(), browser.as_affine());
    let mut info = b"WebPush: info\0".to_vec();
    info.extend_from_slice(browser_bytes);
    info.extend_from_slice(our_public);
    let mut ikm = [0u8; 32];
    Hkdf::<Sha256>::new(Some(auth), shared.raw_secret_bytes().as_slice())
        .expand(&info, &mut ikm)
        .map_err(|_| anyhow::anyhow!("key derivation failed"))?;

    let prk = Hkdf::<Sha256>::new(Some(salt), &ikm);
    let mut key = [0u8; 16];
    let mut nonce = [0u8; 12];
    prk.expand(b"Content-Encoding: aes128gcm\0", &mut key)
        .and_then(|_| prk.expand(b"Content-Encoding: nonce\0", &mut nonce))
        .map_err(|_| anyhow::anyhow!("key derivation failed"))?;

    // One record, so it is also the last: a 2 after the payload says so.
    let mut record = payload.to_vec();
    record.push(2);
    let cipher = Aes128Gcm::new(&key.into());
    let sealed = cipher
        .encrypt(&nonce.into(), record.as_slice())
        .map_err(|_| anyhow::anyhow!("encryption failed"))?;

    let mut body = Vec::with_capacity(16 + 4 + 1 + our_public.len() + sealed.len());
    body.extend_from_slice(salt);
    body.extend_from_slice(&4096u32.to_be_bytes());
    body.push(our_public.len() as u8);
    body.extend_from_slice(our_public);
    body.extend_from_slice(&sealed);
    Ok(body)
}

fn random_secret() -> anyhow::Result<SecretKey> {
    loop {
        let mut bytes = [0u8; 32];
        getrandom::fill(&mut bytes).map_err(|e| anyhow::anyhow!("no randomness: {e}"))?;
        // Almost every 32 bytes is a valid scalar; the rare one that is not
        // is drawn again.
        if let Ok(secret) = SecretKey::from_slice(&bytes) {
            return Ok(secret);
        }
    }
}

fn uncompressed(key: &PublicKey) -> Vec<u8> {
    key.to_sec1_point(false).as_bytes().to_vec()
}

/// `https://host[:port]` of a URL, which is what a VAPID token is for.
fn origin(url: &str) -> Option<String> {
    let rest = url.strip_prefix("https://")?;
    let host = rest.split('/').next()?;
    (!host.is_empty()).then(|| format!("https://{host}"))
}

fn write_private(path: &PathBuf, bytes: &[u8]) -> anyhow::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, bytes)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use p256::ecdsa::signature::Verifier;
    use p256::ecdsa::VerifyingKey;

    /// RFC 8291, section 5: the worked example, byte for byte.
    #[test]
    fn encryption_matches_the_rfcs_own_example() {
        let d = |s: &str| URL_SAFE_NO_PAD.decode(s).unwrap();
        let plaintext = b"When I grow up, I want to be a watermelon";
        let browser_bytes = d("BCVxsr7N_eNgVRqvHtD0zTZsEc6-VV-JvLexhqUzORcxaOzi6-AYWXvTBHm4bjyPjs7Vd8pZGH6SRpkNtoIAiw4");
        let auth = d("BTBZMqHH6r4Tts7J_aSIgg");
        let ours = SecretKey::from_slice(&d("yfWPiYE-n46HLnH0KqZOF1fJJU3MYrct3AELtAQ-oRw")).unwrap();
        let our_public = d("BP4z9KsN6nGRTbVYI_c7VJSPQTBtkgcy27mlmlMoZIIgDll6e3vCYLocInmYWAmS6TlzAC8wEqKK6PBru3jl7A8");
        let salt: [u8; 16] = d("DGv6ra1nlYgDCS1FRnbzlw").try_into().unwrap();
        let browser = PublicKey::from_sec1_bytes(&browser_bytes).unwrap();
        assert_eq!(uncompressed(&ours.public_key()), our_public);

        let body = seal(plaintext, &browser_bytes, &browser, &auth, &ours, &our_public, &salt).unwrap();

        assert_eq!(
            URL_SAFE_NO_PAD.encode(body),
            "DGv6ra1nlYgDCS1FRnbzlwAAEABBBP4z9KsN6nGRTbVYI_c7VJSPQTBtkgcy27mlmlMoZIIgDll6e3vCYLocInmYWAmS6TlzAC8wEqKK6PBru3jl7A_yl95bQpu6cVPTpK4Mqgkf1CXztLVBSt2Ks3oZwbuwXPXLWyouBWLVWGNWQexSgSxsj_Qulcy4a-fN"
        );
    }

    #[test]
    fn a_push_is_signed_for_the_service_it_goes_to() {
        let dir = tempfile::tempdir().unwrap();
        let vapid = Vapid::load_or_create(dir.path()).unwrap();
        let again = Vapid::load_or_create(dir.path()).unwrap();
        assert_eq!(vapid.public_key(), again.public_key(), "the key is kept");

        let header = vapid
            .authorization("https://fcm.googleapis.com/fcm/send/abc", "mailto:me@example.com")
            .unwrap();
        let token = header
            .strip_prefix("vapid t=")
            .and_then(|rest| rest.split(", k=").next())
            .unwrap();
        let parts: Vec<&str> = token.split('.').collect();
        let claims: serde_json::Value =
            serde_json::from_slice(&URL_SAFE_NO_PAD.decode(parts[1]).unwrap()).unwrap();
        assert_eq!(claims["aud"], "https://fcm.googleapis.com");
        assert_eq!(claims["sub"], "mailto:me@example.com");

        let verifying = VerifyingKey::from_sec1_bytes(&URL_SAFE_NO_PAD.decode(vapid.public_key()).unwrap()).unwrap();
        let signature = Signature::from_slice(&URL_SAFE_NO_PAD.decode(parts[2]).unwrap()).unwrap();
        assert!(verifying
            .verify(format!("{}.{}", parts[0], parts[1]).as_bytes(), &signature)
            .is_ok());
        assert!(vapid.authorization("http://insecure.example/x", "mailto:x").is_err());
    }

    #[test]
    fn only_a_wait_a_review_a_failure_or_a_turn_ending_is_worth_a_push() {
        use PaneStatus::*;
        assert_eq!(worth_telling(Working, Waiting), Some("is waiting for you"));
        assert_eq!(worth_telling(Working, Idle), Some("finished its turn"));
        assert_eq!(worth_telling(Working, NeedsReview), Some("wants a review"));
        assert_eq!(worth_telling(Working, Failed), Some("failed"));
        assert_eq!(worth_telling(Idle, Working), None);
        assert_eq!(worth_telling(Waiting, Idle), None, "an answered prompt is not news");
        assert_eq!(worth_telling(Waiting, Waiting), None);
        assert_eq!(worth_telling(Idle, Exited { code: Some(0) }), None);
    }
}
