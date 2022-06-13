use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{TcpListener, TcpStream},
};

#[derive(Serialize, Deserialize)]
struct Message {
    id: String,
    content: String,
}

fn custom_error(error: String) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::Other, error)
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let busy = Arc::new(Mutex::new(false));

    let listener = TcpListener::bind("127.0.0.1:3000").await?;

    loop {
        let (socket, _) = listener.accept().await?;
        let busy = busy.clone();
        tokio::spawn(async move {
            process(socket, busy).await.unwrap();
        });
    }
}

async fn process(socket: TcpStream, busy: Arc<Mutex<bool>>) -> std::io::Result<()> {
    let mut socket = BufReader::new(socket);
    let mut input = String::new();

    socket.write(b"bbup-server\n").await?;
    socket.flush().await?;

    socket.read_line(&mut input).await?;
    println!("Recieved from client: {}", input);
    let deserialized: Message = serde_json::from_str(&input)?;

    let procede: bool;
    {
        let mut busy = match busy.lock() {
            Ok(val) => val,
            Err(error) => return Err(custom_error(error.to_string())),
        };

        if *busy {
            procede = false;
        } else {
            procede = true;
            *busy = true;
        }
    }

    if !procede {
        socket.write(b"bbup-server occupied").await?;
        return Ok(());
    }

    socket
        .write(format!("Hello there {}\n", deserialized.id).as_bytes())
        .await?;
    socket.flush().await?;

    Ok(())
}
