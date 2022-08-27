#!/bin/bash

echo 'start'
sleep 1
echo 'next'
sleep 1
echo 'one more'
sleep 0.5
>&2 echo "this one is on stderr"
sleep 0.5
echo 'done'
