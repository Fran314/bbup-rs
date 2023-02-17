#!/bin/bash

idle="read -r -d '' _ </dev/tty"

server_active=$(ps -ef | grep [b]bup-server)
if [[ $server_active != "" ]]; then
    echo "Found a bbup-server instance active. No bbup-server instance should be active for this test to run correctly" 1>&2
    exit 1
fi

fake_home="$(pwd)/playground"
archive=$fake_home/archive
bs1="$fake_home/backup-source-1"
bs2="$fake_home/backup-source-2"

# Ensure empty playground folder
echo "Creating environment"
[ -d $fake_home ] && rm -r $fake_home
mkdir $fake_home

echo "Instantiating bbup-server and archive"
mkdir $archive
bbup-server -H $fake_home setup -s 4000 -a "archive" 1> /dev/null
bbup-server -H $fake_home create -e "prova" 1> /dev/null

echo "Instantiating bbup (client) and backup source"
mkdir $bs1
mkdir $bs2
bbup -H $fake_home setup -l 3000 -s 4000 -n baldo -a localhost 1> /dev/null
bbup -H $fake_home -C $bs1 init -n -e "prova" 1> /dev/null
bbup -H $fake_home -C $bs2 init -n -e "prova" 1> /dev/null

echo "content" > $bs1/first-added-file
echo "content" > $bs1/second-added-file

./sync.sh $fake_home $bs1
./sync.sh $fake_home $bs2

if $(diff --no-dereference $bs1 $bs2 1> /dev/null)
then
    echo "same content in 1 and 2"
else
    echo "different content in 1 and 2"
fi

tree -phDa $archive
tree -phDa $bs1

# Destroy playground
echo "Cleaning environment"
rm -r $fake_home
echo "Done!"
