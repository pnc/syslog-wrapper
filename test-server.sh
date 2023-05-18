#!/bin/bash

if [ ! -f "localhost.crt" ]; then
  if ! (minica localhost > /dev/null 2>&1) ; then
    set -e
    minica -ca-cert cacert.crt -ca-key cacert.key -domains localhost
    mv localhost/key.pem localhost.key
    mv localhost/cert.pem localhost.crt
    rmdir localhost
  fi
  set -e
fi

exec openssl s_server -cert localhost.crt -key localhost.key -accept $1
