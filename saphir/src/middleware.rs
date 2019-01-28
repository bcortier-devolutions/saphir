use regex::Regex;
use std::sync::Arc;

use crate::utils::ToRegex;
use tokio::prelude::*;
use crate::request::Request;
use crate::response::AsyncOptionResponder;
use crate::response::AsyncResponder;

struct MiddlewareRule {
    included_path: Vec<Regex>,
    excluded_path: Option<Vec<Regex>>,
}

impl MiddlewareRule {
    pub fn new<R: ToRegex>(include_path: Vec<R>, exclude_path: Option<Vec<R>>) -> Self {
        let mut included_path = Vec::new();
        for include in include_path.iter() {
            included_path.push(reg!(include));
        }

        let mut excluded_path: Option<Vec<Regex>> = Option::None;

        if let Some(excludes) = exclude_path {
            let mut excludes_vec = Vec::new();
            for exclude in excludes.iter() {
                excludes_vec.push(reg!(exclude));
            }

            excluded_path = Some(excludes_vec);
        }

        MiddlewareRule {
            included_path,
            excluded_path,
        }
    }

    pub fn validate_path(&self, path: &str) -> bool {
        let path_clone = path.clone();
        if self.included_path.iter().enumerate().find(
            move |&(_index, r)| {
                r.is_match(path_clone)
            }
        ).is_some() {
            if let Some(ref excluded_path) = self.excluded_path {
                return excluded_path.iter().enumerate().find(
                    move |&(_index, re)| {
                        re.is_match(path_clone)
                    }
                ).is_none();
            } else {
                return true;
            }
        }

        false
    }
}

///
#[derive(Clone)]
pub struct MiddlewareStack {
    middlewares: Arc<Vec<(MiddlewareRule, Box<Resolver>)>>,
}

impl MiddlewareStack {
    ///
    pub fn new() -> Self {
        MiddlewareStack {
            middlewares: Arc::new(Vec::new())
        }
    }

    ///
    pub fn resolve(&self, request: Request) -> impl Future<Item=Continuation, Error=()> {
        ResolvedStackFuture {
            request: Some(request),
            middlewares: self.middlewares.clone(),
            current: None,
            next: 0
        }
    }
}

struct ResolvedStackFuture {
    request: Option<Request>,
    middlewares: Arc<Vec<(MiddlewareRule, Box<Resolver>)>>,
    current: Option<ContinuationFuture>,
    next: usize,
}

impl Future for ResolvedStackFuture {
    type Item = Continuation;
    type Error = ();

    fn poll(&mut self) -> Result<Async<Self::Item>, Self::Error> {
        if let Some(fut) = self.current.as_mut().take() {
            match fut.poll()? {
                Async::Ready(Continuation::Next(request)) => {
                    self.request = Some(request);
                }
                Async::Ready(Continuation::Stop(req, responder)) => {
                    return Ok(Async::Ready(Continuation::Stop(req, responder)));
                }
                _ => {
                    task::current().notify();
                    return Ok(Async::NotReady)
                }
            }
        }

        loop {
            if self.next >= self.middlewares.len() {
                return Ok(Async::Ready(Continuation::Next(self.request.take().expect("A MiddlewaresResolverFuture without request should not exist, this is fatal"))));
            }

            let next = &self.middlewares[self.next];

            {
                self.next += 1;
            }

            if next.0.validate_path(self.request.as_ref().expect("A MiddlewaresResolverFuture without request should not exist, this is fatal").uri().path()) {
                self.current = Some(next.1.resolve(self.request.take().expect("A MiddlewaresResolverFuture without request should not exist, this is fatal")));
                task::current().notify();
                return Ok(Async::NotReady);
            }
        }
    }
}

///
pub struct Builder {
    stack: Vec<(MiddlewareRule, Box<Resolver>)>,
}

impl Builder {
    /// Creates a new MiddlewareStack Builder
    pub fn new() -> Self {
        Builder {
            stack: Vec::new()
        }
    }

    /// Method to apply a new middleware onto the stack where the `include_path` vec are all path affected by the middleware,
    /// and `exclude_path` are exclusion amongst the included paths.
    pub fn apply<M: 'static + Resolver>(mut self, m: M, include_path: Vec<&str>, exclude_path: Option<Vec<&str>>) -> Self {
        let rule = MiddlewareRule::new(include_path, exclude_path);
        let boxed_m = Box::new(m);

        self.stack.push((rule, boxed_m));

        self
    }

    /// Build the middleware stack
    pub fn build(self) -> MiddlewareStack {
        let Builder {
            stack,
        } = self;

        MiddlewareStack {
            middlewares: Arc::new(stack),
        }
    }
}

///
pub enum Continuation {
    ///
    Stop(Request, Box<AsyncOptionResponder + Send + Sync>),
    ///
    Next(Request),
}

///
pub fn stop<R: 'static + AsyncResponder + Send + Sync>(request: Request, responder: R) -> Continuation {
    Continuation::Stop(request, Box::new(Some(responder)))
}

///
pub fn next(request: Request) -> Continuation {
    Continuation::Next(request)
}

///
pub struct ContinuationFuture {
    ///
    inner: Box<Future<Item=Continuation, Error=()> + Send>
}

impl Future for ContinuationFuture {
    type Item = Continuation;
    type Error = ();

    fn poll(&mut self) -> Result<Async<Self::Item>, Self::Error> {
        self.inner.poll()
    }
}

///
pub trait Resolver: Send + Sync {
    ///
    fn resolve(&self, request: Request) -> ContinuationFuture;
}

impl<F, U> Resolver for F where F: Send + Sync + Fn(Request) -> U, U: 'static + Send + Future<Item=Continuation, Error=()> {
    fn resolve(&self, request: Request) -> ContinuationFuture {
        ContinuationFuture {
            inner: Box::new((*self)(request))
        }
    }
}