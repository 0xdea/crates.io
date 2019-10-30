use super::Service;

use std::future::Future;
use std::net::SocketAddr;
use std::sync::Arc;

use hyper::service;

/// A builder for a `hyper::Server` (behind an opaque `impl Future`).
#[derive(Debug)]
pub struct Server;

impl Server {
    /// Bind a handler to an address.
    ///
    /// This returns an opaque `impl Future` so while it can be directly spawned on a
    /// `tokio::Runtime` it is not possible to furter configure the `hyper::Server`.  If more
    /// control, such as configuring a graceful shutdown is necessary, then call
    /// `Service::from_conduit` instead.
    pub fn bind<H: conduit::Handler>(addr: &SocketAddr, handler: H) -> impl Future {
        use hyper::server::conn::AddrStream;
        use service::make_service_fn;

        let handler = Arc::new(handler);

        let make_service = make_service_fn(move |socket: &AddrStream| {
            let handler = handler.clone();
            let remote_addr = socket.remote_addr();
            async move { Service::from_conduit(handler, remote_addr) }
        });

        hyper::Server::bind(&addr).serve(make_service)
    }
}
