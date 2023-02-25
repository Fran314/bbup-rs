#!/bin/bash

idle="read -r -d '' _ </dev/tty"

server_active=$(ps -ef | grep [b]bup-server)
if [[ $server_active != "" ]]; then
    echo "Found a bbup-server instance active. No bbup-server instance should be active for this test to run correctly" 1>&2
    exit 1
fi

playground="$(pwd)/playground"
server_conf_dir="$playground/.config/bbup-server"
client_conf_dir="$playground/.config/bbup"

archive="$playground/archive"
bs1="$playground/backup-source-1"
bs2="$playground/backup-source-2"

# Ensure empty playground folder
echo "Creating environment"
[ -d $playground ] && rm -r $playground
mkdir $playground

echo "Instantiating bbup-server and archive"
mkdir $archive
bbup-server -c "$server_conf_dir" setup -s 4000 -a "$archive" 1> /dev/null
bbup-server -c "$server_conf_dir" create -e "prova" 1> /dev/null

echo "Instantiating bbup (client) and backup source"
mkdir $bs1
mkdir $bs2
bbup -c "$client_conf_dir" setup -l 3000 -s 4000 -n baldo -a localhost 1> /dev/null
bbup -c "$client_conf_dir" -l "$bs1" init -n -e "prova" 1> /dev/null
bbup -c "$client_conf_dir" -l "$bs2" init -n -e "prova" 1> /dev/null

echo "content" > $bs1/first-added-file
echo "content" > $bs1/second-added-file

./sync.sh $server_conf_dir $client_conf_dir $bs1
./sync.sh $server_conf_dir $client_conf_dir $bs2

if $(diff --no-dereference $bs1 $bs2 1> /dev/null)
then
    echo "same content in 1 and 2"
    tree -phDa $archive
    tree -phDa $bs1
else
    echo "different content in 1 and 2"
fi

# Destroy playground
echo "Cleaning environment"
rm -r $playground
echo "Done!"
