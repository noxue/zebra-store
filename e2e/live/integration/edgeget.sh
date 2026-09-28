#!/bin/sh
# usage: edgeget.sh <host> <path>   (read-only GET to the store backend with a given Host)
ssh root@107.174.142.210 "docker exec zebra-lab-edge-1 wget -S -qO- --header 'Host: $1' 'http://store:8081$2' 2>&1"
