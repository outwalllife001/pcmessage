use crate::{model::*, storage::Store};
use base64::{engine::general_purpose::STANDARD, Engine};
use parking_lot::Mutex;
use std::{
    collections::HashMap,
    io::Cursor,
    net::Ipv4Addr,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

pub type Events = Arc<dyn Fn(&str, serde_json::Value) + Send + Sync>;
pub struct Seen {
    pub device: Device,
    pub address: String,
    pub at: Instant,
}
pub struct Pending {
    pub public: Pairing,
    pub device: Device,
    pub address: String,
    pub secret: String,
    pub token: Option<String>,
    pub at: Instant,
}
pub struct Core {
    pub local: Mutex<Device>,
    pub store: Mutex<Store>,
    pub seen: Mutex<HashMap<String, Seen>>,
    pub pending: Mutex<HashMap<String, Pending>>,
    pub root: PathBuf,
    pub events: Events,
    pub network_error: Mutex<Option<String>>,
    pub receiving: tokio::sync::Mutex<()>,
    pub sending: tokio::sync::Mutex<()>,
}
impl Core {
    pub fn create(root: PathBuf, port: u16, events: Events) -> Result<Arc<Self>, String> {
        std::fs::create_dir_all(root.join("assets")).map_err(|e| e.to_string())?;
        private_permissions(&root)?;
        let cert_path = root.join("certificate.pem");
        let key_path = root.join("private-key.pem");
        if !cert_path.exists() && !key_path.exists() {
            let certified = rcgen::generate_simple_self_signed(vec!["pcmessage.local".into()])
                .map_err(|e| e.to_string())?;
            atomic_write(&key_path, certified.key_pair.serialize_pem().as_bytes())?;
            private_permissions(&key_path)?;
            atomic_write(&cert_path, certified.cert.pem().as_bytes())?;
        }
        if !key_path.exists() || !cert_path.exists() {
            return Err("设备证书不完整，请恢复应用数据目录".into());
        }
        let certificate = std::fs::read_to_string(cert_path).map_err(|e| e.to_string())?;
        let der = pem::parse(&certificate).map_err(|e| e.to_string())?;
        let store = Store::open(&root.join("messages.sqlite"))?;
        store.recover_sending()?;
        let name = store.setting("name")?.unwrap_or_else(|| {
            hostname::get()
                .map(|n| n.to_string_lossy().chars().take(40).collect())
                .unwrap_or("我的电脑".into())
        });
        let local = Device {
            id: fingerprint(der.contents()),
            name,
            port,
            certificate,
            version: 1,
            platform: std::env::consts::OS.into(),
        };
        Ok(Arc::new(Self {
            local: Mutex::new(local),
            store: Mutex::new(store),
            seen: Mutex::new(HashMap::new()),
            pending: Mutex::new(HashMap::new()),
            root,
            events,
            network_error: Mutex::new(None),
            receiving: tokio::sync::Mutex::new(()),
            sending: tokio::sync::Mutex::new(()),
        }))
    }
    pub fn changed(&self) {
        (self.events)("changed", serde_json::Value::Null);
    }
    pub fn snapshot(&self) -> Result<Snapshot, String> {
        let trusted = self.store.lock().peers()?;
        let seen = self.seen.lock();
        let mut peers: HashMap<String, Peer> = trusted
            .iter()
            .map(|t| {
                (
                    t.device.id.clone(),
                    Peer {
                        device: t.device.clone(),
                        address: t.address.clone(),
                        online: false,
                        paired: true,
                        unread: self.store.lock().unread(&t.device.id),
                    },
                )
            })
            .collect();
        for (id, s) in seen.iter() {
            let paired = trusted
                .iter()
                .any(|t| t.device.id == *id && t.device.certificate == s.device.certificate);
            if paired || s.at.elapsed() < Duration::from_secs(18) {
                peers.insert(
                    id.clone(),
                    Peer {
                        device: s.device.clone(),
                        address: s.address.clone(),
                        online: s.at.elapsed() < Duration::from_secs(18),
                        paired,
                        unread: self.store.lock().unread(id),
                    },
                );
            }
        }
        let mut peers: Vec<_> = peers.into_values().collect();
        peers.sort_by(|a, b| {
            b.paired
                .cmp(&a.paired)
                .then(b.online.cmp(&a.online))
                .then(a.device.name.cmp(&b.device.name))
        });
        let mut pending = self.pending.lock();
        pending.retain(|_, p| p.at.elapsed() < Duration::from_secs(120));
        Ok(Snapshot {
            local: self.local.lock().clone(),
            addresses: local_addresses().iter().map(ToString::to_string).collect(),
            peers,
            pairings: pending
                .values()
                .filter(|p| !(p.public.incoming && p.public.confirmed))
                .map(|p| p.public.clone())
                .collect(),
            network_error: self.network_error.lock().clone(),
        })
    }
    pub fn see(&self, device: Device, address: Ipv4Addr) -> Result<(), String> {
        device.validate()?;
        if !private_ip(address) || device.id == self.local.lock().id {
            return Ok(());
        }
        let mut seen = self.seen.lock();
        if seen.len() >= 128 && !seen.contains_key(&device.id) {
            seen.retain(|_, s| s.at.elapsed() < Duration::from_secs(18));
            if seen.len() >= 128 {
                return Ok(());
            }
        }
        let changed = seen
            .get(&device.id)
            .map(|s| {
                s.at.elapsed() > Duration::from_secs(18)
                    || s.address != address.to_string()
                    || s.device.name != device.name
                    || s.device.port != device.port
            })
            .unwrap_or(true);
        seen.insert(
            device.id.clone(),
            Seen {
                device,
                address: address.to_string(),
                at: Instant::now(),
            },
        );
        drop(seen);
        if changed {
            self.changed();
        }
        Ok(())
    }
    pub fn trusted(&self, id: &str) -> Result<TrustedPeer, String> {
        let mut trusted = self
            .store
            .lock()
            .peers()?
            .into_iter()
            .find(|p| p.device.id == id)
            .ok_or("请先配对这台电脑")?;
        if let Some(s) = self.seen.lock().get(id) {
            trusted.address = s.address.clone();
            trusted.device.port = s.device.port;
            trusted.device.name = s.device.name.clone();
        }
        Ok(trusted)
    }
    pub fn peer(&self, id: &str) -> Result<(Device, String), String> {
        if let Some(s) = self.seen.lock().get(id) {
            return Ok((s.device.clone(), s.address.clone()));
        }
        let p = self.trusted(id)?;
        Ok((p.device, p.address))
    }
    pub fn rename(&self, name: &str) -> Result<(), String> {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 40 {
            return Err("电脑名需为 1–40 个字符".into());
        }
        self.store.lock().set_setting("name", name)?;
        self.local.lock().name = name.into();
        self.changed();
        Ok(())
    }
    pub fn stage(&self, bytes: &[u8], name: &str) -> Result<Attachment, String> {
        let mime = validate_image(bytes)?;
        let attachment = Attachment {
            id: uuid::Uuid::new_v4().to_string(),
            name: safe_name(name),
            mime,
            size: bytes.len(),
            hash: fingerprint(bytes),
        };
        self.save_image(&attachment, bytes)?;
        Ok(attachment)
    }
    pub fn save_image(&self, attachment: &Attachment, bytes: &[u8]) -> Result<(), String> {
        valid_id(&attachment.id)?;
        if bytes.len() != attachment.size
            || fingerprint(bytes) != attachment.hash
            || validate_image(bytes)? != attachment.mime
        {
            return Err("图片校验失败".into());
        }
        let path = self.root.join("assets").join(&attachment.id);
        if path.exists()
            && fingerprint(&std::fs::read(&path).map_err(|e| e.to_string())?) != attachment.hash
        {
            return Err("图片 ID 冲突".into());
        }
        atomic_write(&path, bytes)?;
        atomic_write(
            &path.with_extension("json"),
            &serde_json::to_vec(attachment).map_err(|e| e.to_string())?,
        )?;
        Ok(())
    }
    pub fn attachment(&self, id: &str) -> Result<Attachment, String> {
        valid_id(id)?;
        let path = self.root.join("assets").join(id).with_extension("json");
        serde_json::from_slice(&std::fs::read(path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())
    }
    pub fn image_bytes(&self, id: &str) -> Result<Vec<u8>, String> {
        valid_id(id)?;
        std::fs::read(self.root.join("assets").join(id)).map_err(|e| e.to_string())
    }
    pub fn image_url(&self, id: &str) -> Result<String, String> {
        let a = self.attachment(id)?;
        Ok(format!(
            "data:{};base64,{}",
            a.mime,
            STANDARD.encode(self.image_bytes(id)?)
        ))
    }
    pub async fn receive(
        &self,
        peer_id: &str,
        wire: WireMessage,
        images: HashMap<String, Vec<u8>>,
    ) -> Result<bool, String> {
        wire.validate()?;
        if images.len() != wire.images.len() {
            return Err("图片附件不完整".into());
        }
        // Serialise duplicate detection, attachment writes and durable insertion.
        let _guard = self.receiving.lock().await;
        let digest = wire.digest();
        if self.store.lock().duplicate(peer_id, &wire.id, &digest)? {
            return Ok(false);
        }
        for a in &wire.images {
            self.save_image(a, images.get(&a.id).ok_or("图片附件不完整")?)?;
        }
        let message = Message {
            id: wire.id,
            peer_id: peer_id.into(),
            text: wire.text,
            images: wire.images,
            created_at: now(),
            direction: "incoming".into(),
            status: "sent".into(),
            unread: true,
            delivery_error: None,
        };
        self.store.lock().insert(&message, &digest)?;
        self.changed();
        (self.events)("message-received", serde_json::json!({"peer_id":peer_id}));
        Ok(true)
    }
}
pub fn secret() -> String {
    format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    )
}
pub fn local_addresses() -> Vec<Ipv4Addr> {
    use network_interface::{Addr, NetworkInterface, NetworkInterfaceConfig};
    let mut addresses: Vec<_> = NetworkInterface::show()
        .unwrap_or_default()
        .into_iter()
        .flat_map(|n| n.addr)
        .filter_map(|a| match a {
            Addr::V4(v) if private_ip(v.ip) && !v.ip.is_loopback() => Some(v.ip),
            _ => None,
        })
        .collect();
    addresses.sort();
    addresses.dedup();
    addresses
}
pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    use std::io::Write;
    let temp = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .map_err(|e| e.to_string())?;
        f.write_all(bytes).map_err(|e| e.to_string())?;
        f.sync_all().map_err(|e| e.to_string())?;
        // Existing asset bytes are immutable. Windows rename cannot replace a file.
        if path.exists() {
            if std::fs::read(path).map_err(|e| e.to_string())? == bytes {
                return Ok(());
            }
            return Err("文件已存在且内容不同".into());
        }
        std::fs::rename(&temp, path).map_err(|e| e.to_string())?;
        Ok(())
    })();
    let _ = std::fs::remove_file(temp);
    result
}
pub fn private_permissions(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = if path.is_dir() { 0o700 } else { 0o600 };
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
            .map_err(|e| e.to_string())?;
    }
    let _ = path;
    Ok(())
}
fn safe_name(name: &str) -> String {
    let name = name.rsplit(['/', '\\']).next().unwrap_or("图片");
    let s: String = name.chars().filter(|c| !c.is_control()).take(120).collect();
    if s.is_empty() {
        "图片".into()
    } else {
        s
    }
}
pub fn validate_image(bytes: &[u8]) -> Result<String, String> {
    if bytes.is_empty() || bytes.len() > MAX_IMAGE {
        return Err("单张图片最多 15 MB".into());
    }
    let format = image::guess_format(bytes).map_err(|_| "支持 PNG、JPEG、GIF 和 WebP 图片")?;
    let mime = match format {
        image::ImageFormat::Png => "image/png",
        image::ImageFormat::Jpeg => "image/jpeg",
        image::ImageFormat::Gif => "image/gif",
        image::ImageFormat::WebP => "image/webp",
        _ => return Err("支持 PNG、JPEG、GIF 和 WebP 图片".into()),
    };
    let (w, h) = image::ImageReader::with_format(Cursor::new(bytes), format)
        .into_dimensions()
        .map_err(|_| "图片损坏")?;
    if w == 0 || h == 0 || u64::from(w) * u64::from(h) > 25_000_000 {
        return Err("图片尺寸过大（最多 2500 万像素）".into());
    }
    Ok(mime.into())
}
