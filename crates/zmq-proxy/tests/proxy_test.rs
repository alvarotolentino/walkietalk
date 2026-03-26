use std::sync::atomic::Ordering;
use std::time::Duration;

use zeromq::{PushSocket, Socket, SocketRecv, SocketSend, SubSocket, ZmqMessage};

/// Find a free TCP port by binding to :0 and reading back the assigned port.
fn free_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind to :0");
    listener.local_addr().expect("local_addr").port()
}

#[tokio::test]
async fn test_proxy_forwards_single_message() {
    let pull_port = free_port();
    let pub_port = free_port();
    let pull_addr = format!("tcp://127.0.0.1:{pull_port}");
    let pub_addr = format!("tcp://127.0.0.1:{pub_port}");

    // Start the proxy
    let stats = walkietalk_zmq_proxy::run_proxy(&pull_addr, &pub_addr)
        .await
        .expect("proxy starts");

    // Give sockets time to bind
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Connect a SUB socket and subscribe to all topics
    let mut sub = SubSocket::new();
    sub.connect(&pub_addr).await.expect("SUB connect");
    sub.subscribe("").await.expect("subscribe all");

    // Connect a PUSH socket
    let mut push = PushSocket::new();
    push.connect(&pull_addr).await.expect("PUSH connect");

    // Allow connections to establish
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Send a message
    let payload = b"audio.42 hello-world";
    let msg: ZmqMessage = payload.to_vec().into();
    push.send(msg).await.expect("PUSH send");

    // Receive on SUB with timeout
    let received = tokio::time::timeout(Duration::from_secs(5), sub.recv())
        .await
        .expect("recv timeout")
        .expect("SUB recv");

    let frame = received.into_vec().pop().expect("at least one frame");
    assert_eq!(frame.as_ref(), payload);
    assert_eq!(stats.frames_forwarded.load(Ordering::Relaxed), 1);
}

#[tokio::test]
async fn test_proxy_forwards_multi_frame_message() {
    let pull_port = free_port();
    let pub_port = free_port();
    let pull_addr = format!("tcp://127.0.0.1:{pull_port}");
    let pub_addr = format!("tcp://127.0.0.1:{pub_port}");

    let stats = walkietalk_zmq_proxy::run_proxy(&pull_addr, &pub_addr)
        .await
        .expect("proxy starts");

    tokio::time::sleep(Duration::from_millis(100)).await;

    let mut sub = SubSocket::new();
    sub.connect(&pub_addr).await.expect("SUB connect");
    sub.subscribe("audio.").await.expect("subscribe audio");

    let mut push = PushSocket::new();
    push.connect(&pull_addr).await.expect("PUSH connect");

    tokio::time::sleep(Duration::from_millis(200)).await;

    // Build a 3-frame message (topic, speaker UUID, audio data) — matches our wire format
    let topic = b"audio.99";
    let speaker_id = [0xABu8; 16];
    let audio_data = vec![0xFFu8; 160];

    let mut msg = ZmqMessage::from(topic.to_vec());
    msg.push_back(speaker_id.to_vec().into());
    msg.push_back(audio_data.clone().into());

    push.send(msg).await.expect("PUSH send");

    let received = tokio::time::timeout(Duration::from_secs(5), sub.recv())
        .await
        .expect("recv timeout")
        .expect("SUB recv");

    let frames: Vec<bytes::Bytes> = received.into_vec();
    assert_eq!(frames.len(), 3, "expected 3 frames");
    assert_eq!(frames[0].as_ref(), topic);
    assert_eq!(frames[1].as_ref(), &speaker_id);
    assert_eq!(frames[2].as_ref(), audio_data.as_slice());
    assert_eq!(stats.frames_forwarded.load(Ordering::Relaxed), 1);
}

#[tokio::test]
async fn test_proxy_topic_filtering() {
    let pull_port = free_port();
    let pub_port = free_port();
    let pull_addr = format!("tcp://127.0.0.1:{pull_port}");
    let pub_addr = format!("tcp://127.0.0.1:{pub_port}");

    walkietalk_zmq_proxy::run_proxy(&pull_addr, &pub_addr)
        .await
        .expect("proxy starts");

    tokio::time::sleep(Duration::from_millis(100)).await;

    // SUB only subscribes to "ctrl." topics
    let mut sub = SubSocket::new();
    sub.connect(&pub_addr).await.expect("SUB connect");
    sub.subscribe("ctrl.").await.expect("subscribe ctrl");

    let mut push = PushSocket::new();
    push.connect(&pull_addr).await.expect("PUSH connect");

    tokio::time::sleep(Duration::from_millis(200)).await;

    // Send an audio message (should be filtered out by SUB)
    let audio_msg: ZmqMessage = b"audio.1 data".to_vec().into();
    push.send(audio_msg).await.expect("send audio");

    // Send a ctrl message (should arrive)
    let ctrl_msg: ZmqMessage = b"ctrl.1 event".to_vec().into();
    push.send(ctrl_msg).await.expect("send ctrl");

    // We should only receive the ctrl message
    let received = tokio::time::timeout(Duration::from_secs(5), sub.recv())
        .await
        .expect("recv timeout")
        .expect("SUB recv");

    let frame = received.into_vec().pop().expect("at least one frame");
    assert!(
        frame.starts_with(b"ctrl."),
        "expected ctrl topic, got: {:?}",
        frame
    );

    // Verify the audio message does NOT arrive (short timeout)
    let no_msg = tokio::time::timeout(Duration::from_millis(500), sub.recv()).await;
    assert!(
        no_msg.is_err(),
        "audio message should not arrive on ctrl-only subscriber"
    );
}
