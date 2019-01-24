use http::request::{Request as HttpRequest, Parts as HttpRequestParts};
use http::{Method, Uri, Version, Extensions};
use http::header::{HeaderValue, HeaderMap, AsHeaderName, IntoHeaderName};
use crate::utils::HeaderFormatter;

///
#[derive(Debug)]
pub enum HttpBody {
    ///
    Json,
    ///
    Form,
    ///
    Xml,
    ///
    Data(hyper::Body),
    ///
    Binary(Vec<u8>),

}

impl HttpBody {
    #[doc(hidden)] pub(crate) fn into_hyper_body(self) -> hyper::Body {
        match self {
            HttpBody::Data(b) => b,
            HttpBody::Binary(bin) => hyper::Body::from(bin),
            _ => hyper::Body::empty(),
        }
    }
}

///
#[derive(Debug)]
pub struct Request {
    /// The request's method, URI,
    /// version,headers, extensions
    #[doc(hidden)] head: HttpRequestParts,
    #[doc(hidden)] current_path: String,
    #[doc(hidden)] captures: Vec<String>,
    #[doc(hidden)] body: Option<HttpBody>
}

impl Request {
    /// Returns a reference to the HTTP Method of the request
    #[inline]
    pub fn method(&self) -> &Method {
        &self.head.method
    }

    /// Returns a mutable reference to the HTTP Method of the request
    #[inline]
    pub fn method_mut(&mut self) -> &mut Method {
        &mut self.head.method
    }

    /// Returns a reference to the URI of the request
    #[inline]
    pub fn uri(&self) -> &Uri {
        &self.head.uri
    }

    /// Returns a mutable reference to the URI of the request
    #[inline]
    pub fn uri_mut(&mut self) -> &mut Uri {
        &mut self.head.uri
    }

    /// Returns the captures done by applying regex to the path
    #[inline]
    pub fn captures(&self) -> &Vec<String> {
        &self.captures
    }

    /// Returns a reference to the HTTP Version of the request
    #[inline]
    pub fn version(&self) -> Version {
        self.head.version
    }

    /// Returns a mutable reference to the HTTP Version of the request
    #[inline]
    pub fn version_mut(&mut self) -> &mut Version {
        &mut self.head.version
    }

    /// Returns a reference to the HTTP Headers as a Map
    #[inline]
    pub fn headers_map(&self) -> &HeaderMap<HeaderValue> {
        &self.head.headers
    }

    /// Returns a mutable reference to the HTTP Headers as a Map
    #[inline]
    pub fn headers_map_mut(&mut self) -> &mut HeaderMap<HeaderValue> {
        &mut self.head.headers
    }

    /// Returns a single header value if it is present in the request
    #[inline]
    pub fn get(&self, header: impl AsHeaderName) -> Option<&HeaderValue> {
        self.head.headers.get(header)
    }

    /// Returns a single header value, formatted by `H` trait implementation of `HeaderFormatter`
    /// if it is present in the request.
    #[inline]
    pub fn get_header<H: HeaderFormatter>(&self) -> Option<H> {
        self.head.headers.get(H::NAME).map(|h| H::from_value(h))
    }

    /// Add or set a single header value
    #[inline]
    pub fn set(&mut self, header: impl IntoHeaderName, value: HeaderValue) {
        self.head.headers.insert(header, value);
    }

    /// Add or set a single header value from an implementor of `HeaderFormatter`
    #[inline]
    pub fn set_header_val<H: HeaderFormatter>(&mut self, header: H) {
        self.head.headers.insert(H::NAME, header.into_value());
    }

    /// Returns a reference to the Body
    #[inline]
    pub fn body(&self) -> &HttpBody {
        &self.body.as_ref().take().expect("This should never happend")
    }

    /// Returns a mutable reference to the Body
    #[inline]
    pub fn body_mut(&mut self) -> &mut HttpBody {
        self.body.as_mut().take().expect("This should never happend")
    }

    /// Returns a the owned value of the body
    /// # Warning
    /// Calling this method twice will result in a panic.
    #[inline]
    pub fn take_body(&mut self) -> HttpBody {
        self.body.take().expect("`take_body` shall only be called once, this is fatal")
    }

    /// Map the request body from a type to an other
    pub fn map_body<F>(&mut self, f: F) where F: FnOnce(HttpBody) -> HttpBody {
        let body = self.take_body();
        self.body = Some(f(body));
    }

    ///
    #[inline]
    pub fn extensions(&self) -> &Extensions {
        &self.head.extensions
    }

    ///
    #[inline]
    pub fn extensions_mut(&mut self) -> &mut Extensions {
        &mut self.head.extensions
    }

    #[allow(dead_code)]
    #[doc(hidden)] pub(crate) fn current_path_match(&mut self, re: &::regex::Regex) -> bool {
        let current = self.current_path.clone();
        re.find(&current).map_or_else(|| false, |ma| {
            let mut path = self.current_path.split_off(ma.end());
            if path.len() < 1 {
                path.push('/');
            }
            self.current_path = path;
            true
        })
    }

    #[allow(dead_code)]
    #[doc(hidden)] pub(crate) fn current_path_match_and_capture(&mut self, re: &::regex::Regex) -> bool {
        let current = self.current_path.clone();
        re.captures(&current).map_or_else(|| false, |cap| {
            if let Some(ma) = cap.get(0) {
                let mut path = self.current_path.split_off(ma.end());
                if path.len() < 1 {
                    path.push('/');
                }
                self.current_path = path;
            }

            for i in 1..cap.len() {
                if let Some(ma) = cap.get(i) {
                    self.captures.push(ma.as_str().to_owned())
                }
            }

            true
        })
    }

    #[allow(dead_code)]
    #[doc(hidden)] pub(crate) fn from_http_request_parts(head: HttpRequestParts, b: hyper::Body) -> Self {
        let cp = head.uri.path().to_string();
        Request {
            head,
            current_path: cp,
            captures: vec![],
            body: Some(HttpBody::Data(b))
        }
    }

    #[allow(dead_code)]
    #[doc(hidden)] pub(crate) fn from_http_request(req: HttpRequest<hyper::Body>) -> Self {
        let (h, b) = req.into_parts();
        let cp = h.uri.path().to_string();
        Request {
            head: h,
            current_path: cp,
            captures: vec![],
            body: Some(HttpBody::Data(b))
        }
    }

    #[allow(dead_code)]
    #[doc(hidden)] pub(crate) fn take_parts(self) -> (HttpRequestParts, HttpBody) {
        let Request {
            head,
            current_path: _,
            captures: _,
            mut body
        } = self;

        (head, body.take().unwrap_or_else(|| HttpBody::Data(hyper::Body::empty())))
    }
}

///
#[derive(Debug)]
pub struct BinaryRequest {
    /// The request's method, URI,
    /// version,headers, extensions
    #[doc(hidden)] head: HttpRequestParts,
    #[doc(hidden)] current_path: String,
    #[doc(hidden)] captures: Vec<String>,
    #[doc(hidden)] body: Option<Vec<u8>>
}

impl BinaryRequest {
    /// Returns a reference to the HTTP Method of the request
    #[inline]
    pub fn method(&self) -> &Method {
        &self.head.method
    }

    /// Returns a mutable reference to the HTTP Method of the request
    #[inline]
    pub fn method_mut(&mut self) -> &mut Method {
        &mut self.head.method
    }

    /// Returns a reference to the URI of the request
    #[inline]
    pub fn uri(&self) -> &Uri {
        &self.head.uri
    }

    /// Returns a mutable reference to the URI of the request
    #[inline]
    pub fn uri_mut(&mut self) -> &mut Uri {
        &mut self.head.uri
    }

    /// Returns the captures done by applying regex to the path
    #[inline]
    pub fn captures(&self) -> &Vec<String> {
        &self.captures
    }

    /// Returns a reference to the HTTP Version of the request
    #[inline]
    pub fn version(&self) -> Version {
        self.head.version
    }

    /// Returns a mutable reference to the HTTP Version of the request
    #[inline]
    pub fn version_mut(&mut self) -> &mut Version {
        &mut self.head.version
    }

    /// Returns a reference to the HTTP Headers as a Map
    #[inline]
    pub fn headers_map(&self) -> &HeaderMap<HeaderValue> {
        &self.head.headers
    }

    /// Returns a mutable reference to the HTTP Headers as a Map
    #[inline]
    pub fn headers_map_mut(&mut self) -> &mut HeaderMap<HeaderValue> {
        &mut self.head.headers
    }

    /// Returns a single header value if it is present in the request
    #[inline]
    pub fn get(&self, header: impl AsHeaderName) -> Option<&HeaderValue> {
        self.head.headers.get(header)
    }

    /// Returns a single header value, formatted by `H` trait implementation of `HeaderFormatter`
    /// if it is present in the request.
    #[inline]
    pub fn get_header<H: HeaderFormatter>(&self) -> Option<H> {
        self.head.headers.get(H::NAME).map(|h| H::from_value(h))
    }

    /// Add or set a single header value
    #[inline]
    pub fn set(&mut self, header: impl IntoHeaderName, value: HeaderValue) {
        self.head.headers.insert(header, value);
    }

    /// Add or set a single header value from an implementor of `HeaderFormatter`
    #[inline]
    pub fn set_header_val<H: HeaderFormatter>(&mut self, header: H) {
        self.head.headers.insert(H::NAME, header.into_value());
    }

    /// Returns a reference to the Body
    #[inline]
    pub fn body(&self) -> &Vec<u8> {
        &self.body.as_ref().take().expect("This should never happend")
    }

    /// Returns a mutable reference to the Body
    #[inline]
    pub fn body_mut(&mut self) -> &mut Vec<u8> {
        self.body.as_mut().take().expect("This should never happend")
    }

    /// Returns a the owned value of the body
    /// # Warning
    /// Calling this method twice will result in a panic.
    #[inline]
    pub fn take_body(&mut self) -> Vec<u8> {
        self.body.take().expect("`take_body` shall only be called once, this is fatal")
    }

    ///
    #[inline]
    pub fn extensions(&self) -> &Extensions {
        &self.head.extensions
    }

    ///
    #[inline]
    pub fn extensions_mut(&mut self) -> &mut Extensions {
        &mut self.head.extensions
    }

    #[doc(hidden)]
    pub(crate) fn current_path_match(&mut self, re: &::regex::Regex) -> bool {
        let current = self.current_path.clone();
        re.find(&current).map_or_else(|| false, |ma| {
            let mut path = self.current_path.split_off(ma.end());
            if path.len() < 1 {
                path.push('/');
            }
            self.current_path = path;
            true
        })
    }

    #[doc(hidden)]
    pub(crate) fn current_path_match_and_capture(&mut self, re: &::regex::Regex) -> bool {
        let current = self.current_path.clone();
        re.captures(&current).map_or_else(|| false, |cap| {
            if let Some(ma) = cap.get(0) {
                let mut path = self.current_path.split_off(ma.end());
                if path.len() < 1 {
                    path.push('/');
                }
                self.current_path = path;
            }

            for i in 1..cap.len() {
                if let Some(ma) = cap.get(i) {
                    self.captures.push(ma.as_str().to_owned())
                }
            }

            true
        })
    }

    #[doc(hidden)]
    pub(crate) fn from_http_request_parts(head: HttpRequestParts, b: Vec<u8>) -> Self {
        let cp = head.uri.path().to_string();
        BinaryRequest {
            head,
            current_path: cp,
            captures: vec![],
            body: Some(b)
        }
    }
}