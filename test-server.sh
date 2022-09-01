#!/bin/bash

set -e

exec openssl s_server -accept 8443
