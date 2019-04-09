use http::request::Parts;
use hashbrown::hash_map::HashMap;
use std::collections::VecDeque;
use http::{Method, Uri, Version, HeaderMap, Extensions};
use http::header::HeaderValue;
use crate::utils::UriPathMatcher;
use hyper::Body;
use futures::Future;
use futures::stream::Stream;
use std::fmt::{Debug, Formatter};

///
pub type SyncRequest = Request<Vec<u8>>;

///
pub struct Request<B> {
    ///
    head: Parts,
    ///
    body: B,
    /// Request Params
    current_path: VecDeque<String>,
    ///
    captures: HashMap<String, String>,
}

impl<B> Request<B> {
    #[inline]
    pub(crate) fn new(head: Parts, body: B) -> Request<B> {
        let mut current_path: VecDeque<String> = head.uri.path().to_owned().split('/').map(|s| s.to_owned()).collect();
        current_path.pop_front();
        if current_path.back().map(|s| s.len()).unwrap_or(0) < 1 {
            current_path.pop_back();
        }
        Request {
            head,
            body,
            current_path,
            captures: HashMap::new(),
        }
    }

    ///
    #[inline]
    pub fn method(&self) -> &Method {
        &self.head.method
    }

    ///
    #[inline]
    pub fn method_mut(&mut self) -> &mut Method {
        &mut self.head.method
    }

    ///
    #[inline]
    pub fn uri(&self) -> &Uri {
        &self.head.uri
    }

    ///
    #[inline]
    pub fn uri_mut(&mut self) -> &mut Uri {
        &mut self.head.uri
    }

    ///
    #[inline]
    pub fn captures(&self) -> &HashMap<String, String> {
        &self.captures
    }

    ///
    #[inline]
    pub fn version(&self) -> Version {
        self.head.version
    }

    ///
    #[inline]
    pub fn version_mut(&mut self) -> &mut Version {
        &mut self.head.version
    }

    ///
    #[inline]
    pub fn headers_map(&self) -> &HeaderMap<HeaderValue> {
        &self.head.headers
    }

    ///
    #[inline]
    pub fn headers_map_mut(&mut self) -> &mut HeaderMap<HeaderValue> {
        &mut self.head.headers
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

    ///
    #[inline]
    pub fn body(&self) -> &B {
        &self.body
    }

    ///
    #[inline]
    pub fn body_mut(&mut self) -> &mut B {
        &mut self.body
    }

    ///
    pub(crate) fn current_path_match(&mut self, path: &UriPathMatcher) -> bool {
        let mut current_path = self.current_path.iter();
        // validate path
        for seg in path.iter() {
            if let Some(current) = current_path.next() {
                if !seg.matches(current) {
                    return false;
                }
            } else {
                return false;
            }
        }

        // Alter current path and capture path variable
        {
            for seg in path.iter() {
                if let Some(current) = self.current_path.pop_front() {
                    if let Some(name) = seg.name() {
                        self.captures.insert(name.to_string(), current);
                    }
                }
            }
        }

        true
    }

    ///
    pub(crate) fn current_path_match_all(&mut self, path: &UriPathMatcher) -> bool {
        if path.len() != self.current_path.len() {
            return false;
        }

        let mut current_path = self.current_path.iter();
        // validate path
        for seg in path.iter() {
            if let Some(current) = current_path.next() {
                if !seg.matches(current) {
                    return false;
                }
            } else {
                return false;
            }
        }

        // Alter current path and capture path variable
        {
            for seg in path.iter() {
                if let Some(current) = self.current_path.pop_front() {
                    if let Some(name) = seg.name() {
                        self.captures.insert(name.to_string(), current);
                    }
                }
            }
        }

        true
    }
}

impl<B> Debug for Request<B> where B: Debug {
    fn fmt(&self, f: &mut Formatter) -> Result<(), std::fmt::Error> {
        f.debug_struct("Request").field("head", &self.head).field("captures", &self.captures).field("body", &self.body).finish()
    }
}

/// A trait allowing the implicit conversion of a Hyper::Request into a SyncRequest
pub trait LoadBody {
    ///
    fn load_body(self) -> Box<Future<Item=SyncRequest, Error=::hyper::Error> + Send>;
}

impl LoadBody for hyper::Request<Body> {
    fn load_body(self) -> Box<Future<Item=SyncRequest, Error=::hyper::Error> + Send> {
        let (parts, body) = self.into_parts();
        Box::new(body.concat2().map(move |b| {
            let body_vec: Vec<u8> = b.to_vec();
            SyncRequest::new(parts, body_vec)
        }))
    }
}