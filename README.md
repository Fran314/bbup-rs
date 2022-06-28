# BBUP (Baldo BackUP)

## Setup
download the binaries from the lates release or build them

> N.B: the system isn't fully implemented yet. This build is for test only, and because of this the server and the client must be on the same machine. Furthermore, [server_port] and [local_port] must be the same value. On future builds, the only restriction will be that the server must be on a rechable address form the client and it will need to be accessible via ssh; the [server_port] and [local_port] will be allowed to be any value (not necessarily the same)

> For now I suggest to use something like [server_port] = [local_port] = 3000

> To emulate the final result, you can use different values for [server_port] and [local_port] (let's say [server_port] = 4000 and [local_port] = 3000) and run the following command on another terminal  
> `ssh -R [local_port]:localhost:[server_port] [host_user]@[host_address]`

### Server
- Put or link the `bbup-server` binary in a place known to the PATH variable (I use `~/.local/bin`)
- Create a directory that will be your archive root
- Setup the server with
	```bash
	bbup-server setup
	```
	You'll be asked to input the following:
	- [server_port]: the port that the daemon will listen on. This port does not need to be portforwarded, as the comunication will happen on an ssh tunnel
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
	- [server_port]: the port utilized by the daemon on the server (not useful yet)
	- [host_user]: the username of the user utilized by the server (not useful yet)
	- [host_address]: the address of the server (not useful yet)
- Create a directory that will be your backup source
- Move to the backup source and initialize the source with
	```bash
	cd to/the/backup/source
	bbup init
	```
	You will be asked to 

## Other
To build for RaspberryPi (probably 4, don't know for other models)
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