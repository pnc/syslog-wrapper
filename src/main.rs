use std::process::Command;
use std::process::Stdio;
use std::io::BufReader;
use std::io::BufRead;
use std::io::Write;
use std::thread;
use std::sync::mpsc::channel;

extern crate rustls;

#[derive(Debug)]
enum DeliverValue {
    Line(String),
    Eof(),
}

fn main() {
    let echo_hello = Command::new("./test.sh")
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
        .spawn().unwrap();
    let mut stdout_reader = BufReader::new(echo_hello.stdout.unwrap());
    let mut stderr_reader = BufReader::new(echo_hello.stderr.unwrap());

    let (sender, receiver) = channel();

    let stdout_sender = sender.clone();
    let stdout_handler = thread::spawn(move || {

        loop {
            let mut line = String::new();
            let len = stdout_reader.read_line(&mut line).expect("please be cool");
            if len == 0 {
                break
            }
            println!("stdout line is {len} bytes long");
            stdout_sender.send(DeliverValue::Line(line)).expect("receiver hung up :(");
        }
    });

    let stderr_sender = sender.clone();
    let stderr_handler = thread::spawn(move || {

        loop {
            let mut line = String::new();
            let len = stderr_reader.read_line(&mut line).expect("please be cool");
            if len == 0 {
                break
            }
            println!("stderr line is {len} bytes long");
            stderr_sender.send(DeliverValue::Line(line)).expect("receiver hung up :(");
        }
    });


    let delivery = thread::spawn(move || {
        let mut socket = std::net::TcpStream::connect("localhost:8443").unwrap();

        let mut root_store = rustls::RootCertStore::empty();
        root_store.add_server_trust_anchors(
            webpki_roots::TLS_SERVER_ROOTS
                .0
                .iter()
                .map(|ta| {
                    rustls::OwnedTrustAnchor::from_subject_spki_name_constraints(
                        ta.subject,
                        ta.spki,
                        ta.name_constraints,
                    )
                })
        );

        let config = rustls::ClientConfig::builder()
            .with_safe_defaults()
            .with_root_certificates(root_store)
            .with_no_client_auth();

        let arc = std::sync::Arc::new(config);
        //let dns_name = webpki::DnsNameRef::try_from_ascii_str("localhost").unwrap();
        let example_com = "localhost".try_into().unwrap();
        let mut client = rustls::ClientConnection::new(arc, example_com).unwrap();
        let mut stream = rustls::Stream::new(&mut client, &mut socket); // Create stream
                                                                        // Instead of writing to the client, you write to the stream

        loop {
            let result1 = receiver.recv().unwrap();
            match result1 {
                DeliverValue::Eof() => break,
                DeliverValue::Line(str) => stream.write(str.as_bytes()).unwrap()
            };
        }
    });

    stderr_handler.join().unwrap();
    stdout_handler.join().unwrap();
    sender.send(DeliverValue::Eof()).expect("oh no");
    delivery.join().unwrap();
}
