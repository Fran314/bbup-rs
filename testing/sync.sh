#!/bin/bash

idle="read -r -d '' _ </dev/tty"
fake_home=$1
fake_source=$2

echo "Starting bbup-server daemon"
tmux new -d -s "bbup-test" "echo '[Server panel]\n'; bbup-server -H $fake_home run -pv"
echo "Starting bbup (client) sync"
tmux split-window -t "bbup-test" "echo '[Client panel]\n'; bbup -H $fake_home -C $fake_source sync -pv; $idle"
tmux attach-session -t "bbup-test"
