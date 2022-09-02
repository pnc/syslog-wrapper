use std::ffi::OsString;
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio, exit};
use std::sync::mpsc::channel; // Multiple producer, single consumer channel
use std::thread;

use clap::Parser; // Command line parsing
use rustls::Certificate; // TLS and certificate parsing
use chrono::Utc; // Formatting UTC time for syslog protocol

const SYSLOG_PRIORITY: &str = "22"; // See RFC 5424 sec. 6.2.1
const SYSLOG_VERSION: &str = "1"; // See RFC 5424 sec. 6.2.2
const DEFAULT_SYSLOG_PORT: u16 = 6514;

// https://docs.rs/clap/latest/clap/_derive/_cookbook/escaped_positional/index.html
// https://docs.rs/retry/latest/retry/
// https://www.rfc-editor.org/rfc/rfc3164#section-4.1
// https://www.rfc-editor.org/rfc/rfc5425#section-4.3
// https://www.rfc-editor.org/rfc/rfc5424#section-6
// https://docs.rs/assert_cli/latest/assert_cli/
// Do we actually need `cargo` feature?
/*
Test suite items:
1. Overly long line
2. Produce only stderr and exit, no stdout
4. Tarpit destination (accept connection but then block writes)
5. Total throughput benchmark
6. Invalid cert test
7. Connection interrupted, reconnect without loss
*/

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    // TODO: Allow URI syntax
    /// Hostname (and optional :port, defaults to 6514) of the remote TCP syslog receiver.
    #[clap(value_parser, env = "SYSLOG_SERVER")]
    server: String,

    /// The hostname to report on the syslog messages. Defaults to the actual system hostname.
    #[clap(value_parser, long, env = "SYSLOG_HOSTNAME")]
    hostname: Option<String>,

    /// The app-name/program name to report on the syslog messages. Defaults to `command`, excluding any arguments.
    #[clap(value_parser, long, env = "SYSLOG_APPNAME")]
    appname: Option<String>,

    /// Maximum number of times to retry consecutively before crashing
    #[clap(short, long, value_parser, default_value_t = 10)]
    max_retries: u8,

    /// Path to a file containing a PEM-encoded X509 certificate which will be added to the default trust store.
    #[clap(short, long, value_parser)]
    add_trusted_certificates: Option<PathBuf>,

    /// The actual command to run, and the standard output and standard error
    /// of which will be captured.
    #[clap(last = true, value_parser, required = true)]
    command: Vec<OsString>,
}

#[derive(Debug)]
enum DeliverValue {
    Line(String),
    Eof(),
}

fn main() {
    let mut args = Args::parse();

    // TODO: Drop into builder mode so these don't have to be ugly Optionals.
    // See https://docs.rs/clap/latest/clap/_derive/index.html#mixing-builder-and-derive-apis
    if args.hostname.is_none() {
        args.hostname = Some(gethostname::gethostname().to_string_lossy().to_string());
    }

    if args.appname.is_none() {
        args.appname = Some(args.command[0].to_string_lossy().to_string());
    }

    let (host, port): (String, u16) = match args.server.split_once(":") {
        Some((host, port_str)) => (host.into(), port_str.parse().unwrap()),
        None => (args.server, DEFAULT_SYSLOG_PORT),
    };

    let command_name = args.command[0].clone();
    let spawn_result = Command::new(command_name.clone())
        .args(&args.command[1..])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();

    let mut child_process = match spawn_result {
        Ok(child) => child,
        Err(error) => {
            eprintln!("An error occurred launching {command_name:?}: {error}");
            exit(40);
        },
    };

    let mut stdout_reader = BufReader::new(child_process.stdout.take().unwrap());
    let mut stderr_reader = BufReader::new(child_process.stderr.take().unwrap());

    // TODO: Consider using sync_channel here with a bound, if we want to apply backpressure to the subprocess.
    let (sender, receiver) = channel();

    let stdout_sender = sender.clone();
    let stdout_handler = thread::spawn(move || loop {
        let mut line = String::new();
        let len = stdout_reader.read_line(&mut line).expect("error reading next line from subcommand's stdout");
        if len == 0 {
            break;
        }
        // TODO: Possibly have a pass-through/tee mode that also echoes?
        // println!("stdout line is {len} bytes long");
        stdout_sender
            .send(DeliverValue::Line(line))
            .expect("receiver hung up :(");
    });

    let stderr_sender = sender.clone();
    let stderr_handler = thread::spawn(move || loop {
        let mut line = String::new();
        let len = stderr_reader.read_line(&mut line).expect("error reading next line from subcommand's stderr");
        if len == 0 {
            break;
        }
        // TODO: Possibly have a pass-through/tee mode that also echoes?
        // eprintln!("stderr line is {len} bytes long");
        stderr_sender
            .send(DeliverValue::Line(line))
            .expect("receiver hung up :(");
    });

    let delivery = thread::spawn(move || {
        let mut socket = std::net::TcpStream::connect((host.clone(), port)).unwrap_or_else(|e| {
            eprintln!("Unable to connect to `{host}:{port}`: {e}");
            exit(127);
        });

        let mut root_store = rustls::RootCertStore::empty();

        if let Some(trusted_certificates_file) = args.add_trusted_certificates {
            let cert_file = File::open(trusted_certificates_file.clone())
                .unwrap_or_else(|e| 
                    panic!("Could not open trusted certificate file `{trusted_certificates_file:?}`: {e}.")
                );
            let mut cert_file_reader = std::io::BufReader::new(cert_file);
            // TODO: Would be easy to allow multiple certificates here.
            let custom_cert = match rustls_pemfile::read_one(&mut cert_file_reader) {
                Ok(Some(rustls_pemfile::Item::X509Certificate(cert_data))) => cert_data,
                Ok(_) => panic!("The trusted certificate file did not contain a parseable certificate."),
                Err(e) => panic!("Could not parse trusted certificate: {e}"),
            };

            root_store
                .add(&Certificate(custom_cert))
                .expect("Could not add trusted certificate.");
        }

        root_store.add_server_trust_anchors(webpki_roots::TLS_SERVER_ROOTS.0.iter().map(|ta| {
            rustls::OwnedTrustAnchor::from_subject_spki_name_constraints(
                ta.subject,
                ta.spki,
                ta.name_constraints,
            )
        }));

        let config = rustls::ClientConfig::builder()
            .with_safe_defaults()
            .with_root_certificates(root_store)
            .with_no_client_auth();

        let arc = std::sync::Arc::new(config);
        let server_name = host.as_str().try_into().unwrap();
        let mut client = rustls::ClientConnection::new(arc, server_name).unwrap();
        let mut stream = rustls::Stream::new(&mut client, &mut socket);

        let hostname = args.hostname.expect("The command line parser failed.");
        let appname = args.appname.expect("The command line parser failed.");
        loop {
            let result = receiver.recv().unwrap();
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
                    stream.write(formatted.as_bytes()).unwrap();
                },
            };
        }
    });

    // Wait for the threads to finish consuming the child process's output
    stderr_handler.join().unwrap();
    stdout_handler.join().unwrap();
    sender.send(DeliverValue::Eof()).expect("Unable to send EOF to consuming threads.");
    // Wait for delivery of remaining messages to flush
    delivery.join().unwrap();
    // Wait for the child to exit
    match child_process.wait() {
        Ok(status) => match status.code() {
            // Preserve the exit code of the child
            Some(status) => exit(status),
            None => {
                eprintln!("The subcommand did not return an exit code.");
                exit(40);
            }
        },
        Err(error) => {
            eprintln!("An error occurred running the subcommand: {error}");
            exit(40);
        }
    }
}
