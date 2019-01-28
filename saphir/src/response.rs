use http::response::{Response as HttpResponse, Builder as HttpResponseBuilder};
use http::{StatusCode, Version, HttpTryFrom};
use http::header::{HeaderValue, HeaderName};
use futures::prelude::*;
use crate::utils::HeaderFormatter;
use crate::Request;
use log::error;
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
    pub fn set<K, V>(&mut self, header: K, value: V) -> &mut ResponseBuilder
        where HeaderName: HttpTryFrom<K>,
              HeaderValue: HttpTryFrom<V> {
        self.builder.header(header, value);
        self
    }

    /// Add or set a single header value from an implementor of `HeaderFormatter`
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
    #[inline]
    fn to_body(self) -> Body;
}

impl<I> ToBody for I where I: Into<Body> {
    fn to_body(self) -> Body {
        self.into()
    }
}

#[doc(hidden)]
pub struct ResponseBuilderFuture(Box<Future<Item=ResponseBuilder, Error=()> + Send>);

impl Future for ResponseBuilderFuture {
    type Item = ResponseBuilder;
    type Error = ();
    fn poll(&mut self) -> Result<Async<Self::Item>, Self::Error> { self.0.poll() }
}

#[doc(hidden)]
pub struct ResponseFuture(Box<Future<Item=HttpResponse<Body>, Error=()>>);

impl Future for ResponseFuture {
    type Item = HttpResponse<Body>;
    type Error = ();
    fn poll(&mut self) -> Result<Async<Self::Item>, Self::Error> { self.0.poll() }
}

impl From<ResponseBuilderFuture> for ResponseFuture {
    fn from(rb: ResponseBuilderFuture) -> Self {
        ResponseFuture(Box::new(rb.and_then(|b| futures::future::result(b.build().map_err(|e| error!("unable to build response from builder: {}", e))))))
    }
}

#[doc(hidden)]
pub trait AsyncOptionResponder {
    #[doc(hidden)]
    fn move_respond(&mut self, request: Request) -> ResponseBuilderFuture;
    #[doc(hidden)]
    fn move_respond_with_builder(&mut self, request: Request, builder: ResponseBuilder) -> ResponseBuilderFuture;
}

impl<T: AsyncResponder> AsyncOptionResponder for Option<T> {
    fn move_respond(&mut self, request: Request) -> ResponseBuilderFuture {
        self.take().expect("Cannot use responder twice").respond(request)
    }

    fn move_respond_with_builder(&mut self, request: Request, builder: ResponseBuilder) -> ResponseBuilderFuture {
        self.take().expect("Cannot use responder twice").respond_with_builder(request, builder)
    }
}

///
pub trait AsyncResponder {
    ///
    fn respond(self, request: Request) -> ResponseBuilderFuture;
    ///
    fn respond_with_builder(self, request: Request, builder: ResponseBuilder) -> ResponseBuilderFuture;
}

impl<T: 'static + Send + Sync + Responder> AsyncResponder for T {
    fn respond(self, request: Request) -> ResponseBuilderFuture {
        ResponseBuilderFuture(Box::new(futures::finished(self.respond(request))))
    }

    fn respond_with_builder(self, request: Request, builder: ResponseBuilder) -> ResponseBuilderFuture {
        ResponseBuilderFuture(Box::new(futures::finished(self.respond_with_builder(request, builder))))
    }
}

///
pub trait Responder {
    ///
    fn respond(self, request: Request) -> ResponseBuilder;
    ///
    fn respond_with_builder(self, request: Request, builder: ResponseBuilder) -> ResponseBuilder;
}

macro_rules! int_status_responder {
    ( $( $typ:ty ),+ ) => {
        $(
        impl Responder for $typ {
            fn respond(self, _: Request) -> ResponseBuilder {
                let mut b = ResponseBuilder::new();
                b.status(self as u16);
                b
            }

            fn respond_with_builder(self, _: Request, mut builder: ResponseBuilder) -> ResponseBuilder {
                builder.status(self as u16);
                builder
            }
        }
        )+
    }
}


/// STATUS CODE RESPONDER IMPLEMENTATION
int_status_responder!(u16,i16,u32,i32,u64,i64,usize,isize);

impl Responder for StatusCode {
    fn respond(self, _: Request) -> ResponseBuilder {
        let mut b = ResponseBuilder::new();
        b.status(self);
        b
    }

    fn respond_with_builder(self, _: Request, mut builder: ResponseBuilder) -> ResponseBuilder {
        builder.status(self);
        builder
    }
}

/// BODY RSPONDER IMPLEMENTATION
impl Responder for String {
    fn respond(self, _: Request) -> ResponseBuilder {
        let mut b = ResponseBuilder::new();
        b.body(self);
        b
    }

    fn respond_with_builder(self, _: Request, mut builder: ResponseBuilder) -> ResponseBuilder {
        builder.body(self);
        builder
    }
}