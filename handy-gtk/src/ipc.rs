use tokio::sync::mpsc;
use zbus::{connection, interface, proxy, Connection};

pub const SERVICE_NAME: &str = "computer.handy.Handy";
pub const OBJECT_PATH: &str = "/computer/handy/Handy";

/// Actions the primary instance must handle on behalf of a second instance or the tray menu.
#[derive(Debug, PartialEq, Eq)]
pub enum IpcAction {
    FocusWindow,
    ToggleTranscription,
    TogglePostProcess,
    Cancel,
}

struct HandyService {
    tx: mpsc::Sender<IpcAction>,
}

#[interface(name = "computer.handy.Handy")]
impl HandyService {
    async fn ping(&self) -> bool {
        true
    }

    async fn focus_window(&self) {
        let _ = self.tx.send(IpcAction::FocusWindow).await;
    }

    async fn toggle_transcription(&self) {
        let _ = self.tx.send(IpcAction::ToggleTranscription).await;
    }

    async fn toggle_post_process(&self) {
        let _ = self.tx.send(IpcAction::TogglePostProcess).await;
    }

    async fn cancel(&self) {
        let _ = self.tx.send(IpcAction::Cancel).await;
    }
}

#[proxy(
    interface = "computer.handy.Handy",
    default_service = "computer.handy.Handy",
    default_path = "/computer/handy/Handy"
)]
trait Handy {
    async fn ping(&self) -> zbus::Result<bool>;
    async fn focus_window(&self) -> zbus::Result<()>;
    async fn toggle_transcription(&self) -> zbus::Result<()>;
    async fn toggle_post_process(&self) -> zbus::Result<()>;
    async fn cancel(&self) -> zbus::Result<()>;
}

/// Registers the D-Bus well-known name and starts serving the IPC interface.
/// Returns the connection (must be kept alive) and the action receiver.
pub async fn register_service() -> zbus::Result<(Connection, mpsc::Receiver<IpcAction>)> {
    let (tx, rx) = mpsc::channel(16);
    let conn = connection::Builder::session()?
        .name(SERVICE_NAME)?
        .serve_at(OBJECT_PATH, HandyService { tx })?
        .build()
        .await?;
    Ok((conn, rx))
}

/// Returns true if a primary instance is already running.
pub async fn is_primary_running() -> bool {
    let Ok(conn) = Connection::session().await else {
        return false;
    };
    let Ok(proxy) = HandyProxy::new(&conn).await else {
        return false;
    };
    proxy.ping().await.unwrap_or(false)
}

/// Sends the appropriate action to the running primary instance.
/// If no remote-control flag is set, brings the settings window to the front.
pub async fn dispatch_to_primary(
    toggle_transcription: bool,
    toggle_post_process: bool,
    cancel: bool,
) -> zbus::Result<()> {
    let conn = Connection::session().await?;
    let proxy = HandyProxy::new(&conn).await?;

    if cancel {
        proxy.cancel().await?;
    } else if toggle_post_process {
        proxy.toggle_post_process().await?;
    } else if toggle_transcription {
        proxy.toggle_transcription().await?;
    } else {
        proxy.focus_window().await?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::net::UnixStream;

    async fn make_p2p_pair(tx: mpsc::Sender<IpcAction>) -> (Connection, Connection) {
        let (s1, s2) = UnixStream::pair().expect("UnixStream::pair");
        let guid = zbus::Guid::generate();

        let server_fut = connection::Builder::unix_stream(s1)
            .server(guid)
            .expect("server guid")
            .p2p()
            .serve_at(OBJECT_PATH, HandyService { tx })
            .expect("serve_at")
            .build();

        let client_fut = connection::Builder::unix_stream(s2).p2p().build();

        // Both sides must progress concurrently to complete the auth handshake.
        let (server, client) = tokio::join!(server_fut, client_fut);
        (server.expect("server build"), client.expect("client build"))
    }

    async fn make_proxy(client: &Connection) -> HandyProxy<'_> {
        HandyProxy::builder(client)
            .path(OBJECT_PATH)
            .expect("path")
            .build()
            .await
            .expect("proxy build")
    }

    async fn recv_action(rx: &mut mpsc::Receiver<IpcAction>) -> IpcAction {
        tokio::time::timeout(Duration::from_secs(1), rx.recv())
            .await
            .expect("timed out waiting for IpcAction")
            .expect("channel closed")
    }

    #[tokio::test]
    async fn ping_returns_true() {
        let (tx, _rx) = mpsc::channel(4);
        let (_server, client) = make_p2p_pair(tx).await;
        let proxy = make_proxy(&client).await;
        assert!(proxy.ping().await.expect("ping"));
    }

    #[tokio::test]
    async fn focus_window_delivers_action() {
        let (tx, mut rx) = mpsc::channel(4);
        let (_server, client) = make_p2p_pair(tx).await;
        let proxy = make_proxy(&client).await;

        proxy.focus_window().await.expect("focus_window");
        assert_eq!(recv_action(&mut rx).await, IpcAction::FocusWindow);
    }

    #[tokio::test]
    async fn toggle_transcription_delivers_action() {
        let (tx, mut rx) = mpsc::channel(4);
        let (_server, client) = make_p2p_pair(tx).await;
        let proxy = make_proxy(&client).await;

        proxy
            .toggle_transcription()
            .await
            .expect("toggle_transcription");
        assert_eq!(recv_action(&mut rx).await, IpcAction::ToggleTranscription);
    }

    #[tokio::test]
    async fn toggle_post_process_delivers_action() {
        let (tx, mut rx) = mpsc::channel(4);
        let (_server, client) = make_p2p_pair(tx).await;
        let proxy = make_proxy(&client).await;

        proxy
            .toggle_post_process()
            .await
            .expect("toggle_post_process");
        assert_eq!(recv_action(&mut rx).await, IpcAction::TogglePostProcess);
    }

    #[tokio::test]
    async fn cancel_delivers_action() {
        let (tx, mut rx) = mpsc::channel(4);
        let (_server, client) = make_p2p_pair(tx).await;
        let proxy = make_proxy(&client).await;

        proxy.cancel().await.expect("cancel");
        assert_eq!(recv_action(&mut rx).await, IpcAction::Cancel);
    }
}
