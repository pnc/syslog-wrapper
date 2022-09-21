use std::{thread, fmt};
use chrono::Utc;
use std::io::{Write};
use retry::{retry, delay::Exponential};

use crate::DeliverValue; // Formatting UTC time for syslog protocol

const SYSLOG_PRIORITY: &str = "22"; // See RFC 5424 sec. 6.2.1
const SYSLOG_VERSION: &str = "1"; // See RFC 5424 sec. 6.2.2

pub(crate) struct Sender {
  root_store: rustls::RootCertStore,
  host: String, port: u16,
  hostname: String,
  appname: String
}

#[derive(Debug, Clone)]
pub enum Error {
  ConnectionError(String),
  PipeError(std::sync::mpsc::RecvError),
  SecurityError(rustls::Error)
}

impl fmt::Display for Error {
  fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
      write!(f, "sender error: {}", self)
  }
}

impl From<std::io::Error> for Error {
    fn from(io_error: std::io::Error) -> Self {
        return Error::ConnectionError(io_error.to_string());
    }
}

impl From<std::sync::mpsc::RecvError> for Error {
  fn from(error: std::sync::mpsc::RecvError) -> Self {
      return Error::PipeError(error);
  }
}

impl From<rustls::Error> for Error {
  fn from(error: rustls::Error) -> Self {
      return Error::SecurityError(error);
  }
}

impl Sender {
  pub fn new(root_store: rustls::RootCertStore,
             host: String, port: u16,
             hostname: String,
             appname: String) -> Self {
    let new = Self {
      root_store: root_store,
      host: host,
      port: port,
      hostname: hostname,
      appname: appname
    };
    return new;
  }

  pub fn start(&self, receiver: std::sync::mpsc::Receiver<DeliverValue>) -> thread::JoinHandle<Result<(), retry::Error<Error>>> {
    let config = rustls::ClientConfig::builder()
          .with_safe_defaults()
          .with_root_certificates(self.root_store.clone())
          .with_no_client_auth();

    let host = self.host.clone();
    let port = self.port;
    let hostname = self.hostname.clone();
    let appname = self.appname.clone();
    let server_name: rustls::ServerName = host.as_str().try_into().unwrap();
//    let root_store = self.root_store;

    return thread::spawn(move || {
      // TODO: Allow max retries to be configured via command line
      return retry(Exponential::from_millis(10).take(3), move || {
        let mut socket = std::net::TcpStream::connect((host.clone(), port))?;

        let arc = std::sync::Arc::new(config.clone());

        let mut client = rustls::ClientConnection::new(arc, server_name.clone())?;
        let mut stream = rustls::Stream::new(&mut client, &mut socket);
        loop {
          // TODO: Flip this so we don't consume a value per retry
            let result = receiver.recv()?;
            match result {
                DeliverValue::Eof() => break,
                DeliverValue::Line(str) => {
                    // TODO: Enforce newline?
                    // TODO: What if appname contains space?
                    // TODO: Produce timestamp on sending thread in case this one is behind during a retry?
                    // Timestamp format per https://www.rfc-editor.org/rfc/rfc5424#section-6
                    // E.g: 2003-08-24T05:14:15.000003-07:00
                    let timestamp = Utc::now().format("%Y-%m-%dT%H:%M:%S%.6f%:z");
                    let formatted = format!("<{SYSLOG_PRIORITY}>{SYSLOG_VERSION} {timestamp} {hostname} {appname} - - - {str}");
                    stream.write(formatted.as_bytes())?;
                },
            };
        }
        return Ok(());
      });
    });
  }
}
