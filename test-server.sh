#!/bin/bash

set -e

[ ! -f "localhost.crt" ] && minica localhost

exec openssl s_server -cert localhost.crt -key localhost.key -accept $1
