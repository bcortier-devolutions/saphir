#![allow(dead_code)]
use http::header::HeaderValue;

/// Enum representing whether or not a request should continue to be processed be the server
pub enum RequestContinuation {
    /// Next
    Continue,
    /// None
    Stop,
}

/// Trait to convert string type to regular expressions
pub trait ToRegex {
    ///
    fn to_regex(&self) -> Result<::regex::Regex, ::regex::Error>;
    ///
    fn as_str(&self) -> &str;
}

impl<'a> ToRegex for &'a str {
    fn to_regex(&self) -> Result<::regex::Regex, ::regex::Error> {
        ::regex::Regex::new(self)
    }

    fn as_str(&self) -> &str {
        self
    }
}

impl ToRegex for String {
    fn to_regex(&self) -> Result<::regex::Regex, ::regex::Error> {
        ::regex::Regex::new(self.as_str())
    }

    fn as_str(&self) -> &str {
        &self
    }
}

impl ToRegex for ::regex::Regex {
    fn to_regex(&self) -> Result<::regex::Regex, ::regex::Error> {
        Ok(self.clone())
    }

    fn as_str(&self) -> &str {
        self.as_str()
    }
}

#[macro_export]
macro_rules! reg {
    ($str_regex:expr) => {
        $str_regex.to_regex().expect("the parameter passed to reg macro is not a legitimate regex")
    };
}

/// Trait to help formatting an using complex custom header
pub trait HeaderFormatter {
    /// Name of the Header (e.g. Accept-Encoding)
    const NAME: &'static str;

    ///
    fn from_value(h_val: &HeaderValue) -> Self;
    ///
    fn into_value(self) -> HeaderValue;
}