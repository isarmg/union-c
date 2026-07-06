//! 数据库敏感字段的对称加密。

use std::{fs, sync::OnceLock};

use aes_gcm::{
    Aes256Gcm, Key,
    aead::{Aead, AeadCore, KeyInit, OsRng},
};
use anyhow::{Context, bail};
use base64::{Engine as _, engine::general_purpose::STANDARD};

const LEGACY_PREFIX: &str = "enc:v1:";
const PREFIX: &str = "enc:v2:";
const KEY_ENV: &str = "UNION_SECRET_KEY";
const KEY_ID_ENV: &str = "UNION_SECRET_KEY_ID";
const KEY_PATH: &str = "union/data/union.secret";
static KEY_BYTES: OnceLock<[u8; 32]> = OnceLock::new();

pub fn init() -> anyhow::Result<()> {
    if KEY_BYTES.get().is_some() {
        return Ok(());
    }
    let production =
        std::env::var("UNION_ENV").is_ok_and(|value| value.eq_ignore_ascii_case("production"));
    let key = if let Ok(encoded) = std::env::var(KEY_ENV) {
        decode_key(encoded.trim()).context("invalid UNION_SECRET_KEY")?
    } else if production {
        bail!("{KEY_ENV} must be configured in production")
    } else {
        load_or_create_key()?
    };
    let _ = KEY_BYTES.set(key);
    Ok(())
}

pub fn is_encrypted(value: &str) -> bool {
    value.starts_with(PREFIX) || value.starts_with(LEGACY_PREFIX)
}

pub fn encrypt(value: &str) -> anyhow::Result<String> {
    init()?;
    let key = KEY_BYTES.get().context("secret key was not initialized")?;
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    let ciphertext = cipher
        .encrypt(&nonce, value.as_bytes())
        .map_err(|_| anyhow::anyhow!("failed to encrypt secret"))?;
    let mut payload = nonce.to_vec();
    payload.extend(ciphertext);
    Ok(format!(
        "{PREFIX}{}:{}",
        current_key_id()?,
        STANDARD.encode(payload)
    ))
}

pub fn decrypt(value: &str) -> anyhow::Result<String> {
    let encoded = if let Some(payload) = value.strip_prefix(PREFIX) {
        let (key_id, encoded) = payload
            .split_once(':')
            .ok_or_else(|| anyhow::anyhow!("invalid encrypted secret key identifier"))?;
        let current = current_key_id()?;
        if key_id != current {
            bail!("encrypted secret uses key id '{key_id}', but current key id is '{current}'");
        }
        encoded
    } else {
        value
            .strip_prefix(LEGACY_PREFIX)
            .ok_or_else(|| anyhow::anyhow!("unencrypted secret is not supported"))?
    };
    init()?;
    let payload = STANDARD
        .decode(encoded)
        .context("invalid encrypted secret encoding")?;
    if payload.len() <= 12 {
        bail!("invalid encrypted secret payload");
    }
    let (nonce, ciphertext) = payload.split_at(12);
    let key = KEY_BYTES.get().context("secret key was not initialized")?;
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    let plaintext = cipher
        .decrypt(nonce.into(), ciphertext)
        .map_err(|_| anyhow::anyhow!("failed to decrypt secret; check {KEY_ENV} or {KEY_PATH}"))?;
    String::from_utf8(plaintext).context("decrypted secret is not UTF-8")
}

fn current_key_id() -> anyhow::Result<String> {
    let value = std::env::var(KEY_ID_ENV).unwrap_or_else(|_| "primary".to_string());
    if value.is_empty()
        || value.len() > 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        bail!("{KEY_ID_ENV} must contain 1-64 ASCII letters, digits, '-' or '_'");
    }
    Ok(value)
}

fn decode_key(encoded: &str) -> anyhow::Result<[u8; 32]> {
    let bytes = STANDARD.decode(encoded)?;
    bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("secret key must be 32 bytes encoded as base64"))
}

fn load_or_create_key() -> anyhow::Result<[u8; 32]> {
    match fs::metadata(KEY_PATH) {
        Ok(_) => {
            ensure_private_key_permissions(KEY_PATH)?;
            let value = fs::read_to_string(KEY_PATH)?;
            return decode_key(value.trim());
        }
        Err(err) if err.kind() != std::io::ErrorKind::NotFound => return Err(err.into()),
        Err(_) => {}
    }

    let generated = Aes256Gcm::generate_key(&mut OsRng);
    let encoded = format!("{}\n", STANDARD.encode(generated.as_slice()));
    if let Some(parent) = std::path::Path::new(KEY_PATH).parent() {
        fs::create_dir_all(parent)?;
    }
    match write_private_key_file(KEY_PATH, encoded.as_bytes()) {
        Ok(()) => decode_key(encoded.trim()),
        Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
            ensure_private_key_permissions(KEY_PATH)?;
            let value = fs::read_to_string(KEY_PATH)?;
            decode_key(value.trim())
        }
        Err(err) => Err(err.into()),
    }
}

fn ensure_private_key_permissions(path: &str) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "secret key path is not a regular file",
        ));
    }
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
}

fn write_private_key_file(path: &str, content: &[u8]) -> std::io::Result<()> {
    use std::{io::Write, os::unix::fs::OpenOptionsExt};
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(content)?;
    file.sync_all()
}
