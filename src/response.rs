use http::{StatusCode, HttpTryFrom, Version};
use http::response::Builder as HttpResponseBuilder;
use http::header::{HeaderName, HeaderValue, SET_COOKIE};
use httpdate::HttpDate;
use std::time::UNIX_EPOCH;
use hyper::Body;

///
pub type SyncResponse = Response;

///
pub struct CookieOptions {
    /// Domain name for the cookie. Defaults to the domain name of the app.
    pub domain: Option<String>,
    /// Expiry date of the cookie in GMT. If not specified or set to 0, creates a session cookie.
    pub expires: Option<HttpDate>,
    /// Flags the cookie to be accessible only by the web server.
    pub http_only: bool,
    /// Convenient option for setting the expiry time relative to the current time in milliseconds.
    pub max_age: Option<u64>,
    /// Path for the cookie. Defaults to “/”.
    pub path: Option<String>,
    /// Marks the cookie to be used with HTTPS only.
    pub secure: bool,
    /// Value of the “SameSite” Set-Cookie attribute. More information at https://tools.ietf.org/html/draft-ietf-httpbis-cookie-same-site-00#section-4.1.1.
    pub same_site: Option<String>,
}

impl CookieOptions {
    fn into_string(self) -> String {
        let mut base = String::new();
        let CookieOptions {
            domain,
            expires,
            http_only,
            max_age,
            path,
            secure,
            same_site, } = self;

        if let Some(domain) = domain.as_ref() {
            base.push_str(" Domain=");
            base.push_str(domain);
            base.push(';');
        }

        if let Some(expires) = expires.map(|e| e.to_string()) {
            base.push_str(" Expires=");
            base.push_str(&expires);
            base.push(';');
        }

        if http_only {
            base.push_str(" HttpOnly");
            base.push(';');
        }

        if let Some(max_age) = max_age {
            base.push_str(&format!(" Max-Age={};", max_age));
        }

        if let Some(path) = path.as_ref() {
            base.push_str(" Path=");
            base.push_str(path);
            base.push(';');
        }

        if secure {
            base.push_str(" Secure");
            base.push(';');
        }

        if let Some(same_site) = same_site.filter(|s| s.eq("Lax") || s.eq("Strict")).as_ref() {
            base.push_str(" SameSite=");
            base.push_str(same_site);
            base.push(';');
        }

        base
    }
}

///
pub struct Response {
    builder: HttpResponseBuilder,
    body: Option<Box<ToBody>>,
}

impl Response {
    ///
    pub fn new() -> Self {
        Response {
            builder: HttpResponseBuilder::new(),
            body: None
        }
    }

    ///
    pub fn status<T>(&mut self, status: T) -> &mut Response
        where StatusCode: HttpTryFrom<T>,
    {
        self.builder.status(status);
        self
    }

    ///
    pub fn version(&mut self, version: Version) -> &mut Response {
        self.builder.version(version);
        self
    }

    ///
    pub fn header<K, V>(&mut self, key: K, value: V) -> &mut Response
        where HeaderName: HttpTryFrom<K>,
              HeaderValue: HttpTryFrom<V>
    {
        self.builder.header(key, value);
        self
    }

    ///
    pub fn cookie(&mut self, name: &str, value: &str, options: Option<CookieOptions>) -> &mut Response {
        let mut base = format!("{}={};", name, value);

        if let Some(options) = options.map(|o| o.into_string()).as_ref() {
            base.push_str(options)
        }

        self.header(SET_COOKIE, base);
        self
    }

    ///
    pub fn clear_cookie(&mut self, name: &str, options: Option<CookieOptions>) -> &mut Response {
        let mut base = format!("{}=\"\";", name);

        if let Some(options) = options.map(|mut o| {
            o.max_age = Some(0);
            o.expires = Some(HttpDate::from(UNIX_EPOCH));
            o.into_string()
        }).as_ref() {
            base.push_str(options)
        }

        self.header(SET_COOKIE, base);
        self
    }

    ///
    pub fn body<B: 'static + ToBody>(&mut self, body: B) -> &mut Response {
        self.body = Some(Box::new(body));
        self
    }

    ///
    pub(crate) fn build_response(self) -> Result<hyper::Response<Body>, http::Error> {
        let Response { mut builder, body } = self;
        let b: Body = body.map(|b| b.to_body()).unwrap_or_else(|| Body::empty());
        builder.body(b)
    }
}

///
pub trait ToBody {
    ///
    fn to_body(&self) -> Body;
}

impl<I> ToBody for I where I: Into<Body> + Clone {
    fn to_body(&self) -> Body {
        self.clone().into()
    }
}