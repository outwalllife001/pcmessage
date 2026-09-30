use pcmessage_lib::{
    core::{Core, Events},
    model::*,
    network::{self, Running},
};
use std::{sync::Arc, time::Duration};

fn events() -> Events {
    Arc::new(|_, _| {})
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn removed_devices_stay_hidden_revoke_access_and_keep_history_for_repairing() {
    let ar = tempfile::tempdir().unwrap();
    let br = tempfile::tempdir().unwrap();
    let a = Core::create(ar.path().into(), 0, events()).unwrap();
    let b = Core::create(br.path().into(), 0, events()).unwrap();
    let a_server = Running::start(a.clone(), false).await.unwrap();
    let _b_server = Running::start(b.clone(), false).await.unwrap();
    ready(&a).await;
    ready(&b).await;
    pair(&a, &b).await;
    let aid = a.local.lock().id.clone();
    let bid = b.local.lock().id.clone();
    let saved_peer = a.trusted(&bid).unwrap();
    let attachment = b.stage(b"keep this file", "history.txt").unwrap();
    let sent = network::send(
        b.clone(),
        aid.clone(),
        "保留历史".into(),
        vec![attachment.id.clone()],
    )
    .await
    .unwrap();
    assert_eq!(sent.status, "sent");
    a.forget(&bid).unwrap();
    assert!(a.trusted(&bid).is_err());
    assert!(a.store.lock().trust(&saved_peer).is_err());
    // Discovery and a probe already in flight must not resurrect the removed entry.
    a.see(b.local.lock().clone(), std::net::Ipv4Addr::LOCALHOST)
        .unwrap();
    assert!(a.snapshot().unwrap().peers.is_empty());
    assert!(a.snapshot().unwrap().pairings.is_empty());
    assert!(a.peer(&bid).is_err());
    let failed = network::send(b.clone(), aid.clone(), "撤销后不能发送".into(), vec![])
        .await
        .unwrap();
    assert_eq!(failed.status, "failed");
    let device = a.local.lock().clone();
    let requester = b.local.lock().clone();
    let response = network::client(&device, "127.0.0.1")
        .unwrap()
        .post(format!("https://pcmessage.local:{}/v1/pair", device.port))
        .json(&network::PairRequest {
            id: uuid::Uuid::new_v4().to_string(),
            device: requester,
            secret: "a".repeat(64),
        })
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 403);
    drop(a_server);
    drop(a);
    let restored = Core::create(ar.path().into(), 0, events()).unwrap();
    let _restored_server = Running::start(restored.clone(), false).await.unwrap();
    ready(&restored).await;
    restored
        .see(b.local.lock().clone(), std::net::Ipv4Addr::LOCALHOST)
        .unwrap();
    assert!(restored.snapshot().unwrap().peers.is_empty());
    assert_eq!(
        restored.store.lock().messages(&bid, 100).unwrap()[0].id,
        sent.id
    );
    assert_eq!(
        restored.asset_bytes(&attachment.id).unwrap(),
        b"keep this file"
    );
    let bport = b.local.lock().port;
    network::add_address(&restored, &format!("127.0.0.1:{bport}"))
        .await
        .unwrap();
    assert_eq!(restored.snapshot().unwrap().peers.len(), 1);
    assert!(!restored.snapshot().unwrap().peers[0].paired);
    pair(&restored, &b).await;
    let reply = network::send(
        restored.clone(),
        bid.clone(),
        "重新配对后发送".into(),
        vec![],
    )
    .await
    .unwrap();
    assert_eq!(reply.status, "sent");
    assert_eq!(restored.store.lock().messages(&bid, 100).unwrap().len(), 2);
}
async fn ready(core: &Arc<Core>) {
    let device = core.local.lock().clone();
    let client = network::client(&device, "127.0.0.1").unwrap();
    for _ in 0..100 {
        if client
            .get(format!("https://pcmessage.local:{}/v1/info", device.port))
            .send()
            .await
            .is_ok()
        {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("HTTPS server did not start");
}
fn png() -> Vec<u8> {
    let image = image::RgbaImage::from_pixel(2, 2, image::Rgba([29, 114, 82, 255]));
    let mut output = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image)
        .write_to(&mut output, image::ImageFormat::Png)
        .unwrap();
    output.into_inner()
}
async fn pair(a: &Arc<Core>, b: &Arc<Core>) {
    let port = b.local.lock().port;
    let id = network::add_address(a, &format!("127.0.0.1:{port}"))
        .await
        .unwrap();
    let request = network::begin_pair(a.clone(), &id).await.unwrap();
    let acode = a.snapshot().unwrap().pairings[0].code.clone();
    let bcode = b.snapshot().unwrap().pairings[0].code.clone();
    assert_eq!(acode, bcode);
    // Neither side trusts an unconfirmed request.
    assert!(a.trusted(&id).is_err());
    network::confirm_pair(a, &request, true).unwrap();
    network::confirm_pair(b, &request, true).unwrap();
    for _ in 0..100 {
        if a.trusted(&id).is_ok() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(30)).await;
    }
    panic!("pairing did not complete");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn encrypted_pairing_text_images_dedup_restart_and_retry() {
    let ar = tempfile::tempdir().unwrap();
    let br = tempfile::tempdir().unwrap();
    let a = Core::create(ar.path().into(), 0, events()).unwrap();
    let b = Core::create(br.path().into(), 0, events()).unwrap();
    let _a_server = Running::start(a.clone(), false).await.unwrap();
    let b_server = Running::start(b.clone(), false).await.unwrap();
    ready(&a).await;
    ready(&b).await;
    pair(&a, &b).await;
    let aid = a.local.lock().id.clone();
    let bid = b.local.lock().id.clone();
    let bport = b.local.lock().port;
    let image = a.stage(&png(), "截图.png").unwrap();
    let text = "# 中文消息\n\n**你好** 🏠\n\n```rust\nfn main() {}\n```\n";
    let sent = network::send(a.clone(), bid.clone(), text.into(), vec![image.id.clone()])
        .await
        .unwrap();
    assert_eq!(sent.status, "sent");
    let received = b.store.lock().messages(&aid, 100).unwrap();
    assert_eq!(received.len(), 1);
    assert_eq!(received[0].text, text);
    assert_eq!(b.asset_bytes(&image.id).unwrap(), png());
    assert_eq!(b.store.lock().unread(&aid), 1);
    b.store.lock().mark_read(&aid).unwrap();
    assert_eq!(b.store.lock().unread(&aid), 0);
    // Replay after a hypothetical lost ACK. It must not append a duplicate.
    let trusted = a.trusted(&bid).unwrap();
    let client = network::client(&trusted.device, &trusted.address).unwrap();
    let wire = WireMessage {
        id: sent.id.clone(),
        text: sent.text.clone(),
        images: sent.images.clone(),
        created_at: sent.created_at,
    };
    for _ in 0..2 {
        let form = reqwest::multipart::Form::new()
            .text("message", serde_json::to_string(&wire).unwrap())
            .part(
                image.id.clone(),
                reqwest::multipart::Part::bytes(png()).file_name("截图.png"),
            );
        let response = client
            .post(format!("https://pcmessage.local:{bport}/v1/message"))
            .header("x-peer-id", &aid)
            .header("x-peer-token", &trusted.token)
            .multipart(form)
            .send()
            .await
            .unwrap();
        assert!(response.status().is_success());
    }
    assert_eq!(b.store.lock().messages(&aid, 100).unwrap().len(), 1);
    let reply = network::send(b.clone(), aid.clone(), "Mac → Windows → Mac".into(), vec![])
        .await
        .unwrap();
    assert_eq!(reply.status, "sent");
    assert_eq!(a.store.lock().messages(&bid, 100).unwrap().len(), 2);
    drop(b_server);
    drop(b);
    tokio::time::sleep(Duration::from_millis(30)).await;
    let failed = network::send(a.clone(), bid.clone(), "离线时保留并重试".into(), vec![])
        .await
        .unwrap();
    assert_eq!(failed.status, "failed");
    assert!(failed
        .delivery_error
        .as_deref()
        .unwrap()
        .contains("127.0.0.1"));
    assert_eq!(
        a.store
            .lock()
            .outgoing(&bid, &failed.id)
            .unwrap()
            .delivery_error,
        failed.delivery_error
    );
    let restored = Core::create(br.path().into(), bport, events()).unwrap();
    let _b_server = Running::start(restored.clone(), false).await.unwrap();
    ready(&restored).await;
    assert_eq!(restored.local.lock().id, bid);
    assert_eq!(restored.store.lock().messages(&aid, 100).unwrap().len(), 2);
    assert!(restored.trusted(&aid).is_ok());
    let retried = network::retry(a.clone(), bid.clone(), failed.id.clone())
        .await
        .unwrap();
    assert_eq!(retried.status, "sent");
    assert!(retried.delivery_error.is_none());
    assert_eq!(retried.id, failed.id);
    assert_eq!(restored.store.lock().messages(&aid, 100).unwrap().len(), 3);
}

#[tokio::test]
async fn unpaired_spoofed_and_tampered_payloads_are_rejected() {
    let ar = tempfile::tempdir().unwrap();
    let br = tempfile::tempdir().unwrap();
    let a = Core::create(ar.path().into(), 0, events()).unwrap();
    let b = Core::create(br.path().into(), 0, events()).unwrap();
    let _a = Running::start(a.clone(), false).await.unwrap();
    let _b = Running::start(b.clone(), false).await.unwrap();
    ready(&b).await;
    let device = b.local.lock().clone();
    let client = network::client(&device, "127.0.0.1").unwrap();
    let endpoint = format!("https://pcmessage.local:{}/v1/message", device.port);
    let wire = WireMessage {
        id: uuid::Uuid::new_v4().to_string(),
        text: "unauthorised".into(),
        images: vec![],
        created_at: now(),
    };
    let unpaired_id = a.local.lock().id.clone();
    let response = client
        .post(&endpoint)
        .header("x-peer-id", unpaired_id)
        .header("x-peer-token", "wrong")
        .multipart(
            reqwest::multipart::Form::new().text("message", serde_json::to_string(&wire).unwrap()),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 401);
    pair(&a, &b).await;
    let aid = a.local.lock().id.clone();
    let bid = b.local.lock().id.clone();
    let trusted = a.trusted(&bid).unwrap();
    let attachment = a.stage(&png(), "test.png").unwrap();
    let corrupt = WireMessage {
        id: uuid::Uuid::new_v4().to_string(),
        text: "corrupt image".into(),
        images: vec![attachment.clone()],
        created_at: now(),
    };
    let form = reqwest::multipart::Form::new()
        .text("message", serde_json::to_string(&corrupt).unwrap())
        .part(
            attachment.id,
            reqwest::multipart::Part::bytes(vec![1, 2, 3]),
        );
    let response = client
        .post(&endpoint)
        .header("x-peer-id", &aid)
        .header("x-peer-token", &trusted.token)
        .multipart(form)
        .send()
        .await
        .unwrap();
    assert!(!response.status().is_success());
    assert!(b.store.lock().messages(&aid, 100).unwrap().is_empty());
    // A different self-signed identity cannot be used as the trusted endpoint certificate.
    let wrong = network::client(&a.local.lock().clone(), "127.0.0.1").unwrap();
    assert!(wrong
        .get(format!("https://pcmessage.local:{}/v1/info", device.port))
        .send()
        .await
        .is_err());
    assert!(network::add_address(&a, "8.8.8.8").await.is_err());
    assert!(a.asset_bytes("../../private-key.pem").is_err());
}

#[test]
fn bounded_messages_images_and_identity() {
    let root = tempfile::tempdir().unwrap();
    let c = Core::create(root.path().into(), 47321, events()).unwrap();
    c.local.lock().validate().unwrap();
    let mut forged = c.local.lock().clone();
    forged.id = "a".repeat(64);
    assert!(forged.validate().is_err());
    let wire = WireMessage {
        id: uuid::Uuid::new_v4().to_string(),
        text: "x".repeat(MAX_TEXT + 1),
        images: vec![],
        created_at: now(),
    };
    assert!(wire.validate().is_err());
    let svg = c.stage(b"<svg onload='alert(1)'/>", "drawing.svg").unwrap();
    assert_eq!(svg.mime, "application/octet-stream");
    assert!(c.image_url(&svg.id).is_err());
    assert!(c.rename("").is_err());
    assert!(c.rename(&"字".repeat(41)).is_err());
    c.rename("书房 Mac").unwrap();
    drop(c);
    let restored = Core::create(root.path().into(), 47321, events()).unwrap();
    assert_eq!(restored.local.lock().name, "书房 Mac");
}

#[test]
fn upgrade_keeps_existing_messages_and_delivery_errors() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("messages.sqlite");
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch("CREATE TABLE messages (peer_id TEXT NOT NULL, id TEXT NOT NULL, direction TEXT NOT NULL, text TEXT NOT NULL, images TEXT NOT NULL, created_at INTEGER NOT NULL, status TEXT NOT NULL, unread INTEGER NOT NULL, digest TEXT NOT NULL, PRIMARY KEY(peer_id,id,direction));
        INSERT INTO messages VALUES ('peer','message','outgoing','升级前的消息','[]',1,'failed',0,'digest');").unwrap();
    drop(db);
    let store = pcmessage_lib::storage::Store::open(&path).unwrap();
    let messages = store.messages("peer", 100).unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].text, "升级前的消息");
    assert!(messages[0].delivery_error.is_none());
    store
        .status("peer", "message", "failed", Some("连接超时"))
        .unwrap();
    drop(store);
    let restored = pcmessage_lib::storage::Store::open(&path).unwrap();
    assert_eq!(
        restored
            .outgoing("peer", "message")
            .unwrap()
            .delivery_error
            .as_deref(),
        Some("连接超时")
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn arbitrary_files_roundtrip_export_restart_and_retry() {
    let ar = tempfile::tempdir().unwrap();
    let br = tempfile::tempdir().unwrap();
    let export = tempfile::tempdir().unwrap();
    let a = Core::create(ar.path().into(), 0, events()).unwrap();
    let b = Core::create(br.path().into(), 0, events()).unwrap();
    let _a_server = Running::start(a.clone(), false).await.unwrap();
    let b_server = Running::start(b.clone(), false).await.unwrap();
    ready(&b).await;
    pair(&a, &b).await;
    let aid = a.local.lock().id.clone();
    let bid = b.local.lock().id.clone();
    let port = b.local.lock().port;
    let cases = [
        (
            "计划.md",
            "# 你好 Windows\n\n**中文** 🏠".as_bytes().to_vec(),
        ),
        ("资料.pdf", b"%PDF-1.4\0\xff\r\n".to_vec()),
        ("archive.zip", b"PK\x03\x04\0\x80\xff".to_vec()),
        ("program.exe", b"MZ\0\xff\x80".to_vec()),
        (
            "data.unknown",
            (0..=255).cycle().take(1024 * 1024).collect(),
        ),
        ("empty.txt", vec![]),
        ("drawing.svg", b"<svg onload='alert(1)'/>".to_vec()),
        ("movie.mp4", b"\0\0\0\x18ftypmp42\xff\0".to_vec()),
    ];
    let attachments: Vec<_> = cases
        .iter()
        .map(|(name, bytes)| {
            let path = ar.path().join(name);
            std::fs::write(&path, bytes).unwrap();
            let attachment = a.stage_path(&path).unwrap();
            assert_eq!(attachment.name, *name);
            assert_eq!(attachment.mime, "application/octet-stream");
            assert!(a.image_url(&attachment.id).is_err());
            attachment
        })
        .collect();
    let sent = network::send(
        a.clone(),
        bid.clone(),
        String::new(),
        attachments.iter().map(|a| a.id.clone()).collect(),
    )
    .await
    .unwrap();
    assert_eq!(sent.status, "sent", "{:?}", sent.delivery_error);
    let received = b.store.lock().messages(&aid, 100).unwrap();
    assert_eq!(received[0].images, attachments);
    for (attachment, (name, bytes)) in attachments.iter().zip(&cases) {
        assert_eq!(&b.asset_bytes(&attachment.id).unwrap(), bytes);
        let destination = export.path().join(name);
        b.export_attachment(&attachment.id, &destination).unwrap();
        assert_eq!(&std::fs::read(destination).unwrap(), bytes);
    }
    // A payload with the wrong digest must never be accepted as an arbitrary file.
    let trusted = a.trusted(&bid).unwrap();
    let wire = WireMessage {
        id: uuid::Uuid::new_v4().to_string(),
        text: String::new(),
        images: vec![attachments[0].clone()],
        created_at: now(),
    };
    let response = network::client(&trusted.device, &trusted.address)
        .unwrap()
        .post(format!("https://pcmessage.local:{port}/v1/message"))
        .header("x-peer-id", &aid)
        .header("x-peer-token", &trusted.token)
        .multipart(
            reqwest::multipart::Form::new()
                .text("message", serde_json::to_string(&wire).unwrap())
                .part(
                    attachments[0].id.clone(),
                    reqwest::multipart::Part::bytes(b"changed".to_vec()),
                ),
        )
        .send()
        .await
        .unwrap();
    assert!(!response.status().is_success());
    assert_eq!(b.store.lock().messages(&aid, 100).unwrap().len(), 1);
    drop(b_server);
    drop(b);
    tokio::time::sleep(Duration::from_millis(30)).await;
    let failed = network::send(
        a.clone(),
        bid.clone(),
        String::new(),
        vec![attachments[0].id.clone()],
    )
    .await
    .unwrap();
    assert_eq!(failed.status, "failed");
    let restored = Core::create(br.path().into(), port, events()).unwrap();
    let _restored_server = Running::start(restored.clone(), false).await.unwrap();
    ready(&restored).await;
    assert_eq!(
        restored.store.lock().messages(&aid, 100).unwrap()[0].images,
        attachments
    );
    let retry = network::retry(a, bid, failed.id.clone()).await.unwrap();
    assert_eq!(retry.status, "sent");
    assert_eq!(retry.id, failed.id);
    assert_eq!(restored.store.lock().messages(&aid, 100).unwrap().len(), 2);
    assert_eq!(
        restored.asset_bytes(&attachments[0].id).unwrap(),
        cases[0].1
    );
}

#[tokio::test]
async fn file_limits_and_old_peer_compatibility() {
    let ar = tempfile::tempdir().unwrap();
    let br = tempfile::tempdir().unwrap();
    let a = Core::create(ar.path().into(), 0, events()).unwrap();
    let b = Core::create(br.path().into(), 0, events()).unwrap();
    b.local.lock().file_transfer = false;
    let _a_server = Running::start(a.clone(), false).await.unwrap();
    let _server = Running::start(b.clone(), false).await.unwrap();
    ready(&b).await;
    pair(&a, &b).await;
    let bid = b.local.lock().id.clone();
    let aid = a.local.lock().id.clone();
    let attachment = a.stage(b"plain text file", "readme.txt").unwrap();
    let file = network::send(
        a.clone(),
        bid.clone(),
        String::new(),
        vec![attachment.id.clone()],
    )
    .await
    .unwrap();
    assert_eq!(file.status, "failed");
    assert!(file.delivery_error.unwrap().contains("0.2.0"));
    assert!(b.store.lock().messages(&aid, 100).unwrap().is_empty());
    let text = network::send(a.clone(), bid.clone(), "文字仍可发送".into(), vec![])
        .await
        .unwrap();
    assert_eq!(text.status, "sent");
    let image = a.stage(&png(), "image.png").unwrap();
    assert_eq!(
        network::send(a.clone(), bid, String::new(), vec![image.id])
            .await
            .unwrap()
            .status,
        "sent"
    );
    b.local.lock().file_transfer = true;
    a.see(b.local.lock().clone(), "127.0.0.1".parse().unwrap())
        .unwrap();
    let bid = b.local.lock().id.clone();
    assert_eq!(
        network::retry(a.clone(), bid, file.id)
            .await
            .unwrap()
            .status,
        "sent"
    );
    let oversized = ar.path().join("large.bin");
    std::fs::File::create(&oversized)
        .unwrap()
        .set_len(MAX_FILE as u64 + 1)
        .unwrap();
    assert!(a.stage_path(&oversized).unwrap_err().contains("100 MB"));
    assert!(a.stage_path(ar.path()).is_err());
    let mut invalid = attachment.clone();
    invalid.name = "../../escaped.txt".into();
    assert!(invalid.validate().is_err());
    invalid = attachment;
    invalid.size = MAX_FILE + 1;
    assert!(invalid.validate().is_err());
    let mut old_info = serde_json::to_value(b.local.lock().clone()).unwrap();
    old_info.as_object_mut().unwrap().remove("file_transfer");
    let decoded: Device = serde_json::from_value(old_info).unwrap();
    assert!(!decoded.file_transfer);
    decoded.validate().unwrap();
}
