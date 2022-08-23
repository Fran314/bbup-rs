# BBUP (Baldo BackUP)
`bbup` is a backup software that manages and versions multiple backup sources with multiple backup types. The types of backup are bijective backup (which is basically synchronization), injective backup (for managing something like photos) and block injective backup (for projects).

## Install
Currently the only way to install the software is either to manually download the binaries from the latest releases or to build the binaries yourself.

### Download
You can download the latest release [here](https://github.com/Fran314/bbup-rs/releases/latest)
The binaries that you have to donwload depend on the system you're planning to run this on.  
The precompiled targets to choose between are:

| Target | Intended system |
| --- | --- |
| **x86_64-unknown-linux-gnu** | generic 64bit Linux system |
| **x86_64-pc-windows-gnu** | generic 64bit Windows system |
| **armv7-unknown-linux-gnueabihf** | Raspberry Pi 32bit, ... |
| **aarch64-unknown-linux-gnu** | Raspberry Pi 64bit, ... |

Make sure to download the correct binaries for your system

### Build
The following instructions are the procedure that I use to build the binaries for different targets on my machine, which is a Linux machine running Linux Mint. The process may vary on different OSs.

To build the surce code you'll need to have installed the rust toolchain. You can find how to do so [here](https://www.rust-lang.org/tools/install).

Clone the repository with
```bash
git clone https://github.com/Fran314/bbup-rs.git
```
Then enter the root directory of the repository, and the following procedure will change depending on which target you want to build for.

After you built the binaries, you will have two files `bbup` and `bbup-server`. Put `bbup` on the client machine and `bbup-server` on the server machine. I suggest you either put them or link them in a directory known to the `$PATH` variable (I use `~/.local/bin`).

#### Linux
Clone the repo, and inside the root directory of the repo run
```bash
cargo build --release
```
The binaries `bbup` and `bbup-server` will be in `./target/release`

#### Windows
Make sure you have installed the right target in the rust toolchain with
```bash
rustup target add x86_64-pc-windows-gnu
```
Then build the source code with
```bash
cargo build --release --target x86_64-pc-windows-gnu
```
The executables `bbup.exe` and `bbup-server.exe` will be in `./target/x86_64-pc-windows-gnu/release`

#### Raspberry Pi (64 bit)
Make sure you have installed the right target in the rust toolchain and the right linker with with
```bash
rustup target add aarch64-unknown-linux-gnu
sudo apt install gcc-aarch64-linux-gnu
```
and add the file `./.cargo/config` with the following content
```
[target.aarch64-unknown-linux-gnu]
linker = "aarch64-linux-gnu-gcc"
```

Then build the source code with
```bash
cargo build --release --target aarch64-unknown-linux-gnu
```

The binaries `bbup` and `bbup-server` will be in `./target/aarch64-linux-gnu-gcc/release`

#### Raspberry Pi (32 bit)
Make sure you have installed the right target in the rust toolchain and the right linker with with
```bash
rustup target add armv7-unknown-linux-gnueabihf
sudo apt install gcc-arm-linux-gnueabihf
```
and add the file `./.cargo/config` with the following content
```
[target.armv7-unknown-linux-gnueabihf]
linker = "arm-linux-gnueabihf-gcc"
```

Then build the source code with
```bash
cargo build --release --target armv7-unknown-linux-gnueabihf
```

The binaries `bbup` and `bbup-server` will be in `./target/armv7-unknown-linux-gnueabihf/release`


## Setup
download the binaries from the lates release or build them

### Server
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
