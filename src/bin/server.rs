use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{TcpListener, TcpStream},
};

use bbup_rust::comunications::Basic;

struct ServerState {
    _commit_list: Vec<String>,
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let state = Arc::new(Mutex::new(ServerState {
        _commit_list: Vec::<String>::new(),
    }));

    let listener = TcpListener::bind("127.0.0.1:3000").await?;

    loop {
        let (socket, addr) = listener.accept().await?;
        let state = state.clone();
        tokio::spawn(async move {
            process(socket, state).await.unwrap();
        });
    }
}

async fn write<T: Serialize>(socket: &mut BufReader<TcpStream>, content: T) -> std::io::Result<()> {
    socket
        .write((serde_json::to_string(&content)? + "\n").as_bytes())
        .await?;
    socket.flush().await?;

    Ok(())
}

async fn read<'a, T: Deserialize<'a>>(
    socket: &mut BufReader<TcpStream>,
    buffer: &'a mut String,
) -> std::io::Result<T> {
    buffer.clear();
    socket.read_line(buffer).await?;
    let output: T = serde_json::from_str(buffer.as_str())?;
    Ok(output)
}

async fn process(socket: TcpStream, state: Arc<Mutex<ServerState>>) -> std::io::Result<()> {
    let mut socket = BufReader::new(socket);
    let mut buffer = String::new();

    write(&mut socket, Basic::new("bbup-server")).await?;
    let read_val: Basic = read(&mut socket, &mut buffer).await?;
    println!("Recieved from client: {}", read_val.content);

    let _state = match state.try_lock() {
        Ok(val) => val,
        Err(_) => {
            write(&mut socket, Basic::new("bbup-server occupied")).await?;
            return Ok(());
        }
    };

    write(&mut socket, Basic::new("Hello there")).await?;

    let read_val: Basic = read(&mut socket, &mut buffer).await?;
    println!("Recieved from client: {}", read_val.content);

    Ok(())
}
