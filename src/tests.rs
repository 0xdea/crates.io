use std::collections::HashMap;
use std::error::Error;
use std::io::Cursor;

use conduit::{Handler, Request, Response};
use futures::executor;
use futures::prelude::*;
use hyper::service::Service;

struct OkResult;
impl Handler for OkResult {
    fn call(&self, _req: &mut dyn Request) -> Result<Response, Box<dyn Error + Send>> {
        Ok(Response {
            status: (200, "OK"),
            headers: build_headers("value"),
            body: Box::new(Cursor::new("Hello, world!")),
        })
    }
}

struct ErrorResult;
impl Handler for ErrorResult {
    fn call(&self, _req: &mut dyn Request) -> Result<Response, Box<dyn Error + Send>> {
        let error = ::std::io::Error::last_os_error();
        Err(Box::new(error))
    }
}

struct Panic;
impl Handler for Panic {
    fn call(&self, _req: &mut dyn Request) -> Result<Response, Box<dyn Error + Send>> {
        panic!()
    }
}

struct InvalidHeader;
impl Handler for InvalidHeader {
    fn call(&self, _req: &mut dyn Request) -> Result<Response, Box<dyn Error + Send>> {
        let mut headers = build_headers("discarded");
        headers.insert("invalid".into(), vec!["\r\n".into()]);
        Ok(Response {
            status: (200, "OK"),
            headers,
            body: Box::new(Cursor::new("discarded")),
        })
    }
}

struct InvalidStatus;
impl Handler for InvalidStatus {
    fn call(&self, _req: &mut dyn Request) -> Result<Response, Box<dyn Error + Send>> {
        Ok(Response {
            status: (1000, "invalid status code"),
            headers: build_headers("discarded"),
            body: Box::new(Cursor::new("discarded")),
        })
    }
}

struct AssertPathNormalized;
impl Handler for AssertPathNormalized {
    fn call(&self, req: &mut dyn Request) -> Result<Response, Box<dyn Error + Send>> {
        if req.path() == "/normalized" {
            OkResult.call(req)
        } else {
            ErrorResult.call(req)
        }
    }
}

fn build_headers(msg: &str) -> HashMap<String, Vec<String>> {
    let mut headers = HashMap::new();
    headers.insert("ok".into(), vec![msg.into()]);
    headers
}

fn block_on<F>(future: F)
where
    F: Future<Output = ()> + Send + 'static,
{
    let rt = tokio::runtime::Builder::new()
        .core_threads(1)
        .blocking_threads(1)
        .build()
        .unwrap();
    rt.spawn(future);
    executor::block_on(rt.shutdown_on_idle());
}

fn make_service<H: Handler>(
    handler: H,
) -> impl Service<
    ReqBody = hyper::Body,
    ResBody = hyper::Body,
    Future = impl Future<Output = Result<hyper::Response<hyper::Body>, hyper::Error>> + Send + 'static,
    Error = hyper::Error,
> {
    use hyper::service::service_fn;

    let handler = std::sync::Arc::new(handler);

    service_fn(move |request: hyper::Request<hyper::Body>| {
        let remote_addr = ([0, 0, 0, 0], 0).into();
        super::blocking_handler(handler.clone(), request, remote_addr)
    })
}

async fn simulate_request<H: Handler>(handler: H) -> hyper::Response<hyper::Body> {
    let mut service = make_service(handler);
    service.call(hyper::Request::default()).await.unwrap()
}

async fn into_chunk(resp: hyper::Response<hyper::Body>) -> hyper::Chunk {
    resp.into_body().try_concat().await.unwrap()
}

async fn assert_generic_err(resp: hyper::Response<hyper::Body>) {
    assert_eq!(resp.status(), 500);
    assert!(resp.headers().is_empty());
    let full_body = into_chunk(resp).await;
    assert_eq!(&*full_body, b"Internal Server Error");
}

#[test]
fn valid_ok_response() {
    block_on(async {
        let resp = simulate_request(OkResult).await;
        assert_eq!(resp.status(), 200);
        assert_eq!(resp.headers().len(), 1);
        let full_body = into_chunk(resp).await;
        assert_eq!(&*full_body, b"Hello, world!");
    })
}

#[test]
fn invalid_ok_responses() {
    block_on(async {
        assert_generic_err(simulate_request(InvalidHeader).await).await;
        assert_generic_err(simulate_request(InvalidStatus).await).await;
    })
}

#[test]
fn err_responses() {
    block_on(async {
        assert_generic_err(simulate_request(ErrorResult).await).await;
    })
}

#[ignore] // catch_unwind not yet implemented
#[test]
fn recover_from_panic() {
    block_on(async {
        assert_generic_err(simulate_request(Panic).await).await;
    })
}

#[test]
fn normalize_path() {
    block_on(async {
        let mut service = make_service(AssertPathNormalized);
        let req = hyper::Request::put("//removed/.././.././normalized")
            .body(hyper::Body::default())
            .unwrap();
        let resp = service.call(req).await.unwrap();
        assert_eq!(resp.status(), 200);
        assert_eq!(resp.headers().len(), 1);

        let req = hyper::Request::put("//normalized")
            .body(hyper::Body::default())
            .unwrap();
        let resp = service.call(req).await.unwrap();
        assert_eq!(resp.status(), 200);
        assert_eq!(resp.headers().len(), 1);
    })
}
