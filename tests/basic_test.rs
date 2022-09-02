extern crate assert_cli;
use assert_cli::Assert;

use std::net::TcpListener;
use std::process::{Command, Stdio, Child};


#[test]
fn it_suggests_help() {
    Assert::main_binary()
      .fails_with(2).and()
      .stderr().contains("try --help").unwrap();
}

#[test]
fn it_fails_to_locate_binary() {
  Assert::main_binary()
      .with_args(&["localhost", "--", "./notreal.sh"])
      .fails_with(40).and()
      .stderr().contains("No such file or directory").unwrap();
}

#[test]
fn it_fails_to_connect() {
  Assert::main_binary()
    .with_args(&["localhost", "--", "ls"])
      .fails_with(127).and()
      .stderr().contains("Unable to connect").unwrap();
}

#[test]
fn it_preserves_exit_code() {
  let (mut server, port_flag) = spawn_test_server();

  Assert::main_binary()
    .with_args(&[&*port_flag, "--", "sh", "-c", "exit 69"])
      .fails_with(69).and().unwrap();

  server.kill().unwrap();
}

#[test]
fn it_connects_and_sends_several_lines() {
  let (mut server, port_flag) = spawn_test_server();

  Assert::main_binary()
    .with_args(&[&*port_flag, "--", "seq", "1", "5"]).unwrap();

  server.kill().unwrap();
  let output = server.wait_with_output().expect("Not able to capture test server output.");
  let output_string = String::from_utf8(output.stdout).unwrap();
  let output_lines: Vec<&str> = output_string.lines().filter_map(|line| {
    if line.starts_with("<") {
      return Some(line);
    } else {
      return Option::None;
    }
  }).collect();
  assert_eq!(vec!["this is a special string"], output_lines);
}

fn spawn_test_server() -> (Child, String) {
  let listener = TcpListener::bind("localhost:0").expect("Unable to pick a port.");
  let port = listener.local_addr().expect("No local address.").port();
  // TODO: Automatically run minica
  let server_command = Command::new("./test-server.sh")
        .arg(format!("{port}"))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Unable to spawn test-server.sh during test.");
        return (server_command, format!("localhost:{port}"));
}
