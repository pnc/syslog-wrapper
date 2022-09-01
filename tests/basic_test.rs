#[macro_use]
extern crate assert_cli;
use assert_cli::Assert;

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
    .with_args(&["--", "./notreal.sh"])
      .fails_with(40).and()
      .stderr().contains("No such file or directory").unwrap();
}

#[test]
fn it_fails_to_connect() {
  Assert::main_binary()
    .with_args(&["--", "ls"])
      .fails_with(20).and()
      .stderr().contains("unable to do whatever").unwrap();
}

#[test]
fn it_preserves_exit_code() {
  let mut server_command = spawn_test_server();

  Assert::main_binary()
    .with_args(&["--", "sh", "-c", "exit 69"])
      .fails_with(69).and().unwrap();

  server_command.kill().unwrap();
}

#[test]
fn it_connects_and_sends_several_lines() {
  let mut server_command = spawn_test_server();

  Assert::main_binary()
    .with_args(&["--", "echo", "this is a special string"]).unwrap();

  server_command.kill().unwrap();
  let output = server_command.wait_with_output().expect("Not able to capture test server output.");
  assert_eq!("this is a special string", String::from_utf8(output.stdout).unwrap());
}

fn spawn_test_server() -> Child {
  // TODO: Automatically run minica
  let server_command = Command::new("./test-server.sh")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Unable to spawn test-server.sh during test.");
        return server_command;
}
