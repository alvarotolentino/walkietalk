use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio_tungstenite::{
    connect_async, tungstenite::Message as WsMessage, MaybeTlsStream, WebSocketStream,
};

pub type WsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;
pub type WsWriteTx = mpsc::Sender<WsMessage>;
pub type WsReadRx = mpsc::Receiver<WsMessage>;

/// Establish a WebSocket connection and split it into a writer channel and reader channel.
///
/// The writer spawns a task that forwards messages from the sender channel to the WS sink.
/// The reader spawns a task that forwards messages from the WS stream to the receiver channel.
pub async fn connect_ws(url: &str) -> Result<(WsWriteTx, WsReadRx), String> {
    let (ws_stream, _response) = connect_async(url)
        .await
        .map_err(|e| format!("WebSocket connection failed: {e}"))?;

    let (mut sink, mut stream) = ws_stream.split();

    // Writer channel: commands send WsMessage → this task writes to the WS sink
    let (write_tx, mut write_rx) = mpsc::channel::<WsMessage>(64);

    tokio::spawn(async move {
        while let Some(msg) = write_rx.recv().await {
            if sink.send(msg).await.is_err() {
                break;
            }
        }
        let _ = sink.close().await;
    });

    // Reader channel: WS stream → mpsc → consumer
    let (read_tx, read_rx) = mpsc::channel::<WsMessage>(64);

    tokio::spawn(async move {
        while let Some(Ok(msg)) = stream.next().await {
            if read_tx.send(msg).await.is_err() {
                break;
            }
        }
    });

    Ok((write_tx, read_rx))
}
