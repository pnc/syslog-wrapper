use std::borrow::Borrow;
use std::ffi::OsString;
use std::fs::File;
use std::io::BufRead;
use std::io::BufReader;
use std::io::Write;
use std::process::exit;
use std::process::Command;
use std::process::Stdio;
use std::sync::mpsc::sync_channel;
use std::thread;

use clap::Parser;
use rustls::Certificate;

// Consider: https://docs.rs/binary-layout/latest/binary_layout/

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
3. Preserve exit code
4. Tarpit destination (accept connection but then block writes)
5. Total throughput benchmark
6. Invalid cert test
7. Connection interrupted, reconnect without loss
*/

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    // TODO: Allow URI syntax
    // TODO: Allow pulling this from environment
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
        None => (args.server, 6514),
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

    let (sender, receiver) = sync_channel(200);

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
            .try_send(DeliverValue::Line(line))
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
            .try_send(DeliverValue::Line(line))
            .expect("receiver hung up :(");
    });

    let delivery = thread::spawn(move || {
        let mut socket = std::net::TcpStream::connect((host.clone(), port)).unwrap_or_else(|e| {
            eprintln!("Unable to connect to `{host}:{port}`: {e}");
            exit(127);
        });

        let mut root_store = rustls::RootCertStore::empty();
        // TODO: Put this behind a special --add-trusted-certificates flag

        // let cert_file = File::open("server.pem").expect("Could not open server.pem");
        // let mut cert_file_reader = std::io::BufReader::new(cert_file);
        // let custom_cert = match rustls_pemfile::read_one(&mut cert_file_reader) {
        //     Ok(Some(rustls_pemfile::Item::X509Certificate(cert_data))) => cert_data,
        //     _ => panic!("could not parse"),
        // };

        // root_store
        //     .add(&Certificate(custom_cert))
        //     .expect("could not add trust");

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
        //let dns_name = webpki::DnsNameRef::try_from_ascii_str("localhost").unwrap();
        let example_com = host.as_str().try_into().unwrap();
        let mut client = rustls::ClientConnection::new(arc, example_com).unwrap();
        let mut stream = rustls::Stream::new(&mut client, &mut socket); // Create stream
                                                                        // Instead of writing to the client, you write to the stream

        let hostname = args.hostname.expect("The command line parser failed.");
        let appname = args.appname.expect("The command line parser failed.");
        loop {
            let result1 = receiver.recv().unwrap();
            match result1 {
                DeliverValue::Eof() => break,
                DeliverValue::Line(str) => {
                    // TODO: Enforce newline?
                    // TODO: What if appname contains space?
                    let formatted = format!("<165>1 2003-08-24T05:14:15.000003-07:00 {hostname} {appname} 8710 - - {str}");
                    stream.write(formatted.as_bytes()).unwrap();
                },
            };
        }
    });

    stderr_handler.join().unwrap();
    stdout_handler.join().unwrap();
    sender.send(DeliverValue::Eof()).expect("oh no");
    delivery.join().unwrap();
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
