use std::process::Command;
use std::process::Stdio;
use std::io::BufReader;
use std::io::BufRead;
use std::thread;
use std::sync::mpsc::channel;

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
        loop {
            let result1 = receiver.recv().unwrap();
            match result1 {
                DeliverValue::Eof() => break,
                DeliverValue::Line(str) => println!("deliver: {str}")
            }
        }
    });

    stderr_handler.join().unwrap();
    stdout_handler.join().unwrap();
    sender.send(DeliverValue::Eof()).expect("oh no");
    delivery.join().unwrap();
}
