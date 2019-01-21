use http::response::{Response as HttpResponse, Builder as HttpResponseBuilder};
use http::{StatusCode, Version, HttpTryFrom};
use http::header::{HeaderValue, HeaderName};
use crate::utils::HeaderFormatter;
use hyper::body::Body;
use std::any::Any;

///
pub struct ResponseBuilder {
    #[doc(hidden)] builder: HttpResponseBuilder,
    #[doc(hidden)] body: Option<Body>,
}

impl ResponseBuilder {
    ///
    pub fn new() -> Self {
        ResponseBuilder {
            builder: HttpResponseBuilder::new(),
            body: None,
        }
    }

    ///
    pub fn status<T>(&mut self, status: T) -> &mut ResponseBuilder
        where StatusCode: HttpTryFrom<T>,
    {
        self.builder.status(status);
        self
    }

    ///
    pub fn version(&mut self, version: Version) -> &mut ResponseBuilder {
        self.builder.version(version);
        self
    }

    /// Add or set a single header value
    #[inline]
    pub fn set<K, V>(&mut self, header: K, value: V) -> &mut ResponseBuilder
        where HeaderName: HttpTryFrom<K>,
              HeaderValue: HttpTryFrom<V> {
        self.builder.header(header, value);
        self
    }

    /// Add or set a single header value from an implementor of `HeaderFormatter`
    #[inline]
    pub fn set_header_val<H: HeaderFormatter>(&mut self, header: H) -> &mut ResponseBuilder {
        self.builder.header(H::NAME, header.into_value());
        self
    }

    ///
    pub fn extension<T>(&mut self, extension: T) -> &mut ResponseBuilder
        where T: Any + Send + Sync + 'static,
    {
        self.builder.extension(extension);
        self
    }

    ///
    pub fn body<B: 'static + ToBody>(&mut self, body: B) -> &mut ResponseBuilder {
        self.body = Some(body.to_body());
        self
    }

    #[doc(hidden)]
    pub(crate) fn build(self) -> Result<HttpResponse<Body>, String> {
        let ResponseBuilder {
            mut builder,
            body,
        } = self;

        builder.body(body.unwrap_or(Body::empty()).to_body()).map_err(|er| er.to_string())
    }
}


#[doc(hidden)]
pub trait ToBody {
    #[doc(hidden)]
    fn to_body(self) -> Body;
}

impl<I> ToBody for I where I: Into<Body> {
    fn to_body(self) -> Body {
        self.into()
    }
}