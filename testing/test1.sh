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

echo "content 1" > $bs1/untouched-file
ln -s "path/to/1" $bs1/untouched-symlink
mkdir $bs1/untouched-dir
echo "content 2" > $bs1/untouched-dir/some-file
ln -s "path/to/2" $bs1/untouched-dir/some-symlink
mkdir $bs1/untouched-dir/some-dir

echo "content 3" > $bs1/local-removed-file
ln -s "path/to/3" $bs1/local-removed-symlink
mkdir $bs1/local-removed-dir
echo "content 4" > $bs1/local-removed-dir/some-file
ln -s "path/to/4" $bs1/local-removed-dir/some-symlink
mkdir $bs1/local-removed-dir/some-dir

echo "content 5" > $bs1/miss-removed-file
ln -s "path/to/5" $bs1/miss-removed-symlink
mkdir $bs1/miss-removed-dir
echo "content 6" > $bs1/miss-removed-dir/some-file
ln -s "path/to/6" $bs1/miss-removed-dir/some-symlink
mkdir $bs1/miss-removed-dir/some-dir

echo "content 7" > $bs1/both-removed-file
ln -s "path/to/7" $bs1/both-removed-symlink
mkdir $bs1/both-removed-dir
echo "content 8" > $bs1/both-removed-dir/some-file
ln -s "path/to/8" $bs1/both-removed-dir/some-symlink
mkdir $bs1/both-removed-dir/some-dir

echo "content 9" > $bs1/local-edited-file
ln -s "path/to/9" $bs1/local-edited-symlink
mkdir $bs1/local-edited-dir
echo "content 10" > $bs1/local-edited-dir/old-file
ln -s "path/to/10" $bs1/local-edited-dir/old-symlink
mkdir $bs1/local-edited-dir/old-dir

echo "content 11" > $bs1/miss-edited-file
ln -s "path/to/11" $bs1/miss-edited-symlink
mkdir $bs1/miss-edited-dir
echo "content 12" > $bs1/miss-edited-dir/old-file
ln -s "path/to/12" $bs1/miss-edited-dir/old-symlink
mkdir $bs1/miss-edited-dir/old-dir

echo "content 13" > $bs1/both-edited-file
ln -s "path/to/13" $bs1/both-edited-symlink
mkdir $bs1/both-edited-dir
echo "content 14" > $bs1/both-edited-dir/old-file
ln -s "path/to/14" $bs1/both-edited-dir/old-symlink
mkdir $bs1/both-edited-dir/old-dir


./sync.sh $fake_home $bs1
./sync.sh $fake_home $bs2


rm $bs1/local-removed-file
rm $bs1/local-removed-symlink
rm -r $bs1/local-removed-dir

rm $bs1/both-removed-file
rm $bs1/both-removed-symlink
rm -r $bs1/both-removed-dir

echo "content 15" > $bs1/local-added-file
ln -s "path/to/15" $bs1/local-added-symlink
mkdir $bs1/local-added-dir
echo "content 16" > $bs1/local-added-dir/some-file
ln -s "path/to/16" $bs1/local-added-dir/some-symlink
mkdir $bs1/local-added-dir/some-dir

echo "content 17" > $bs1/both-added-file
ln -s "path/to/17" $bs1/both-added-symlink
mkdir $bs1/both-added-dir
echo "content 18" > $bs1/both-added-dir/some-file
ln -s "path/to/18" $bs1/both-added-dir/some-symlink
mkdir $bs1/both-added-dir/some-dir

echo "content 19" > $bs1/local-edited-file
rm $bs1/local-edited-symlink && ln -s "path/to/19" $bs1/local-edited-symlink
rm $bs1/local-edited-dir/old-file
rm $bs1/local-edited-dir/old-symlink
rm -r $bs1/local-edited-dir/old-dir
echo "content 20" > $bs1/local-edited-dir/new-file
ln -s "path/to/20" $bs1/local-edited-dir/new-symlink
mkdir $bs1/local-edited-dir/new-dir

echo "content 21" > $bs1/both-edited-file
rm $bs1/both-edited-symlink && ln -s "path/to/21" $bs1/both-edited-symlink
rm $bs1/both-edited-dir/old-file
rm $bs1/both-edited-dir/old-symlink
rm -r $bs1/both-edited-dir/old-dir
echo "content 22" > $bs1/both-edited-dir/new-file
ln -s "path/to/22" $bs1/both-edited-dir/new-symlink
mkdir $bs1/both-edited-dir/new-dir


rm $bs2/miss-removed-file
rm $bs2/miss-removed-symlink
rm -r $bs2/miss-removed-dir

rm $bs2/both-removed-file
rm $bs2/both-removed-symlink
rm -r $bs2/both-removed-dir

echo "content 23" > $bs2/miss-added-file
ln -s "path/to/23" $bs2/miss-added-symlink
mkdir $bs2/miss-added-dir
echo "content 24" > $bs2/miss-added-dir/some-file
ln -s "path/to/24" $bs2/miss-added-dir/some-symlink
mkdir $bs2/miss-added-dir/some-dir

echo "content 17" > $bs2/both-added-file
ln -s "path/to/17" $bs2/both-added-symlink
mkdir $bs2/both-added-dir
echo "content 18" > $bs2/both-added-dir/some-file
ln -s "path/to/18" $bs2/both-added-dir/some-symlink
mkdir $bs2/both-added-dir/some-dir

echo "content 25" > $bs2/miss-edited-file
rm $bs2/miss-edited-symlink && ln -s "path/to/25" $bs2/miss-edited-symlink
rm $bs2/miss-edited-dir/old-file
rm $bs2/miss-edited-dir/old-symlink
rm -r $bs2/miss-edited-dir/old-dir
echo "content 26" > $bs2/miss-edited-dir/new-file
ln -s "path/to/26" $bs2/miss-edited-dir/new-symlink
mkdir $bs2/miss-edited-dir/new-dir

echo "content 21" > $bs2/both-edited-file
rm $bs2/both-edited-symlink && ln -s "path/to/21" $bs2/both-edited-symlink
rm $bs2/both-edited-dir/old-file
rm $bs2/both-edited-dir/old-symlink
rm -r $bs2/both-edited-dir/old-dir
echo "content 22" > $bs2/both-edited-dir/new-file
ln -s "path/to/22" $bs2/both-edited-dir/new-symlink
mkdir $bs2/both-edited-dir/new-dir

./sync.sh $fake_home $bs1
./sync.sh $fake_home $bs2
./sync.sh $fake_home $bs1
./sync.sh $fake_home $bs2
./sync.sh $fake_home $bs1

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
