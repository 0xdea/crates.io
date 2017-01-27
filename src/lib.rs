extern crate semver;
extern crate conduit;

use std::collections::HashMap;
use std::io::prelude::*;
use std::io::Cursor;
use std::net::{SocketAddr, Ipv4Addr, SocketAddrV4};

use semver::Version;
use conduit::{Method, Scheme, Host, Extensions, Headers, TypeMap};

pub struct MockRequest {
    path: String,
    method: Method,
    query_string: Option<String>,
    body: Option<Vec<u8>>,
    build_headers: HashMap<String, String>,
    headers: MockHeaders,
    extensions: TypeMap,
    reader: Option<Cursor<Vec<u8>>>
}

impl MockRequest {
    pub fn new(method: Method, path: &str) -> MockRequest {
        let headers = HashMap::new();

        MockRequest {
            path: path.to_string(),
            extensions: TypeMap::new(),
            query_string: None,
            body: None,
            build_headers: headers,
            headers: MockHeaders { headers: HashMap::new() },
            method: method,
            reader: None
        }
    }

    pub fn with_method(&mut self, method: Method) -> &mut MockRequest {
        self.method = method;
        self
    }

    pub fn with_path(&mut self, path: &str) -> &mut MockRequest {
        self.path = path.to_string();
        self
    }

    pub fn with_query(&mut self, string: &str) -> &mut MockRequest {
        self.query_string = Some(string.to_string());
        self
    }

    pub fn with_body(&mut self, bytes: &[u8]) -> &mut MockRequest {
        self.body = Some(bytes.to_vec());
        self.reader = None;
        self
    }

    pub fn header(&mut self, name: &str, value: &str) -> &mut MockRequest {
        self.build_headers.insert(name.to_string(), value.to_string());
        let headers = MockHeaders { headers: self.build_headers.clone() };
        self.headers = headers;

        self
    }
}

pub struct MockHeaders {
    headers: HashMap<String, String>
}

impl Headers for MockHeaders {
    fn find(&self, key: &str) -> Option<Vec<&str>> {
        self.headers.get(key).map(|v| vec![&v[..]])
    }

    fn has(&self, key: &str) -> bool {
        self.headers.contains_key(key)
    }

    fn all(&self) -> Vec<(&str, Vec<&str>)> {
        self.headers.iter().map(|(k,v)| (&k[..], vec![&v[..]]))
                    .collect()
    }
}

impl conduit::Request for MockRequest {
    fn http_version(&self) -> Version {
        Version::parse("1.1.0").unwrap()
    }

    fn conduit_version(&self) -> Version {
        Version::parse("0.1.0").unwrap()
    }

    fn method(&self) -> Method { self.method.clone() }
    fn scheme(&self) -> Scheme { Scheme::Http }
    fn host(&self) -> Host { Host::Name("example.com") }
    fn virtual_root(&self) -> Option<&str> { None }

    fn path(&self) -> &str {
        &self.path
    }

    fn query_string(&self) -> Option<&str> {
        self.query_string.as_ref().map(|s| &s[..])
    }

    fn remote_addr(&self) -> SocketAddr {
        SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 80))
    }

    fn content_length(&self) -> Option<u64> {
        self.body.as_ref().map(|b| b.len() as u64)
    }

    fn headers(&self) -> &Headers {
        &self.headers as &Headers
    }

    fn body(&mut self) -> &mut Read {
        if self.reader.is_none() {
            let body = self.body.clone().unwrap_or(Vec::new());
            self.reader = Some(Cursor::new(body));
        }
        self.reader.as_mut().unwrap()
    }

    fn extensions(&self) -> &Extensions {
        &self.extensions
    }
    fn mut_extensions(&mut self) -> &mut Extensions {
        &mut self.extensions
    }
}

#[cfg(test)]
mod tests {
    use super::MockRequest;
    use semver::Version;

    use std::net::{SocketAddr, SocketAddrV4, Ipv4Addr};

    use conduit::{Request, Method, Host, Scheme};

    #[test]
    fn simple_request_test() {
        let mut req = MockRequest::new(Method::Get, "/");

        assert_eq!(req.http_version(), Version::parse("1.1.0").unwrap());
        assert_eq!(req.conduit_version(), Version::parse("0.1.0").unwrap());
        assert_eq!(req.method(), Method::Get);
        assert_eq!(req.scheme(), Scheme::Http);
        assert_eq!(req.host(), Host::Name("example.com"));
        assert_eq!(req.virtual_root(), None);
        assert_eq!(req.path(), "/");
        assert_eq!(req.query_string(), None);
        assert_eq!(req.remote_addr(),
                   SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1),
                                                    80)));
        assert_eq!(req.content_length(), None);
        assert_eq!(req.headers().all().len(), 0);
        let mut s = String::new();
        req.body().read_to_string(&mut s).ok().expect("No body");
        assert_eq!(s, "".to_string());
    }

    #[test]
    fn request_body_test() {
        let mut req = MockRequest::new(Method::Post, "/articles");
        req.with_body(b"Hello world");

        assert_eq!(req.method(), Method::Post);
        assert_eq!(req.path(), "/articles");
        let mut s = String::new();
        req.body().read_to_string(&mut s).ok().expect("No body");
        assert_eq!(s, "Hello world".to_string());
        assert_eq!(req.content_length(), Some(11));
    }

    #[test]
    fn request_query_test() {
        let mut req = MockRequest::new(Method::Post, "/articles");
        req.with_query("foo=bar");

        assert_eq!(req.query_string().expect("No query string"), "foo=bar");
    }

    #[test]
    fn request_headers() {
        let mut req = MockRequest::new(Method::Post, "/articles");
        req.header("User-Agent", "lulz");
        req.header("DNT", "1");

        assert_eq!(req.headers().all().len(), 2);
        assert_eq!(req.headers().find("User-Agent").unwrap(), vec!("lulz"));
        assert_eq!(req.headers().find("DNT").unwrap(), vec!("1"));
    }
}
