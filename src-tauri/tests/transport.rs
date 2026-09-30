use pcmessage_lib::{
    core::{Core, Events},
    model::*,
    network::{self, Running},
};
use std::{sync::Arc, time::Duration};

fn events() -> Events {
    Arc::new(|_, _| {})
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
    assert_eq!(b.image_bytes(&image.id).unwrap(), png());
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
    assert!(a.image_bytes("../../private-key.pem").is_err());
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
    assert!(c.stage(b"<svg onload='alert(1)'/>", "bad.svg").is_err());
    assert!(c.rename("").is_err());
    assert!(c.rename(&"字".repeat(41)).is_err());
    c.rename("书房 Mac").unwrap();
    drop(c);
    let restored = Core::create(root.path().into(), 47321, events()).unwrap();
    assert_eq!(restored.local.lock().name, "书房 Mac");
}
