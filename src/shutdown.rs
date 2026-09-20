use axum::Router;
use std::{future::Future, io, net::SocketAddr, time::Duration};
use tokio::{net::TcpListener, sync::oneshot};

pub struct Signals {
    #[cfg(unix)]
    interrupt: tokio::signal::unix::Signal,
    #[cfg(unix)]
    terminate: tokio::signal::unix::Signal,
}

impl Signals {
    pub fn new() -> io::Result<Self> {
        Ok(Self {
            #[cfg(unix)]
            interrupt: tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())?,
            #[cfg(unix)]
            terminate: tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?,
        })
    }

    pub async fn wait(&mut self) {
        #[cfg(unix)]
        tokio::select! {
            _ = self.interrupt.recv() => {},
            _ = self.terminate.recv() => {},
        }
        #[cfg(not(unix))]
        if tokio::signal::ctrl_c().await.is_err() {
            tracing::error!("Could not listen for shutdown signal");
        }
    }
}

pub async fn serve(
    listener: TcpListener,
    app: Router,
    signal: impl Future<Output = ()>,
    timeout: Duration,
) -> io::Result<()> {
    let (stop, stopped) = oneshot::channel();
    let mut server = tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .with_graceful_shutdown(async {
            let _ = stopped.await;
        })
        .await
    });
    tokio::select! {
        result = &mut server => return result.map_err(io::Error::other)?,
        _ = signal => tracing::info!("Shutdown requested; draining active requests"),
    }
    let _ = stop.send(());
    match tokio::time::timeout(timeout, &mut server).await {
        Ok(result) => {
            result.map_err(io::Error::other)??;
            tracing::info!("HTTP server stopped");
        }
        Err(_) => {
            tracing::warn!("Request drain timed out; forcing shutdown");
            server.abort();
            let _ = server.await;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::routing::get;
    use std::sync::Arc;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpStream,
        sync::Notify,
    };

    async fn exercise(release_request: bool) {
        let entered = Arc::new(Notify::new());
        let release = Arc::new(Notify::new());
        let app = Router::new().route(
            "/",
            get({
                let entered = entered.clone();
                let release = release.clone();
                move || {
                    let entered = entered.clone();
                    let release = release.clone();
                    async move {
                        entered.notify_one();
                        release.notified().await;
                        "completed"
                    }
                }
            }),
        );
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (stop, stopped) = oneshot::channel();
        let server = tokio::spawn(serve(
            listener,
            app,
            async {
                let _ = stopped.await;
            },
            if release_request {
                Duration::from_secs(1)
            } else {
                Duration::from_millis(100)
            },
        ));
        let mut client = TcpStream::connect(address).await.unwrap();
        client
            .write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
            .await
            .unwrap();
        entered.notified().await;
        stop.send(()).unwrap();
        if release_request {
            tokio::time::timeout(Duration::from_secs(1), async {
                while TcpStream::connect(address).await.is_ok() {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            assert!(!server.is_finished());
            release.notify_one();
            let mut body = String::new();
            client.read_to_string(&mut body).await.unwrap();
            assert!(body.starts_with("HTTP/1.1 200"));
            assert!(body.ends_with("completed"));
        }
        tokio::time::timeout(Duration::from_secs(2), server)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert!(TcpStream::connect(address).await.is_err());
    }

    #[tokio::test]
    async fn completes_an_inflight_request() {
        exercise(true).await;
    }

    #[tokio::test]
    async fn stalled_request_does_not_block_shutdown() {
        exercise(false).await;
    }
}
