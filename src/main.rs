mod args;
mod auth;
mod error;
mod http_logger;
mod http_utils;
mod logger;
mod noscript;
mod server;
mod utils;

#[macro_use]
extern crate log;

use crate::args::{build_cli, print_completions, Args};
use crate::server::Server;
#[cfg(feature = "tls")]
use crate::utils::{load_certs, load_private_key};

use anyhow::{anyhow, bail, Context, Result};
use args::BindAddr;
use clap_complete::Shell;
use futures_util::future::join_all;

use hyper::{body::Incoming, service::service_fn, Request};
use hyper_util::{
    rt::{TokioExecutor, TokioIo},
    server::{
        conn::auto::Builder,
        graceful::{GracefulShutdown, Watcher},
    },
};
use std::future::Future;
use std::io;
use std::net::{IpAddr, SocketAddr, TcpListener as StdTcpListener};
use std::pin::pin;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::task::JoinSet;
use tokio::time::{sleep, timeout};
use tokio::{net::TcpListener, task::JoinHandle};
#[cfg(feature = "tls")]
use tokio_rustls::{
    rustls::{crypto::ring, ServerConfig},
    TlsAcceptor,
};
use tokio_util::sync::CancellationToken;

/// Delay after the first failed `accept()`, doubled on each further failure.
const ACCEPT_BACKOFF_MIN: Duration = Duration::from_millis(10);
const ACCEPT_BACKOFF_MAX: Duration = Duration::from_secs(1);

#[tokio::main]
async fn main() -> Result<()> {
    let cmd = build_cli();
    let matches = cmd.get_matches();
    if let Some(generator) = matches.get_one::<Shell>("completions") {
        let mut cmd = build_cli();
        print_completions(*generator, &mut cmd);
        return Ok(());
    }
    let mut args = Args::parse(matches)?;
    logger::init(args.log_file.clone()).map_err(|e| anyhow!("Failed to init logger, {e}"))?;
    let (new_addrs, print_addrs) = check_addrs(&args)?;
    args.addrs = new_addrs;
    let running = Arc::new(AtomicBool::new(true));
    let listening = print_listening(&args, &print_addrs)?;
    let shutdown_timeout = Duration::from_secs(args.shutdown_timeout);
    let shutdown = Shutdown::default();
    let handles = serve(args, running.clone(), shutdown.clone())?;
    println!("{listening}");

    let mut serving = pin!(join_all(handles));
    tokio::select! {
        ret = &mut serving => {
            for r in ret {
                if let Err(e) = r {
                    error!("{e}");
                }
            }
        },
        _ = shutdown_signal() => {
            // The health check starts answering 503 and walks stop early.
            running.store(false, Ordering::SeqCst);
            shutdown.drain.cancel();
            info!(
                "Shutting down, waiting up to {}s for in-flight requests",
                shutdown_timeout.as_secs()
            );
            if timeout(shutdown_timeout, &mut serving).await.is_err() {
                warn!("Shutdown timeout reached, aborting remaining requests");
                shutdown.abort.cancel();
                serving.await;
            }
        },
    }
    Ok(())
}

/// Signals sent to the accept loops when the process is asked to stop.
#[derive(Clone, Default)]
struct Shutdown {
    /// Stop accepting and let open connections finish their current request.
    drain: CancellationToken,
    /// Drop the connections that are still open.
    abort: CancellationToken,
}

/// Back-off between failed `accept()` calls, e.g. while out of file descriptors.
#[derive(Default)]
struct AcceptBackoff {
    delay: Option<Duration>,
}

impl AcceptBackoff {
    /// Delay to wait before the next `accept()` after a failure.
    fn next_delay(&mut self) -> Duration {
        let delay = self
            .delay
            .map_or(ACCEPT_BACKOFF_MIN, |d| (d * 2).min(ACCEPT_BACKOFF_MAX));
        self.delay = Some(delay);
        delay
    }

    fn reset(&mut self) {
        self.delay = None;
    }

    /// Log the failure (once per burst) and sleep. Returns `false` if the
    /// drain was requested meanwhile.
    async fn wait(&mut self, err: &io::Error, drain: &CancellationToken) -> bool {
        if self.delay.is_none() {
            error!("Failed to accept connection, {err}");
        }
        let delay = self.next_delay();
        tokio::select! {
            _ = drain.cancelled() => false,
            _ = sleep(delay) => true,
        }
    }
}

trait Listener: Send + 'static {
    type Stream: AsyncRead + AsyncWrite + Unpin + Send + 'static;
    /// A connection as accepted, before it is ready to carry HTTP.
    type Accepted: Send + 'static;

    /// Wait for the next connection. This must stay cheap: a connection that
    /// needs more work than `accept(2)` does it in `establish`.
    fn accept(
        &self,
    ) -> impl Future<Output = io::Result<(Self::Accepted, Option<SocketAddr>)>> + Send;

    /// Make an accepted connection usable, e.g. the TLS handshake. It runs in
    /// the connection's own task so a stalled client cannot block the accept
    /// loop. `None` is a connection that was dropped before it became usable.
    fn establish(
        &self,
        accepted: Self::Accepted,
    ) -> impl Future<Output = Option<Self::Stream>> + Send + 'static;
}

impl Listener for TcpListener {
    type Stream = tokio::net::TcpStream;
    type Accepted = tokio::net::TcpStream;

    async fn accept(&self) -> io::Result<(Self::Accepted, Option<SocketAddr>)> {
        let (stream, addr) = TcpListener::accept(self).await?;
        Ok((stream, Some(addr)))
    }

    fn establish(
        &self,
        accepted: Self::Accepted,
    ) -> impl Future<Output = Option<Self::Stream>> + Send + 'static {
        std::future::ready(Some(accepted))
    }
}

#[cfg(feature = "tls")]
struct TlsListener {
    listener: TcpListener,
    acceptor: TlsAcceptor,
}

#[cfg(feature = "tls")]
impl Listener for TlsListener {
    type Stream = tokio_rustls::server::TlsStream<tokio::net::TcpStream>;
    type Accepted = tokio::net::TcpStream;

    async fn accept(&self) -> io::Result<(Self::Accepted, Option<SocketAddr>)> {
        let (stream, addr) = self.listener.accept().await?;
        Ok((stream, Some(addr)))
    }

    fn establish(
        &self,
        accepted: Self::Accepted,
    ) -> impl Future<Output = Option<Self::Stream>> + Send + 'static {
        let acceptor = self.acceptor.clone();
        async move {
            let handshake_timeout = Duration::from_secs(10);
            timeout(handshake_timeout, acceptor.accept(accepted))
                .await
                .ok()
                .and_then(|v| v.ok())
        }
    }
}

#[cfg(unix)]
impl Listener for tokio::net::UnixListener {
    type Stream = tokio::net::UnixStream;
    type Accepted = tokio::net::UnixStream;

    async fn accept(&self) -> io::Result<(Self::Accepted, Option<SocketAddr>)> {
        let (stream, _addr) = tokio::net::UnixListener::accept(self).await?;
        Ok((stream, None))
    }

    fn establish(
        &self,
        accepted: Self::Accepted,
    ) -> impl Future<Output = Option<Self::Stream>> + Send + 'static {
        std::future::ready(Some(accepted))
    }
}

/// Accept connections until the drain is requested, then wait for the open
/// ones to finish. Connections are dropped as soon as the abort is requested.
async fn accept_loop<L: Listener>(listener: L, server: Arc<Server>, shutdown: Shutdown) {
    let graceful = GracefulShutdown::new();
    let mut conns = JoinSet::new();
    let mut backoff = AcceptBackoff::default();
    loop {
        tokio::select! {
            _ = shutdown.drain.cancelled() => break,
            // Reap finished connections so the set doesn't grow forever.
            Some(_) = conns.join_next(), if !conns.is_empty() => {}
            ret = listener.accept() => match ret {
                Ok((accepted, addr)) => {
                    backoff.reset();
                    let establish = listener.establish(accepted);
                    let server = server.clone();
                    let watcher = graceful.watcher();
                    conns.spawn(async move {
                        if let Some(stream) = establish.await {
                            handle_stream(server, TokioIo::new(stream), addr, watcher).await;
                        }
                    });
                }
                Err(err) => {
                    if !backoff.wait(&err, &shutdown.drain).await {
                        break;
                    }
                }
            },
        }
    }
    // Refuse new connections instead of leaving them in the backlog.
    drop(listener);
    tokio::select! {
        _ = graceful.shutdown() => {}
        _ = shutdown.abort.cancelled() => conns.shutdown().await,
    }
}

fn serve(args: Args, running: Arc<AtomicBool>, shutdown: Shutdown) -> Result<Vec<JoinHandle<()>>> {
    let addrs = args.addrs.clone();
    let port = args.port;
    let tls_config = (args.tls_cert.clone(), args.tls_key.clone());
    let path_is_file = args.path_is_file;
    let server_handle = Arc::new(Server::init(args, running)?);
    if !path_is_file {
        server_handle.spawn_part_sweep();
    }
    let mut handles = vec![];
    for bind_addr in addrs.iter() {
        let server_handle = server_handle.clone();
        let shutdown = shutdown.clone();
        match bind_addr {
            BindAddr::IpAddr(ip) => {
                let listener = create_listener(SocketAddr::new(*ip, port))
                    .with_context(|| format!("Failed to bind `{ip}:{port}`"))?;

                match &tls_config {
                    #[cfg(feature = "tls")]
                    (Some(cert_file), Some(key_file)) => {
                        let certs = load_certs(cert_file)?;
                        let key = load_private_key(key_file)?;
                        // Select the `ring` provider explicitly instead of relying on
                        // rustls' feature-based auto-detection, which panics when more
                        // than one provider is enabled (e.g. via dev-dependencies).
                        let mut config =
                            ServerConfig::builder_with_provider(Arc::new(ring::default_provider()))
                                .with_safe_default_protocol_versions()?
                                .with_no_client_auth()
                                .with_single_cert(certs, key)?;
                        config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
                        let config = Arc::new(config);
                        let listener = TlsListener {
                            listener,
                            acceptor: TlsAcceptor::from(config),
                        };

                        handles.push(tokio::spawn(accept_loop(listener, server_handle, shutdown)));
                    }
                    (None, None) => {
                        handles.push(tokio::spawn(accept_loop(listener, server_handle, shutdown)));
                    }
                    _ => {
                        bail!("tls-cert and tls-key must be set together")
                    }
                };
            }
            #[cfg(unix)]
            BindAddr::SocketPath(path) => {
                let socket_path = if path.starts_with("@")
                    && cfg!(any(target_os = "linux", target_os = "android"))
                {
                    let mut path_buf = path.as_bytes().to_vec();
                    path_buf[0] = b'\0';
                    unsafe { std::ffi::OsStr::from_encoded_bytes_unchecked(&path_buf) }
                        .to_os_string()
                } else {
                    let _ = std::fs::remove_file(path);
                    path.into()
                };
                let listener = tokio::net::UnixListener::bind(socket_path)
                    .with_context(|| format!("Failed to bind `{path}`"))?;
                handles.push(tokio::spawn(accept_loop(listener, server_handle, shutdown)));
            }
        }
    }
    Ok(handles)
}

async fn handle_stream<T>(
    handle: Arc<Server>,
    stream: TokioIo<T>,
    addr: Option<SocketAddr>,
    watcher: Watcher,
) where
    T: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    let hyper_service =
        service_fn(move |request: Request<Incoming>| handle.clone().call(request, addr));

    let builder = Builder::new(TokioExecutor::new()).title_case_headers(true);
    let conn = builder.serve_connection_with_upgrades(stream, hyper_service);
    match watcher.watch(conn).await {
        Ok(()) => {}
        Err(_err) => {
            // This error only appears when the client doesn't send a request and terminate the connection.
            //
            // If client sends one request then terminate connection whenever, it doesn't appear.
        }
    }
}

fn create_listener(addr: SocketAddr) -> Result<TcpListener> {
    use socket2::{Domain, Protocol, Socket, Type};
    let socket = Socket::new(Domain::for_address(addr), Type::STREAM, Some(Protocol::TCP))?;
    if addr.is_ipv6() {
        socket.set_only_v6(true)?;
    }
    // On Windows `SO_REUSEADDR` allows binding an address that is already in use,
    // so a second instance would start successfully but never receive connections.
    // Only set it elsewhere, where it just allows rebinding a port in `TIME_WAIT`.
    #[cfg(not(windows))]
    socket.set_reuse_address(true)?;
    socket.bind(&addr.into())?;
    socket.listen(1024 /* Default backlog */)?;
    let std_listener = StdTcpListener::from(socket);
    std_listener.set_nonblocking(true)?;
    let listener = TcpListener::from_std(std_listener)?;
    Ok(listener)
}

fn check_addrs(args: &Args) -> Result<(Vec<BindAddr>, Vec<BindAddr>)> {
    let mut new_addrs = vec![];
    let mut print_addrs = vec![];
    let has_unspecified = args
        .addrs
        .iter()
        .any(|a| matches!(a, BindAddr::IpAddr(ip) if ip.is_unspecified()));
    let (ipv4_addrs, ipv6_addrs) = if has_unspecified {
        interface_addrs()?
    } else {
        (vec![], vec![])
    };
    for bind_addr in args.addrs.iter() {
        new_addrs.push(bind_addr.clone());
        match bind_addr {
            BindAddr::IpAddr(ip) => match &ip {
                IpAddr::V4(_) => {
                    if ip.is_unspecified() {
                        print_addrs.extend(ipv4_addrs.clone());
                    } else {
                        print_addrs.push(bind_addr.clone());
                    }
                }
                IpAddr::V6(_) => {
                    if ip.is_unspecified() {
                        print_addrs.extend(ipv6_addrs.clone());
                    } else {
                        print_addrs.push(bind_addr.clone());
                    }
                }
            },
            #[cfg(unix)]
            _ => print_addrs.push(bind_addr.clone()),
        }
    }
    print_addrs.sort_unstable();
    Ok((new_addrs, print_addrs))
}

fn interface_addrs() -> Result<(Vec<BindAddr>, Vec<BindAddr>)> {
    let (mut ipv4_addrs, mut ipv6_addrs) = (vec![], vec![]);
    let ifaces =
        if_addrs::get_if_addrs().with_context(|| "Failed to get local interface addresses")?;
    for iface in ifaces.into_iter() {
        let ip = iface.ip();
        if ip.is_ipv4() {
            ipv4_addrs.push(BindAddr::IpAddr(ip))
        }
        if ip.is_ipv6() {
            ipv6_addrs.push(BindAddr::IpAddr(ip))
        }
    }
    Ok((ipv4_addrs, ipv6_addrs))
}

fn print_listening(args: &Args, print_addrs: &[BindAddr]) -> Result<String> {
    let mut output = String::new();
    let urls = print_addrs
        .iter()
        .map(|bind_addr| match bind_addr {
            BindAddr::IpAddr(addr) => {
                let addr = match addr {
                    IpAddr::V4(_) => format!("{}:{}", addr, args.port),
                    IpAddr::V6(_) => format!("[{}]:{}", addr, args.port),
                };
                let protocol = if args.tls_cert.is_some() {
                    "https"
                } else {
                    "http"
                };
                format!("{}://{}{}", protocol, addr, args.uri_prefix)
            }
            #[cfg(unix)]
            BindAddr::SocketPath(path) => path.to_string(),
        })
        .collect::<Vec<_>>();

    if urls.len() == 1 {
        output.push_str(&format!("Listening on {}", urls[0]))
    } else {
        let info = urls
            .iter()
            .map(|v| format!("  {v}"))
            .collect::<Vec<String>>()
            .join("\n");
        output.push_str(&format!("Listening on:\n{info}\n"))
    }

    Ok(output)
}

/// Resolves when the process is asked to stop: SIGINT or SIGTERM on Unix,
/// Ctrl+C elsewhere.
async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        let (mut sigint, mut sigterm) = match (
            signal(SignalKind::interrupt()),
            signal(SignalKind::terminate()),
        ) {
            (Ok(sigint), Ok(sigterm)) => (sigint, sigterm),
            (Err(e), _) | (_, Err(e)) => {
                error!("Failed to install signal handlers, {e}");
                return;
            }
        };
        tokio::select! {
            _ = sigint.recv() => {}
            _ = sigterm.recv() => {}
        }
    }
    #[cfg(not(unix))]
    if let Err(e) = tokio::signal::ctrl_c().await {
        error!("Failed to install CTRL+C signal handler, {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accept_backoff_doubles_up_to_the_cap_and_resets() {
        let mut backoff = AcceptBackoff::default();
        let delays: Vec<_> = (0..9).map(|_| backoff.next_delay().as_millis()).collect();
        assert_eq!(delays, [10, 20, 40, 80, 160, 320, 640, 1000, 1000]);
        backoff.reset();
        assert_eq!(backoff.next_delay(), ACCEPT_BACKOFF_MIN);
    }
}
