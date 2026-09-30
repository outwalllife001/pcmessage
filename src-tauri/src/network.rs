use crate::{core::*, model::*};
use axum::{
    extract::{ConnectInfo, DefaultBodyLimit, Multipart, Path, State},
    http::{HeaderMap, StatusCode},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    net::{IpAddr, Ipv4Addr, SocketAddr, SocketAddrV4},
    sync::Arc,
    time::{Duration, Instant},
};
use subtle::ConstantTimeEq;

type ApiError = (StatusCode, String);
fn bad(e: impl ToString) -> ApiError {
    (StatusCode::BAD_REQUEST, e.to_string())
}
fn internal(e: impl ToString) -> ApiError {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}

pub struct Running {
    pub core: Arc<Core>,
    handle: axum_server::Handle,
    tasks: Vec<tokio::task::JoinHandle<()>>,
}
impl Drop for Running {
    fn drop(&mut self) {
        self.handle.shutdown();
        for task in &self.tasks {
            task.abort();
        }
    }
}
impl Running {
    pub async fn start(core: Arc<Core>, discovery: bool) -> Result<Self, String> {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let requested = core.local.lock().port;
        let listener = std::net::TcpListener::bind((Ipv4Addr::UNSPECIFIED, requested))
            .or_else(|e| {
                if e.kind() == std::io::ErrorKind::AddrInUse {
                    std::net::TcpListener::bind((Ipv4Addr::UNSPECIFIED, 0))
                } else {
                    Err(e)
                }
            })
            .map_err(|e| format!("无法启动局域网接收服务：{e}"))?;
        listener.set_nonblocking(true).map_err(|e| e.to_string())?;
        core.local.lock().port = listener.local_addr().map_err(|e| e.to_string())?.port();
        let tls = axum_server::tls_rustls::RustlsConfig::from_pem_file(
            core.root.join("certificate.pem"),
            core.root.join("private-key.pem"),
        )
        .await
        .map_err(|e| e.to_string())?;
        let routes = Router::new()
            .route("/v1/info", get(info))
            .route("/v1/pair", post(pair_request))
            .route("/v1/pair/{id}", get(pair_status))
            .route("/v1/message", post(receive))
            .layer(DefaultBodyLimit::max(MAX_TRANSFER + 1024 * 1024))
            .with_state(core.clone());
        let handle = axum_server::Handle::new();
        let server = axum_server::from_tcp_rustls(listener, tls).handle(handle.clone());
        let c = core.clone();
        let mut tasks = vec![tokio::spawn(async move {
            if let Err(e) = server
                .serve(routes.into_make_service_with_connect_info::<SocketAddr>())
                .await
            {
                *c.network_error.lock() = Some(format!("接收服务停止：{e}"));
                c.changed();
            }
        })];
        if discovery {
            match discovery_socket() {
                Ok(socket) => {
                    let socket = Arc::new(socket);
                    let rx = socket.clone();
                    let c = core.clone();
                    tasks.push(tokio::spawn(async move {
                        let mut buffer = [0u8; 8192];
                        loop {
                            match rx.recv_from(&mut buffer).await {
                                Ok((n, source)) => {
                                    if let IpAddr::V4(address) = source.ip() {
                                        if let Ok(device) =
                                            serde_json::from_slice::<Device>(&buffer[..n])
                                        {
                                            let _ = c.see(device, address);
                                        }
                                    }
                                }
                                Err(e) => {
                                    *c.network_error.lock() = Some(format!("设备发现失败：{e}"));
                                    c.changed();
                                    break;
                                }
                            }
                        }
                    }));
                    let c = core.clone();
                    tasks.push(tokio::spawn(async move {
                        let mut interval = tokio::time::interval(Duration::from_secs(5));
                        loop {
                            interval.tick().await;
                            let packet = serde_json::to_vec(&*c.local.lock()).unwrap_or_default();
                            let destination = SocketAddrV4::new(GROUP, DISCOVERY_PORT);
                            let _ = socket.send_to(&packet, destination).await;
                            // Send and join on all active LAN interfaces, including Ethernet + Wi-Fi.
                            for address in local_addresses() {
                                let _ = socket.join_multicast_v4(GROUP, address);
                                if let Ok(tx) = std::net::UdpSocket::bind((address, 0)) {
                                    let _ = tx.set_multicast_ttl_v4(1);
                                    let _ = tx.send_to(&packet, destination);
                                }
                            }
                            // Probe remembered peers over pinned HTTPS; multicast is an optimisation.
                            let peers = c.store.lock().peers().unwrap_or_default();
                            for peer in peers {
                                let c = c.clone();
                                tokio::spawn(async move {
                                    if let Ok(client) = client(&peer.device, &peer.address) {
                                        if let Ok(response) = client
                                            .get(url(peer.device.port, "/v1/info"))
                                            .send()
                                            .await
                                        {
                                            if let Ok(info) = read_json::<Device>(response).await {
                                                if info.id == peer.device.id {
                                                    if let Ok(address) = peer.address.parse() {
                                                        let _ = c.see(info, address);
                                                    }
                                                }
                                            }
                                        }
                                    }
                                });
                            }
                            c.changed();
                        }
                    }));
                }
                Err(e) => {
                    *core.network_error.lock() =
                        Some(format!("自动发现不可用，可用 IP 添加电脑：{e}"));
                    core.changed();
                }
            }
        }
        Ok(Self {
            core,
            handle,
            tasks,
        })
    }
}
fn discovery_socket() -> Result<tokio::net::UdpSocket, String> {
    use socket2::{Domain, Protocol, Socket, Type};
    let socket =
        Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP)).map_err(|e| e.to_string())?;
    socket.set_reuse_address(true).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    socket.set_reuse_port(true).map_err(|e| e.to_string())?;
    socket
        .bind(&SocketAddr::from((Ipv4Addr::UNSPECIFIED, DISCOVERY_PORT)).into())
        .map_err(|e| e.to_string())?;
    socket.set_multicast_ttl_v4(1).map_err(|e| e.to_string())?;
    socket
        .join_multicast_v4(&GROUP, &Ipv4Addr::UNSPECIFIED)
        .map_err(|e| e.to_string())?;
    socket.set_nonblocking(true).map_err(|e| e.to_string())?;
    tokio::net::UdpSocket::from_std(socket.into()).map_err(|e| e.to_string())
}
pub fn client(device: &Device, address: &str) -> Result<reqwest::Client, String> {
    device.validate()?;
    let address: Ipv4Addr = address.parse().map_err(|_| "请输入局域网 IPv4 地址")?;
    if !private_ip(address) {
        return Err("仅支持局域网地址".into());
    }
    let certificate =
        reqwest::Certificate::from_pem(device.certificate.as_bytes()).map_err(|e| e.to_string())?;
    reqwest::Client::builder()
        .no_proxy()
        .https_only(true)
        .tls_built_in_root_certs(false)
        .add_root_certificate(certificate)
        .resolve("pcmessage.local", SocketAddr::from((address, device.port)))
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(3))
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())
}
fn url(port: u16, path: &str) -> String {
    format!("https://pcmessage.local:{port}{path}")
}
async fn read_json<T: serde::de::DeserializeOwned>(
    mut response: reqwest::Response,
) -> Result<T, String> {
    if !response.status().is_success() {
        return Err(format!("对方返回 {}", response.status()));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
        if bytes.len() + chunk.len() > 8192 {
            return Err("设备响应过大".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).map_err(|e| e.to_string())
}
async fn info(State(c): State<Arc<Core>>) -> Json<Device> {
    Json(c.local.lock().clone())
}
pub async fn add_address(c: &Arc<Core>, value: &str) -> Result<String, String> {
    let (address, port) = if let Ok(address) = value.trim().parse::<Ipv4Addr>() {
        (address, DEFAULT_PORT)
    } else {
        let socket = value
            .trim()
            .parse::<SocketAddrV4>()
            .map_err(|_| "请输入 IP 或 IP:端口，例如 192.168.1.20")?;
        (*socket.ip(), socket.port())
    };
    if !private_ip(address) || port == 0 {
        return Err("请输入局域网地址".into());
    }
    // Bootstrap only: no credentials or messages are sent on this unverified connection.
    // The subsequent visual pairing code binds the certificate fingerprints on both PCs.
    let bootstrap = reqwest::Client::builder()
        .no_proxy()
        .danger_accept_invalid_certs(true)
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(3))
        .timeout(Duration::from_secs(5))
        .build()
        .map_err(|e| e.to_string())?;
    let device: Device = read_json(
        bootstrap
            .get(format!("https://{address}:{port}/v1/info"))
            .send()
            .await
            .map_err(|_| "连接失败，请检查 IP、端口和防火墙")?,
    )
    .await?;
    device.validate()?;
    if device.id == c.local.lock().id {
        return Err("这是当前电脑".into());
    }
    if device.port != port {
        return Err("对方返回的端口不一致".into());
    }
    // Verify that the fetched certificate actually belongs to the reachable server.
    let verified: Device = read_json(
        client(&device, &address.to_string())?
            .get(url(port, "/v1/info"))
            .send()
            .await
            .map_err(|e| e.to_string())?,
    )
    .await?;
    if verified.id != device.id {
        return Err("设备身份发生变化，请重试".into());
    }
    let id = device.id.clone();
    c.see(device, address)?;
    Ok(id)
}
#[derive(Serialize, Deserialize)]
pub struct PairRequest {
    pub id: String,
    pub device: Device,
    pub secret: String,
}
#[derive(Serialize, Deserialize)]
struct PairReply {
    status: String,
    token: Option<String>,
}
async fn pair_request(
    State(c): State<Arc<Core>>,
    ConnectInfo(remote): ConnectInfo<SocketAddr>,
    Json(request): Json<PairRequest>,
) -> Result<StatusCode, ApiError> {
    valid_id(&request.id).map_err(bad)?;
    request.device.validate().map_err(bad)?;
    if request.secret.len() != 64 || !request.secret.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(bad("配对请求无效"));
    }
    let address = match remote.ip() {
        IpAddr::V4(address) if private_ip(address) => address,
        _ => return Err(bad("仅支持局域网")),
    };
    let local = c.local.lock().clone();
    if request.device.id == local.id {
        return Err(bad("不能与自己配对"));
    }
    c.see(request.device.clone(), address).map_err(bad)?;
    let mut pending = c.pending.lock();
    pending.retain(|_, p| p.at.elapsed() < Duration::from_secs(120));
    if let Some(existing) = pending.get(&request.id) {
        if existing.secret == request.secret && existing.device.id == request.device.id {
            return Ok(StatusCode::ACCEPTED);
        }
        return Err(bad("配对 ID 冲突"));
    }
    if pending.len() >= 8 {
        return Err((StatusCode::TOO_MANY_REQUESTS, "配对请求过多".into()));
    }
    if pending
        .values()
        .any(|p| p.device.id == request.device.id && !p.public.confirmed)
    {
        return Err((
            StatusCode::CONFLICT,
            "已有配对请求，请只在一台电脑上发起配对".into(),
        ));
    }
    let code = pair_code(&local.id, &request.device.id, &request.secret);
    pending.insert(
        request.id.clone(),
        Pending {
            public: Pairing {
                id: request.id,
                peer_id: request.device.id.clone(),
                name: request.device.name.clone(),
                code,
                incoming: true,
                confirmed: false,
            },
            device: request.device,
            address: address.to_string(),
            secret: request.secret,
            token: None,
            at: Instant::now(),
        },
    );
    drop(pending);
    c.changed();
    Ok(StatusCode::ACCEPTED)
}
async fn pair_status(
    State(c): State<Arc<Core>>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<PairReply>, ApiError> {
    let secret = header(&headers, "x-pair-secret")?;
    let pending = c.pending.lock();
    let p = pending
        .get(&id)
        .filter(|p| {
            p.public.incoming
                && p.at.elapsed() < Duration::from_secs(120)
                && bool::from(secret.as_bytes().ct_eq(p.secret.as_bytes()))
        })
        .ok_or((StatusCode::NOT_FOUND, "配对请求已结束".into()))?;
    let status = if p.public.confirmed {
        if p.token.is_some() {
            "accepted"
        } else {
            "rejected"
        }
    } else {
        "waiting"
    };
    Ok(Json(PairReply {
        status: status.into(),
        token: p.token.clone(),
    }))
}
pub async fn begin_pair(c: Arc<Core>, peer_id: &str) -> Result<String, String> {
    let (device, address) = c.peer(peer_id)?;
    let request = PairRequest {
        id: uuid::Uuid::new_v4().to_string(),
        device: c.local.lock().clone(),
        secret: secret(),
    };
    let code = pair_code(&request.device.id, &device.id, &request.secret);
    let request_id = request.id.clone();
    {
        let mut pending = c.pending.lock();
        pending.retain(|_, p| p.at.elapsed() < Duration::from_secs(120));
        if pending.values().any(|p| p.device.id == device.id) {
            return Err("这台电脑已有配对请求".into());
        }
        if pending.len() >= 8 {
            return Err("配对请求过多".into());
        }
        pending.insert(
            request.id.clone(),
            Pending {
                public: Pairing {
                    id: request.id.clone(),
                    peer_id: device.id.clone(),
                    name: device.name.clone(),
                    code,
                    incoming: false,
                    confirmed: false,
                },
                device: device.clone(),
                address: address.clone(),
                secret: request.secret.clone(),
                token: None,
                at: Instant::now(),
            },
        );
    }
    let client = client(&device, &address)?;
    let response = client
        .post(url(device.port, "/v1/pair"))
        .json(&request)
        .send()
        .await;
    if !matches!(&response,Ok(r) if r.status().is_success()) {
        c.pending.lock().remove(&request.id);
        c.changed();
        return Err("配对请求发送失败，请检查对方是否在线".into());
    }
    c.changed();
    let poll_id = request.id.clone();
    tokio::spawn(async move {
        for _ in 0..120 {
            tokio::time::sleep(Duration::from_secs(1)).await;
            if !c.pending.lock().contains_key(&poll_id) {
                return;
            }
            let result = client
                .get(url(device.port, &format!("/v1/pair/{poll_id}")))
                .header("x-pair-secret", &request.secret)
                .send()
                .await;
            if let Ok(response) = result {
                if let Ok(reply) = read_json::<PairReply>(response).await {
                    if reply.status == "rejected" {
                        c.pending.lock().remove(&poll_id);
                        c.changed();
                        (c.events)("notice", serde_json::json!("对方已取消配对"));
                        return;
                    }
                    if reply.status == "accepted" {
                        if let Some(token) = reply.token {
                            if token.len() == 64 {
                                let confirmed = c
                                    .pending
                                    .lock()
                                    .get(&poll_id)
                                    .map(|p| p.public.confirmed)
                                    .unwrap_or(false);
                                if confirmed {
                                    let peer = TrustedPeer {
                                        device: device.clone(),
                                        address: address.clone(),
                                        token,
                                    };
                                    let result = c.store.lock().trust(&peer);
                                    if let Err(e) = result {
                                        (c.events)(
                                            "notice",
                                            serde_json::json!(format!("无法保存配对：{e}")),
                                        );
                                    }
                                    c.pending.lock().remove(&poll_id);
                                    c.changed();
                                    return;
                                }
                            }
                        }
                    }
                }
            }
        }
        c.pending.lock().remove(&poll_id);
        c.changed();
        (c.events)("notice", serde_json::json!("配对超时，请重试"));
    });
    Ok(request_id)
}
pub fn confirm_pair(c: &Arc<Core>, id: &str, accept: bool) -> Result<(), String> {
    let mut pending = c.pending.lock();
    let p = pending
        .get_mut(id)
        .filter(|p| p.at.elapsed() < Duration::from_secs(120))
        .ok_or("配对请求已过期")?;
    if !accept {
        if p.public.incoming {
            p.public.confirmed = true;
            p.token = None;
        } else {
            pending.remove(id);
        }
        drop(pending);
        c.changed();
        return Ok(());
    }
    if p.public.incoming && !p.public.confirmed {
        let token = secret();
        c.store.lock().trust(&TrustedPeer {
            device: p.device.clone(),
            address: p.address.clone(),
            token: token.clone(),
        })?;
        p.token = Some(token);
    }
    p.public.confirmed = true;
    drop(pending);
    c.changed();
    Ok(())
}
fn header<'a>(headers: &'a HeaderMap, name: &str) -> Result<&'a str, ApiError> {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .ok_or((StatusCode::UNAUTHORIZED, "缺少设备凭证".into()))
}
fn authenticate(c: &Core, headers: &HeaderMap) -> Result<String, ApiError> {
    let id = header(headers, "x-peer-id")?;
    let token = header(headers, "x-peer-token")?;
    let trusted = c
        .trusted(id)
        .map_err(|_| (StatusCode::UNAUTHORIZED, "设备尚未配对".into()))?;
    if !bool::from(token.as_bytes().ct_eq(trusted.token.as_bytes())) {
        return Err((StatusCode::UNAUTHORIZED, "设备凭证无效，请重新配对".into()));
    }
    Ok(id.into())
}
async fn receive(
    State(c): State<Arc<Core>>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Result<StatusCode, ApiError> {
    let peer = authenticate(&c, &headers)?;
    let mut wire = None;
    let mut images = HashMap::new();
    let mut total = 0;
    while let Some(field) = multipart.next_field().await.map_err(bad)? {
        let name = field.name().unwrap_or("").to_owned();
        if name == "message" {
            if wire.is_some() {
                return Err(bad("消息正文重复"));
            }
            let bytes = field.bytes().await.map_err(bad)?;
            if bytes.len() > MAX_TEXT + 16384 {
                return Err(bad("消息正文过大"));
            }
            let message: WireMessage = serde_json::from_slice(&bytes).map_err(bad)?;
            message.validate().map_err(bad)?;
            total += message.text.len();
            wire = Some(message);
        } else {
            valid_id(&name).map_err(bad)?;
            if images.len() >= MAX_IMAGES || images.contains_key(&name) {
                return Err(bad("图片附件数量无效"));
            }
            let bytes = field.bytes().await.map_err(bad)?;
            if bytes.len() > MAX_IMAGE {
                return Err(bad("单张图片最多 15 MB"));
            }
            total += bytes.len();
            images.insert(name, bytes.to_vec());
        }
        if total > MAX_TRANSFER {
            return Err(bad("一条消息最多 32 MB"));
        }
    }
    c.receive(&peer, wire.ok_or_else(|| bad("缺少消息正文"))?, images)
        .await
        .map_err(internal)?;
    Ok(StatusCode::OK)
}
pub async fn send(
    c: Arc<Core>,
    peer_id: String,
    text: String,
    image_ids: Vec<String>,
) -> Result<Message, String> {
    let images: Vec<_> = image_ids
        .iter()
        .map(|id| c.attachment(id))
        .collect::<Result<_, _>>()?;
    let wire = WireMessage {
        id: uuid::Uuid::new_v4().to_string(),
        text,
        images,
        created_at: now(),
    };
    wire.validate()?;
    c.trusted(&peer_id)?;
    let mut message = Message {
        id: wire.id.clone(),
        peer_id: peer_id.clone(),
        text: wire.text.clone(),
        images: wire.images.clone(),
        created_at: wire.created_at,
        direction: "outgoing".into(),
        status: "sending".into(),
        unread: false,
        delivery_error: None,
    };
    c.store.lock().insert(&message, &wire.digest())?;
    c.changed();
    let result = transmit(&c, &peer_id, &wire).await;
    message.delivery_error = result.as_ref().err().cloned();
    message.status = if result.is_ok() { "sent" } else { "failed" }.into();
    c.store.lock().status(
        &peer_id,
        &message.id,
        &message.status,
        message.delivery_error.as_deref(),
    )?;
    c.changed();
    // A failed transmission is still a saved message with its original ID for retry.
    Ok(message)
}
pub async fn retry(c: Arc<Core>, peer_id: String, id: String) -> Result<Message, String> {
    let mut message = c.store.lock().outgoing(&peer_id, &id)?;
    if message.status == "sent" {
        return Ok(message);
    }
    c.store.lock().status(&peer_id, &id, "sending", None)?;
    c.changed();
    let wire = WireMessage {
        id: message.id.clone(),
        text: message.text.clone(),
        images: message.images.clone(),
        created_at: message.created_at,
    };
    let result = transmit(&c, &peer_id, &wire).await;
    message.delivery_error = result.as_ref().err().cloned();
    message.status = if result.is_ok() { "sent" } else { "failed" }.into();
    c.store.lock().status(
        &peer_id,
        &id,
        &message.status,
        message.delivery_error.as_deref(),
    )?;
    c.changed();
    Ok(message)
}
async fn transmit(c: &Arc<Core>, peer_id: &str, wire: &WireMessage) -> Result<(), String> {
    let _guard = c.sending.lock().await;
    let peer = c.trusted(peer_id)?;
    let client = client(&peer.device, &peer.address)?;
    let mut form = reqwest::multipart::Form::new().text(
        "message",
        serde_json::to_string(wire).map_err(|e| e.to_string())?,
    );
    for image in &wire.images {
        let bytes = c.image_bytes(&image.id)?;
        if fingerprint(&bytes) != image.hash {
            return Err("本地图片损坏".into());
        }
        let part = reqwest::multipart::Part::bytes(bytes)
            .file_name(image.name.clone())
            .mime_str(&image.mime)
            .map_err(|e| e.to_string())?;
        form = form.part(image.id.clone(), part);
    }
    let local_id = c.local.lock().id.clone();
    let response = client
        .post(url(peer.device.port, "/v1/message"))
        .header("x-peer-id", local_id)
        .header("x-peer-token", &peer.token)
        .multipart(form)
        .send()
        .await
        .map_err(|e| {
            let mut details = e.to_string();
            let mut source = std::error::Error::source(&e);
            while let Some(cause) = source {
                details.push_str(&format!("：{cause}"));
                source = cause.source();
            }
            #[cfg(target_os = "macos")]
            if details.contains("os error 65") || details.contains("os error 13") {
                return format!(
                    "无法连接 {}:{}。请检查 Mac 的“本地网络”权限；若已开启，可关闭后重新开启，再重启 PCMessage。",
                    peer.address, peer.device.port
                );
            }
            if e.is_timeout() {
                return format!("连接 {}:{} 超时，请检查对方 PCMessage 和防火墙。", peer.address, peer.device.port);
            }
            format!("无法连接 {}:{}：{details}", peer.address, peer.device.port)
        })?;
    if !response.status().is_success() {
        return Err(format!("发送失败：{}", response.status()));
    }
    Ok(())
}
