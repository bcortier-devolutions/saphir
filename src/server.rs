//! Server is the centerpiece on saphir, it contains everything to handle request and dispatch
//! it the proper router
//!
//! *SAFETY NOTICE*
//!
//! To allow controller and middleware to respond future with static lifetime, the server stack is
//! put inside a static variable. This is needed for safety, but also means that only one saphir
//! server can run at a time

use std::future::Future;
use std::net::SocketAddr;
use std::mem::MaybeUninit;

use futures::prelude::*;
use futures::stream::StreamExt;
use futures::task::{Context, Poll};
use hyper::Body;
use hyper::server::conn::Http;
use hyper::service::Service;
use tokio::net::TcpListener;
use parking_lot::{Once, OnceState};

use crate::error::SaphirError;
use crate::http_context::HttpContext;
use crate::request::Request;
use crate::response::Response;
use crate::router::{Builder as RouterBuilder, RouterChain, RouterChainEnd};
use crate::router::Router;
use crate::middleware::{Builder as MiddlewareStackBuilder, MiddlewareChain, MiddleChainEnd};

/// Default time for request handling is 30 seconds
pub const DEFAULT_REQUEST_TIMEOUT_MS: u64 = 30_000;
/// Default listener ip addr is AnyAddr (0.0.0.0)
pub const DEFAULT_LISTENER_IFACE: &'static str = "0.0.0.0:0";

#[doc(hidden)]
static mut STACK: MaybeUninit<Stack> = MaybeUninit::uninit();
#[doc(hidden)]
static INIT_STACK: Once = Once::new();

/// Using Feature `https`
///
/// A struct representing certificate or private key configuration.
#[cfg(feature = "https")]
#[derive(Clone)]
pub enum SslConfig {
    /// File path
    FilePath(String),

    /// File content where all \n and space have been removed.
    FileData(String),
}

pub struct ListenerBuilder {
    iface: Option<String>,
    request_timeout_ms: Option<u64>,
    #[cfg(feature = "https")]
    cert_config: Option<SslConfig>,
    #[cfg(feature = "https")]
    key_config: Option<SslConfig>,
}

impl ListenerBuilder {
    #[inline]
    pub fn new() -> Self {
        ListenerBuilder {
            iface: None,
            request_timeout_ms: Some(DEFAULT_REQUEST_TIMEOUT_MS),
            #[cfg(feature = "https")]
            cert_config: None,
            #[cfg(feature = "https")]
            key_config: None,
        }
    }

    #[inline]
    pub fn interface(mut self, s: &str) -> Self {
        self.iface = Some(s.to_string());
        self
    }

    #[inline]
    pub fn request_timeout<T: Into<Option<u64>>>(mut self, timeout_ms: T) -> Self {
        self.request_timeout_ms = timeout_ms.into();
        self
    }

    /// Using Feature `https`
    ///
    /// Set the listener ssl certificates files. The cert needs to be PEM encoded
    /// while the key can be either RSA or PKCS8
    #[inline]
    #[cfg(feature = "https")]
    pub fn set_ssl_certificates(self, cert_path: &str, key_path: &str) -> Self {
        self.set_ssl_config(SslConfig::FilePath(cert_path.to_string()), SslConfig::FilePath(key_path.to_string()))
    }

    /// Using Feature `https`
    ///
    /// Set the listener ssl config. The cert needs to be PEM encoded
    /// while the key can be either RSA or PKCS8. The file path can be used or the
    /// file content directly where all \n and space have been removed.
    #[inline]
    #[cfg(feature = "https")]
    pub fn set_ssl_config(mut self, cert_config: SslConfig, key_config: SslConfig) -> Self {
        self.cert_config = Some(cert_config);
        self.key_config = Some(key_config);
        self
    }


    #[cfg(feature = "https")]
    #[inline]
    pub(crate) fn build(self) -> ListenerConfig {
        let ListenerBuilder {
            iface,
            request_timeout_ms,
            cert_config,
            key_config
        } = self;

        let iface = iface.unwrap_or_else(|| {
            DEFAULT_LISTENER_IFACE.to_string()
        });

        ListenerConfig {
            iface,
            request_timeout_ms,
            cert_config,
            key_config,
        }
    }

    #[cfg(not(feature = "https"))]
    #[doc(hidden)]
    #[inline]
    pub(crate )fn build(self) -> ListenerConfig {
        let ListenerBuilder {
            iface,
            request_timeout_ms,
        } = self;

        let iface = iface.unwrap_or_else(|| {
            DEFAULT_LISTENER_IFACE.to_string()
        });

        ListenerConfig {
            iface,
            request_timeout_ms,
        }
    }
}

#[cfg(feature = "https")]
pub struct ListenerConfig {
    iface: String,
    request_timeout_ms: Option<u64>,
    cert_config: Option<SslConfig>,
    key_config: Option<SslConfig>,
}

#[cfg(not(feature = "https"))]
pub struct ListenerConfig {
    iface: String,
    request_timeout_ms: Option<u64>,
}

#[cfg(feature = "https")]
impl ListenerConfig {
    pub(crate) fn ssl_config(&self) -> (Option<&SslConfig>, Option<&SslConfig>) {
        (self.cert_config.as_ref(), self.key_config.as_ref())
    }
}

pub struct Builder<Controllers, Middlewares>
    where
        Controllers: 'static + RouterChain + Unpin + Send + Sync,
        Middlewares: 'static + MiddlewareChain + Unpin + Send + Sync,
{
    listener: Option<ListenerBuilder>,
    router: RouterBuilder<Controllers>,
    middlewares: MiddlewareStackBuilder<Middlewares>,
}

impl<Controllers, Middlewares> Builder<Controllers, Middlewares>
    where
        Controllers: 'static + RouterChain + Unpin + Send + Sync,
        Middlewares: 'static + MiddlewareChain + Unpin + Send + Sync,
{
    #[inline]
    pub fn configure_listener<F>(mut self, f: F) -> Self
        where F: FnOnce(ListenerBuilder) -> ListenerBuilder
    {
        let l = if let Some(builder) = self.listener.take() {
            builder
        } else {
            ListenerBuilder::new()
        };

        self.listener = Some(f(l));

        self
    }

    #[inline]
    pub fn configure_router<F, NewChain: RouterChain + Unpin + Send + Sync>(self, f: F) -> Builder<NewChain, Middlewares>
        where F: FnOnce(RouterBuilder<Controllers>) -> RouterBuilder<NewChain>
    {
        Builder {
            listener: self.listener,
            router: f(self.router),
            middlewares: self.middlewares,
        }
    }

    #[inline]
    pub fn configure_middlewares<F, NewChain: MiddlewareChain + Unpin + Send + Sync>(self, f: F) -> Builder<Controllers, NewChain>
        where F: FnOnce(MiddlewareStackBuilder<Middlewares>) -> MiddlewareStackBuilder<NewChain>
    {
        Builder {
            listener: self.listener,
            router: self.router,
            middlewares: f(self.middlewares),
        }
    }

    pub fn build(self) -> Server {
        Server {
            listener_config: self.listener.unwrap_or_else(|| ListenerBuilder::new()).build(),
            stack: Stack {
                router: self.router.build(),
                middlewares: self.middlewares.build(),
            },
        }
    }
}

pub struct Server {
    listener_config: ListenerConfig,
    stack: Stack,
}

impl Server {
    /// Produce a server builder
    #[inline]
    pub fn builder() -> Builder<RouterChainEnd, MiddleChainEnd> {
        Builder {
            listener: None,
            router: RouterBuilder::default(),
            middlewares: MiddlewareStackBuilder::default(),
        }
    }

    /// Return a future with will run the server. Simply run this future inside the tokio executor
    /// or await it in a async context
    pub async fn run(self) -> Result<(), SaphirError> {
        let Server { listener_config, stack } = self;

        if INIT_STACK.state() != OnceState::New {
            return Err(SaphirError::Other("cannot run a second server".to_owned()));
        }

        INIT_STACK.call_once(|| {
            // # SAFETY #
            // We write only once in the static memory. No override.
            // Above check also make sure there is no second server.
            unsafe { STACK.as_mut_ptr().write(stack); }
        });

        // # SAFETY #
        // Memory has been initialized above.
        let stack = unsafe { STACK.as_ptr().as_ref().expect("Memory has been initialized above.") };

        let http = Http::new();

        let mut listener = TcpListener::bind(listener_config.iface.clone()).await?;
        let local_addr = listener.local_addr()?;

        let incoming = {
            #[cfg(feature = "https")]
                {
                    use crate::server::ssl_loading_utils::MaybeTlsAcceptor;
                    match listener_config.ssl_config() {
                        (Some(cert_config), Some(key_config)) => {
                            use std::sync::Arc;
                            use crate::server::ssl_loading_utils::*;
                            use tokio_rustls::TlsAcceptor;

                            let certs = load_certs(&cert_config);
                            let key = load_private_key(&key_config);
                            let mut cfg = ::rustls::ServerConfig::new(::rustls::NoClientAuth::new());
                            let _ = cfg.set_single_cert(certs, key);
                            let arc_config = Arc::new(cfg);

                            let acceptor = TlsAcceptor::from(arc_config);

                            let inc = listener.incoming().and_then(move |stream| {
                                acceptor.accept(stream)
                            });

                            info!("Saphir started and listening on : https://{}", local_addr);

                            MaybeTlsAcceptor::Tls(Box::pin(inc))
                        }
                        (cert_config, key_config) if cert_config.xor(key_config).is_some() => {
                            return Err(SaphirError::Other("Invalid SSL configuration, missing cert or key".to_string()));
                        }
                        _ => {
                            let incoming = listener.incoming();
                            info!("Saphir started and listening on : http://{}", local_addr);
                            MaybeTlsAcceptor::Plain(Box::pin(incoming))
                        }
                    }
                }

            #[cfg(not(feature = "https"))]
                {
                    info!("Saphir started and listening on : http://{}", local_addr);
                    listener.incoming()
                }
        };

        if let Some(request_timeout_ms) = listener_config.request_timeout_ms {
            use tokio::time::{Duration, timeout};
            incoming.for_each_concurrent(None, |client_socket| async {
                match client_socket {
                    Ok(client_socket) => {
                        let peer_addr = client_socket.peer_addr().ok();
                        let http_handler = http.serve_connection(client_socket, stack.new_handler(peer_addr));
                        let f = timeout(Duration::from_millis(request_timeout_ms), http_handler);

                        tokio::spawn(f);
                    }
                    Err(e) => {
                        warn!("incoming connection encountered an error: {}", e);
                    }
                }
            }).await;
        } else {
            incoming.for_each_concurrent(None, |client_socket| async {
                match client_socket {
                    Ok(client_socket) => {
                        let peer_addr = client_socket.peer_addr().ok();
                        let http_handler = http.serve_connection(client_socket, stack.new_handler(peer_addr));

                        tokio::spawn(http_handler);
                    }
                    Err(e) => {
                        warn!("incoming connection encountered an error: {}", e);
                    }
                }
            }).await;
        }

        Ok(())
    }
}

#[doc(hidden)]
pub struct Stack {
    router: Router,
    middlewares: Box<dyn MiddlewareChain>,
}

unsafe impl Send for Stack {}

unsafe impl Sync for Stack {}

impl Stack {
    fn new_handler(&'static self, peer_addr: Option<SocketAddr>) -> StackHandler {
        StackHandler {
            stack: self,
            peer_addr,
        }
    }

    async fn invoke(&self, req: Request<Body>) -> Result<Response<Body>, SaphirError> {
        let ctx = HttpContext::new(req, self.router.clone());
        self.middlewares.next(ctx).await
    }
}

#[doc(hidden)]
#[derive(Clone)]
pub struct StackHandler {
    stack: &'static Stack,
    peer_addr: Option<SocketAddr>,
}

impl Service<hyper::Request<hyper::Body>> for StackHandler {
    type Response = hyper::Response<hyper::Body>;
    type Error = SaphirError;
    type Future = Box<dyn Future<Output=Result<hyper::Response<hyper::Body>, Self::Error>> + Send + Unpin>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: hyper::Request<hyper::Body>) -> Self::Future {
        let req = Request::new(req, self.peer_addr.take());
        let fut = Box::pin(self.stack.invoke(req).map(|r| r.and_then(|r| r.into_raw())));

        Box::new(fut) as Box<dyn Future<Output=Result<hyper::Response<hyper::Body>, SaphirError>> + Send + Unpin>
    }
}


#[doc(hidden)]
#[cfg(feature = "https")]
mod ssl_loading_utils {
    use rustls;
    use std::fs;
    use std::io::BufReader;
    use crate::server::SslConfig;
    use futures_util::stream::Stream;
    use futures_util::task::{Context, Poll};
    use std::pin::Pin;
    use futures::io::Error;
    use std::net::SocketAddr;
    use tokio::io::{AsyncRead, AsyncWrite};

    pub enum MaybeTlsStream {
        Tls(Pin<Box<tokio_rustls::server::TlsStream<tokio::net::TcpStream>>>),
        Plain(Pin<Box<tokio::net::TcpStream>>),
    }

    impl MaybeTlsStream {
        pub fn peer_addr(&self) -> Result<SocketAddr, tokio::io::Error> {
            match self {
                MaybeTlsStream::Tls(t) => t.as_ref().get_ref().get_ref().0.peer_addr(),
                MaybeTlsStream::Plain(p) => p.as_ref().get_ref().peer_addr(),
            }
        }
    }

    impl AsyncRead for MaybeTlsStream {
        fn poll_read(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut [u8]) -> Poll<Result<usize, Error>> {
            match self.get_mut() {
                MaybeTlsStream::Tls(t) => t.as_mut().poll_read(cx, buf),
                MaybeTlsStream::Plain(p) => p.as_mut().poll_read(cx, buf),
            }
        }
    }

    impl AsyncWrite for MaybeTlsStream {
        fn poll_write(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<Result<usize, Error>> {
            match self.get_mut() {
                MaybeTlsStream::Tls(t) => t.as_mut().poll_write(cx, buf),
                MaybeTlsStream::Plain(p) => p.as_mut().poll_write(cx, buf),
            }
        }

        fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Error>> {
            match self.get_mut() {
                MaybeTlsStream::Tls(t) => t.as_mut().poll_flush(cx),
                MaybeTlsStream::Plain(p) => p.as_mut().poll_flush(cx),
            }
        }

        fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Error>> {
            match self.get_mut() {
                MaybeTlsStream::Tls(t) => t.as_mut().poll_shutdown(cx),
                MaybeTlsStream::Plain(p) => p.as_mut().poll_shutdown(cx),
            }
        }
    }

    pub enum MaybeTlsAcceptor<'a, S: Stream<Item=Result<tokio_rustls::server::TlsStream<tokio::net::TcpStream>, tokio::io::Error>>> {
        Tls(Pin<Box<S>>),
        Plain(Pin<Box<tokio::net::tcp::Incoming<'a>>>),
    }

    impl<'a, S: Stream<Item=Result<tokio_rustls::server::TlsStream<tokio::net::TcpStream>, tokio::io::Error>>> Stream for MaybeTlsAcceptor<'a, S> {
        type Item = Result<MaybeTlsStream, tokio::io::Error>;

        fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
            match self.get_mut() {
                MaybeTlsAcceptor::Tls(tls) => tls.as_mut().poll_next(cx).map(|t| t.map(|tls_res| tls_res.map(|tls| MaybeTlsStream::Tls(Box::pin(tls))))),
                MaybeTlsAcceptor::Plain(plain) => plain.as_mut().poll_next(cx).map(|t| t.map(|tls_res| tls_res.map(|tls| MaybeTlsStream::Plain(Box::pin(tls))))),
            }
        }
    }

    pub fn load_certs(cert_config: &SslConfig) -> Vec<rustls::Certificate> {
        match cert_config {
            SslConfig::FilePath(filename) => {
                let certfile = fs::File::open(filename).expect("cannot open certificate file");
                let mut reader = BufReader::new(certfile);
                rustls::internal::pemfile::certs(&mut reader).expect("Unable to load certificate from file")
            }
            SslConfig::FileData(data) => {
                extract_der_data(data.to_string(),
                                 "-----BEGIN CERTIFICATE-----",
                                 "-----END CERTIFICATE-----",
                                 &|v| rustls::Certificate(v))
                    .expect("Unable to load certificate from data")
            }
        }
    }

    pub fn load_private_key(key_config: &SslConfig) -> rustls::PrivateKey {
        match key_config {
            SslConfig::FilePath(filename) => {
                load_private_key_from_file(&filename)
            }
            SslConfig::FileData(data) => {
                let pkcs8_keys = load_pkcs8_private_key_from_data(data);

                if !pkcs8_keys.is_empty() {
                    pkcs8_keys[0].clone()
                } else {
                    let rsa_keys = load_rsa_private_key_from_data(data);
                    assert!(!rsa_keys.is_empty(), "Unable to load key");
                    rsa_keys[0].clone()
                }
            }
        }
    }

    fn load_private_key_from_file(filename: &str) -> rustls::PrivateKey {
        let rsa_keys = {
            let keyfile = fs::File::open(filename)
                .expect("cannot open private key file");
            let mut reader = BufReader::new(keyfile);
            rustls::internal::pemfile::rsa_private_keys(&mut reader)
                .expect("file contains invalid rsa private key")
        };

        let pkcs8_keys = {
            let keyfile = fs::File::open(filename)
                .expect("cannot open private key file");
            let mut reader = BufReader::new(keyfile);
            rustls::internal::pemfile::pkcs8_private_keys(&mut reader)
                .expect("file contains invalid pkcs8 private key (encrypted keys not supported)")
        };

        // prefer to load pkcs8 keys
        if !pkcs8_keys.is_empty() {
            pkcs8_keys[0].clone()
        } else {
            assert!(!rsa_keys.is_empty(), "Unable to load key");
            rsa_keys[0].clone()
        }
    }

    fn load_pkcs8_private_key_from_data(data: &str) -> Vec<rustls::PrivateKey> {
        extract_der_data(data.to_string(),
                         "-----BEGIN PRIVATE KEY-----",
                         "-----END PRIVATE KEY-----",
                         &|v| rustls::PrivateKey(v))
            .expect("Unable to load private key from data")
    }

    fn load_rsa_private_key_from_data(data: &str) -> Vec<rustls::PrivateKey> {
        extract_der_data(data.to_string(),
                         "-----BEGIN RSA PRIVATE KEY-----",
                         "-----END RSA PRIVATE KEY-----",
                         &|v| rustls::PrivateKey(v))
            .expect("Unable to load private key from data")
    }

    fn extract_der_data<A>(mut data: String,
                           start_mark: &str,
                           end_mark: &str,
                           f: &dyn Fn(Vec<u8>) -> A)
                           -> Result<Vec<A>, ()> {
        let mut ders = Vec::new();

        loop {
            if let Some(start_index) = data.find(start_mark) {
                let drain_index = start_index + start_mark.len();
                data.drain(..drain_index);
                if let Some(index) = data.find(end_mark) {
                    let base64_buf = &data[..index];
                    let der = base64::decode(&base64_buf).map_err(|_| ())?;
                    ders.push(f(der));

                    let drain_index = index + end_mark.len();
                    data.drain(..drain_index);
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        Ok(ders)
    }
}