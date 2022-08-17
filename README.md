# BBUP (Baldo BackUP)

## Setup
download the binaries from the lates release or build them

### Server
- Put or link the `bbup-server` binary in a place known to the PATH variable (I use `~/.local/bin`)
- Create a directory that will be your archive root
- Setup the server with
	```bash
	bbup-server setup
	```
	You'll be asked to input the following:
	- [server_port]: the port that the daemon will listen on. This port does not need to be portforwarded, as the comunication will happen on an ssh tunnel
	- [archive_root]: the path to the root of the archive. This path must be relative to ~ (i.e: if the archive root is ~/foo/bar/archive/, enter `foo/bar/archive/`)
- Start the server with
	```bash
	bbup-server run
	```

### Client
- Put or link the `bbup` binary in a place known to the PATH variable (I use `~/.local/bin`)
- Setup the client with
	```bash
	bbup setup
	```
	You will be asked to input the following:
	- [local_port]: the port that the client will connect to
	- [server_port]: the port utilized by the daemon on the server
	- [host_user]: the username of the user utilized by the server
	- [host_address]: the address of the server
- Create a directory that will be your backup source
- Move to the backup source and initialize the source with
	```bash
	cd to/the/backup/source
	bbup init
	```
	You will be asked to input the following:
	- [endpoint]: the endpoint for the backup of this backup source. This is a path to the root of the endpoint relative to the root of the archive (i.e: if the endpoint is `~/foo/bar/archive/moo/boo/my-photos`, enter `moo/boo/my-photos`)
	- [exclude_list]: the list of paths to exclude (like a .gitignore)

## Build

### Raspberry Pi (64 bit)
To build for RaspberryPi (probably 4, don't know for other models)
```bash
cargo build --release --target aarch64-unknown-linux-gnu
```
If it doesn't work, add the target and install the linker install
```bash
rustup target add aarch64-unknown-linux-gnu
sudo apt install gcc-aarch64-linux-gnu
```
and add the file ./.cargo/config with the following content
```
[target.aarch64-unknown-linux-gnu]
linker = "aarch64-linux-gnu-gcc"
```

### Raspberry Pi (32 bit)
To build for RaspberryPi (32 bit)
```bash
cargo build --release --target armv7-unknown-linux-gnueabihf
```
If it doesn't work, add the target and install the linker install
```bash
rustup target add armv7-unknown-linux-gnueabihf
sudo apt install gcc-arm-linux-gnueabihf
```
and add the file ./.cargo/config with the following content
```
[target.armv7-unknown-linux-gnueabihf]
linker = "arm-linux-gnueabihf-gcc"
```