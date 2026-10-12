//! End-to-end tests of the DSU server over real loopback UDP, using a client that mimics Cemu.

use std::time::Duration;
use wiiu_dsu::pad::buttons;
use wiiu_dsu::probe::DsuProbe;
use wiiu_dsu::protocol::{PadDataRequest, ServerMessage, SlotState};
use wiiu_dsu::{DsuServer, PadData, SlotTable, TouchPoint};

const T: Duration = Duration::from_secs(2);

async fn start() -> (SlotTable, std::net::SocketAddr, tokio::sync::oneshot::Sender<()>) {
    let table = SlotTable::new();
    let server = DsuServer::bind("127.0.0.1:0".parse().unwrap(), table.clone())
        .await
        .unwrap();
    let addr = server.local_addr().unwrap();
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    tokio::spawn(server.run(async {
        let _ = rx.await;
    }));
    (table, addr, tx)
}

#[tokio::test]
async fn answers_version_and_port_info_like_cemu_expects() {
    let (_table, addr, _stop) = start().await;
    let probe = DsuProbe::connect(addr).await.unwrap();

    probe.send_version_request().await.unwrap();
    match probe.recv(T).await.unwrap() {
        ServerMessage::Version { version, .. } => assert_eq!(version, 1001),
        other => panic!("expected version, got {other:?}"),
    }

    probe.send_port_info_request(&[0, 1, 2, 3]).await.unwrap();
    let mut seen = Vec::new();
    for _ in 0..4 {
        match probe.recv(T).await.unwrap() {
            ServerMessage::PortInfo { info, .. } => {
                assert_eq!(info.state, SlotState::Connected);
                seen.push(info.slot);
            }
            other => panic!("expected port info, got {other:?}"),
        }
    }
    seen.sort();
    assert_eq!(seen, vec![0, 1, 2, 3]);
}

#[tokio::test]
async fn ignores_out_of_range_port_info_slots() {
    let (_table, addr, _stop) = start().await;
    let probe = DsuProbe::connect(addr).await.unwrap();
    probe.send_port_info_request(&[7]).await.unwrap();
    assert!(probe.recv(Duration::from_millis(200)).await.is_err());
}

#[tokio::test]
async fn streams_buttons_and_pointer_for_subscribed_slot() {
    let (table, addr, _stop) = start().await;
    let probe = DsuProbe::connect(addr).await.unwrap();

    // Neutral at first.
    let pad = probe.wait_for_pad(1, T, |_| true).await.unwrap();
    assert_eq!(pad, PadData::neutral());

    let mut pressed = PadData::neutral();
    pressed.buttons = buttons::CROSS | buttons::CIRCLE;
    pressed.touch[0] = Some(TouchPoint { id: 0, x: 960, y: 471 });
    table.set_pad(1, pressed);

    let pad = probe.wait_for_pad(1, T, |p| p.buttons != 0).await.unwrap();
    assert_eq!(pad, pressed);

    table.set_pad(1, PadData::neutral());
    let pad = probe.wait_for_pad(1, T, |p| p.buttons == 0).await.unwrap();
    assert_eq!(pad.touch, [None, None]);
}

#[tokio::test]
async fn slots_are_independent() {
    let (table, addr, _stop) = start().await;
    let probe = DsuProbe::connect(addr).await.unwrap();

    let mut a = PadData::neutral();
    a.buttons = buttons::CROSS;
    table.set_pad(1, a);
    let mut b = PadData::neutral();
    b.buttons = buttons::SQUARE;
    b.touch[0] = Some(TouchPoint { id: 0, x: 10, y: 20 });
    table.set_pad(2, b);

    assert_eq!(probe.wait_for_pad(1, T, |p| p.buttons != 0).await.unwrap(), a);
    assert_eq!(probe.wait_for_pad(2, T, |p| p.buttons != 0).await.unwrap(), b);
    assert_eq!(probe.wait_for_pad(3, T, |_| true).await.unwrap(), PadData::neutral());
}

#[tokio::test]
async fn all_slots_subscription_delivers_every_slot() {
    let (_table, addr, _stop) = start().await;
    let probe = DsuProbe::connect(addr).await.unwrap();
    probe.send_pad_request(PadDataRequest::AllSlots).await.unwrap();
    let mut seen = [false; 4];
    while !seen.iter().all(|s| *s) {
        if let ServerMessage::PadData { info, .. } = probe.recv(T).await.unwrap() {
            seen[info.slot as usize] = true;
        }
    }
}

#[tokio::test]
async fn ignores_garbage_and_keeps_serving() {
    let (_table, addr, _stop) = start().await;
    let sock = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    for junk in [
        &b""[..],
        b"DSUC",
        &[0xFF; 300],
        b"DSUC\xe9\x03\x04\x00\0\0\0\0\0\0\0\0\0\0\x10\0",
    ] {
        sock.send_to(junk, addr).await.unwrap();
    }
    let probe = DsuProbe::connect(addr).await.unwrap();
    probe.send_version_request().await.unwrap();
    assert!(matches!(probe.recv(T).await.unwrap(), ServerMessage::Version { .. }));
}
