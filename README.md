# syslog-wrapper

Boring name, boring tool:

1. Run a subprocess,
2. collect its standard output and standard error,
3. and forward those to an RFC 5425-compliant remote syslog host.

That means you can send a list of numbers directly to Papertrail with:

```bash
SYSLOG_HOSTNAME=pleasant-cornfield-5 cargo run logs2.papertrailapp.com:48001 -- seq 1 5
```

and they'll show up like this:

```
Sep 02 17:12:07 precarious-tuft-41 seq 1
Sep 02 17:12:07 precarious-tuft-41 seq 2
Sep 02 17:12:07 precarious-tuft-41 seq 3
Sep 02 17:12:07 precarious-tuft-41 seq 4
Sep 02 17:12:07 precarious-tuft-41 seq 5
```

In more practical usage, you might wrap a Docker process like so:

```bash
export SYSLOG_HOSTNAME=precarious-tuft-41
export SYSLOG_APPNAME=carriage-cobbler
export SYSLOG_SERVER=logs2.papertrailapp.com:48001
syslog-wrapper ./bin/cobble
```

## Recommended development environment

1. Install `rustup`
2. Add `rust-analyzer` extension to VS Code

## Run tests

`cargo test`

## Run for interactive testing

`SYSLOG_HOSTNAME=dingdong cargo run logs2.papertrailapp.com:48001 -- seq 1 5`
