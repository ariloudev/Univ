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

/// One request: its method, its path without the query, its raw query, its headers, and its
/// body.
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
        find_header(&self.headers, name)
    }

    /// The decoded value of the first query parameter of that name, as a form encodes it:
    /// `+` is a space and `%XX` a byte of UTF-8. A value that does not decode is kept as sent.
    #[must_use]
    pub fn param(&self, name: &str) -> Option<String> {
        self.query
            .split('&')
            .filter_map(|pair| pair.split_once('=').or(Some((pair, ""))))
            .find(|(key, _)| decode(key) == name)
            .map(|(_, value)| decode(value))
    }
}

/// `text` with `+` as a space and each `%XX` as its byte, or `text` itself when the bytes are
/// not UTF-8. A `%` not followed by two hexadecimal digits is kept.
fn decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let escaped = match bytes.get(i..i + 3) {
            Some([b'%', high, low]) => hex(*high).zip(hex(*low)).map(|(h, l)| h * 16 + l),
            _ => None,
        };
        if let Some(byte) = escaped {
            out.push(byte);
            i += 3;
        } else {
            out.push(if bytes[i] == b'+' { b' ' } else { bytes[i] });
            i += 1;
        }
    }
    String::from_utf8(out).unwrap_or_else(|_| text.to_owned())
}

/// The value of one hexadecimal digit.
fn hex(digit: u8) -> Option<u8> {
    char::from(digit)
        .to_digit(16)
        .and_then(|value| u8::try_from(value).ok())
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
        find_header(&self.headers, name)
    }
}

/// The value of the first header of that name in `headers`, compared without regard to case.
fn find_header<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_query(query: &str) -> Request {
        let mut request = Request::new(Method::Get, "/v1/x");
        query.clone_into(&mut request.query);
        request
    }

    #[test]
    fn a_query_parameter_is_decoded_as_a_form_encodes_it() {
        let request = with_query("name=M%2031&kind=void+wall&release=6");
        assert_eq!(request.param("name").as_deref(), Some("M 31"));
        assert_eq!(request.param("kind").as_deref(), Some("void wall"));
        assert_eq!(request.param("release").as_deref(), Some("6"));
        assert_eq!(request.param("missing"), None);
    }

    #[test]
    fn a_malformed_escape_is_kept_as_sent() {
        let request = with_query("a=100%&b=%zz&c=%C3%A9&d");
        assert_eq!(request.param("a").as_deref(), Some("100%"));
        assert_eq!(request.param("b").as_deref(), Some("%zz"));
        assert_eq!(request.param("c").as_deref(), Some("é"));
        assert_eq!(request.param("d").as_deref(), Some(""));
    }
}
