//! The browser build's stand-in for `ureq` (renamed to `ureq` in the crates that use it, for
//! `wasm32` only). The page has no HTTP client for ureq: it reads std's clock, which panics
//! there, and needs sockets the browser doesn't give. Here every request fails with a transport
//! error before it leaves, as a request does with the network down, so what fetches from the
//! internet (sign-in, Realms, BlueMap tiles, the pack browser...) takes its offline path, and
//! ureq's TLS stack stays out of the page.
//!
//! Only the part of ureq 2's API the game calls is here; add more as the compiler asks.
#![forbid(unsafe_code)]

use std::{fmt, io, time::Duration};

/// Why every request fails in the browser.
const NO_CLIENT: &str = "the browser build has no HTTP client";

/// `ureq::AgentBuilder`: its settings are kept nowhere.
#[derive(Clone, Debug, Default)]
pub struct AgentBuilder;

impl AgentBuilder {
    pub fn new() -> Self {
        Self
    }
    pub fn timeout(self, _: Duration) -> Self {
        self
    }
    pub fn timeout_connect(self, _: Duration) -> Self {
        self
    }
    pub fn timeout_read(self, _: Duration) -> Self {
        self
    }
    pub fn timeout_write(self, _: Duration) -> Self {
        self
    }
    pub fn user_agent(self, _: &str) -> Self {
        self
    }
    pub fn redirects(self, _: u32) -> Self {
        self
    }
    pub fn build(self) -> Agent {
        Agent
    }
}

/// `ureq::Agent`.
#[derive(Clone, Debug, Default)]
pub struct Agent;

impl Agent {
    pub fn new() -> Self {
        Self
    }
    pub fn request(&self, _method: &str, _url: &str) -> Request {
        Request
    }
    pub fn get(&self, url: &str) -> Request {
        self.request("GET", url)
    }
    pub fn post(&self, url: &str) -> Request {
        self.request("POST", url)
    }
    pub fn put(&self, url: &str) -> Request {
        self.request("PUT", url)
    }
    pub fn delete(&self, url: &str) -> Request {
        self.request("DELETE", url)
    }
}

/// `ureq::get`.
pub fn get(url: &str) -> Request {
    Agent.get(url)
}
/// `ureq::post`.
pub fn post(url: &str) -> Request {
    Agent.post(url)
}

/// `ureq::Request`: never sent.
#[derive(Clone, Debug)]
pub struct Request;

impl Request {
    pub fn set(self, _header: &str, _value: &str) -> Self {
        self
    }
    pub fn query(self, _name: &str, _value: &str) -> Self {
        self
    }
    pub fn timeout(self, _: Duration) -> Self {
        self
    }
    pub fn call(self) -> Result<Response, Error> {
        Err(Error::Transport(Transport))
    }
    pub fn send(self, _body: impl io::Read) -> Result<Response, Error> {
        self.call()
    }
    pub fn send_bytes(self, _body: &[u8]) -> Result<Response, Error> {
        self.call()
    }
    pub fn send_string(self, _body: &str) -> Result<Response, Error> {
        self.call()
    }
}

/// `ureq::Response`: none ever comes back, so none can be made.
#[derive(Debug)]
pub enum Response {}

impl Response {
    pub fn status(&self) -> u16 {
        match *self {}
    }
    pub fn status_text(&self) -> &str {
        match *self {}
    }
    pub fn header(&self, _name: &str) -> Option<&str> {
        match *self {}
    }
    pub fn content_type(&self) -> &str {
        match *self {}
    }
    pub fn into_string(self) -> io::Result<String> {
        match self {}
    }
    pub fn into_reader(self) -> Box<dyn io::Read + Send + Sync + 'static> {
        match self {}
    }
}

/// `ureq::Error`: in the browser always [`Error::Transport`].
#[derive(Debug)]
pub enum Error {
    Status(u16, Response),
    Transport(Transport),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Status(code, _) => write!(f, "status code {code}"),
            Error::Transport(t) => t.fmt(f),
        }
    }
}
impl std::error::Error for Error {}
impl From<Error> for io::Error {
    fn from(e: Error) -> Self {
        io::Error::other(e.to_string())
    }
}

/// `ureq::Transport`: the request never left.
#[derive(Clone, Debug)]
pub struct Transport;

impl Transport {
    pub fn kind(&self) -> ErrorKind {
        ErrorKind::Io
    }
    pub fn message(&self) -> Option<&str> {
        Some(NO_CLIENT)
    }
}
impl fmt::Display for Transport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(NO_CLIENT)
    }
}

/// `ureq::ErrorKind`: the one a request without a client fails with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorKind {
    Io,
}
impl fmt::Display for ErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Network Error")
    }
}
