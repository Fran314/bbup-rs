#!/bin/bash

idle="read -r -d '' _ </dev/tty"
server_conf_dir=$1
client_conf_dir=$2
bs=$3

echo "Starting bbup-server daemon"
tmux new -d -s "bbup-test" "echo '[Server panel]\n'; bbup-server -c $server_conf_dir run -pv"
echo "Starting bbup (client) sync"
tmux split-window -t "bbup-test" "echo '[Client panel]\n'; bbup -c $client_conf_dir -l $bs sync -pv; $idle"
tmux attach-session -t "bbup-test"
