//! Requests and responses as plain values, independent of any HTTP runtime.

/// The methods Univ Server answers; any other is kept by name and refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Method {
    Get,
    Head,
    Post,
    Patch,
    Delete,
    Options,
    Other(String),
}

impl Method {
    /// The method named by an HTTP request line, matched without regard to case.
    #[must_use]
    pub fn parse(name: &str) -> Self {
        match name.to_ascii_uppercase().as_str() {
            "GET" => Self::Get,
            "HEAD" => Self::Head,
            "POST" => Self::Post,
            "PATCH" => Self::Patch,
            "DELETE" => Self::Delete,
            "OPTIONS" => Self::Options,
            _ => Self::Other(name.to_owned()),
        }
    }

    /// The method's name as HTTP writes it.
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Get => "GET",
            Self::Head => "HEAD",
            Self::Post => "POST",
            Self::Patch => "PATCH",
            Self::Delete => "DELETE",
            Self::Options => "OPTIONS",
            Self::Other(name) => name,
        }
    }
}

/// One request: its method, its path without the query, its raw query, its headers with
/// lower-case names, and its body.
#[derive(Clone, Debug)]
pub struct Request {
    pub method: Method,
    pub path: String,
    pub query: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Request {
    /// A request with no query, headers or body.
    #[must_use]
    pub fn new(method: Method, path: &str) -> Self {
        Self {
            method,
            path: path.to_owned(),
            query: String::new(),
            headers: Vec::new(),
            body: Vec::new(),
        }
    }

    /// The value of the first header of that name, compared without regard to case.
    #[must_use]
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

/// What a response carries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Body {
    Empty,
    Bytes(Vec<u8>),
}

/// One response: its status, its headers and its body.
#[derive(Clone, Debug)]
pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Body,
}

impl Response {
    /// A response of that status with no headers and no body.
    #[must_use]
    pub fn empty(status: u16) -> Self {
        Self {
            status,
            headers: Vec::new(),
            body: Body::Empty,
        }
    }

    /// A 200 response holding `value` as JSON.
    #[must_use]
    pub fn json(value: &serde_json::Value) -> Self {
        Self::bytes(200, "application/json", value.to_string().into_bytes())
    }

    /// A response of that status and content type holding `body`.
    #[must_use]
    pub fn bytes(status: u16, content_type: &str, body: Vec<u8>) -> Self {
        Self::empty(status)
            .with_header("content-type", content_type)
            .with_body(Body::Bytes(body))
    }

    /// The response with one more header.
    #[must_use]
    pub fn with_header(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.to_owned(), value.to_owned()));
        self
    }

    /// The response with that body.
    #[must_use]
    pub fn with_body(mut self, body: Body) -> Self {
        self.body = body;
        self
    }

    /// The value of the first header of that name, compared without regard to case.
    #[must_use]
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}
