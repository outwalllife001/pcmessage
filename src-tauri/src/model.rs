use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::net::Ipv4Addr;

pub const DEFAULT_PORT: u16 = 47321;
pub const DISCOVERY_PORT: u16 = 47322;
pub const GROUP: Ipv4Addr = Ipv4Addr::new(239, 255, 47, 32);
pub const MAX_IMAGE: usize = 15 * 1024 * 1024;
pub const MAX_TRANSFER: usize = 32 * 1024 * 1024;
pub const MAX_TEXT: usize = 256 * 1024;
pub const MAX_IMAGES: usize = 8;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub port: u16,
    pub certificate: String,
    pub version: u8,
    pub platform: String,
}
impl Device {
    pub fn validate(&self) -> Result<(), String> {
        if self.version != 1
            || self.port == 0
            || self.name.trim().is_empty()
            || self.name.chars().count() > 40
            || self.certificate.len() > 4096
        {
            return Err("设备信息无效".into());
        }
        reqwest::Certificate::from_pem(self.certificate.as_bytes()).map_err(|_| "设备证书无效")?;
        let cert = pem::parse(&self.certificate).map_err(|_| "设备证书无效")?;
        if fingerprint(cert.contents()) != self.id {
            return Err("设备身份与证书不一致".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Peer {
    #[serde(flatten)]
    pub device: Device,
    pub address: String,
    pub online: bool,
    pub paired: bool,
    pub unread: usize,
}
#[derive(Debug, Clone)]
pub struct TrustedPeer {
    pub device: Device,
    pub address: String,
    pub token: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Attachment {
    pub id: String,
    pub name: String,
    pub mime: String,
    pub size: usize,
    pub hash: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    pub peer_id: String,
    pub text: String,
    pub images: Vec<Attachment>,
    pub created_at: i64,
    pub direction: String,
    pub status: String,
    pub unread: bool,
    #[serde(default)]
    pub delivery_error: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireMessage {
    pub id: String,
    pub text: String,
    pub images: Vec<Attachment>,
    pub created_at: i64,
}
impl WireMessage {
    pub fn validate(&self) -> Result<(), String> {
        valid_id(&self.id)?;
        if self.text.len() > MAX_TEXT
            || self.images.len() > MAX_IMAGES
            || (self.text.trim().is_empty() && self.images.is_empty())
        {
            return Err("消息为空或超出大小限制".into());
        }
        let mut total = self.text.len();
        let mut ids = std::collections::HashSet::new();
        for image in &self.images {
            valid_id(&image.id)?;
            if !ids.insert(&image.id)
                || image.size > MAX_IMAGE
                || image.size == 0
                || image.name.chars().count() > 120
                || image.hash.len() != 64
                || !image.hash.bytes().all(|b| b.is_ascii_hexdigit())
                || !["image/png", "image/jpeg", "image/gif", "image/webp"]
                    .contains(&image.mime.as_str())
            {
                return Err("图片信息无效".into());
            }
            total = total.checked_add(image.size).ok_or("图片过大")?;
        }
        if total > MAX_TRANSFER {
            return Err("一条消息最多 32 MB".into());
        }
        Ok(())
    }
    pub fn digest(&self) -> String {
        fingerprint(&serde_json::to_vec(self).expect("serializable message"))
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pairing {
    pub id: String,
    pub peer_id: String,
    pub name: String,
    pub code: String,
    pub incoming: bool,
    pub confirmed: bool,
}
#[derive(Debug, Clone, Serialize)]
pub struct Snapshot {
    pub local: Device,
    pub addresses: Vec<String>,
    pub peers: Vec<Peer>,
    pub pairings: Vec<Pairing>,
    pub network_error: Option<String>,
}
pub fn valid_id(id: &str) -> Result<(), String> {
    let parsed = uuid::Uuid::parse_str(id).map_err(|_| "图片或消息 ID 无效")?;
    if parsed.to_string() != id {
        return Err("图片或消息 ID 格式无效".into());
    }
    Ok(())
}
pub fn fingerprint(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
pub fn pair_code(a: &str, b: &str, request: &str) -> String {
    let (a, b) = if a < b { (a, b) } else { (b, a) };
    let digest = Sha256::digest(format!("pcmessage-v1:{a}:{b}:{request}"));
    let n = u32::from_be_bytes(digest[..4].try_into().unwrap()) % 1_000_000;
    format!("{n:06}")
}
pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
pub fn private_ip(ip: Ipv4Addr) -> bool {
    ip.is_private() || ip.is_loopback() || ip.is_link_local()
}
