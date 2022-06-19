use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::{
    io::BufReader,
    net::{TcpListener, TcpStream},
};

use bbup_rust::comunications::asyncrw::{read, write};
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
        let (socket, _) = listener.accept().await?;
        let state = state.clone();
        tokio::spawn(async move {
            process(socket, state).await.unwrap();
        });
    }
}

async fn process(socket: TcpStream, state: Arc<Mutex<ServerState>>) -> std::io::Result<()> {
    let mut socket = BufReader::new(socket);
    let mut buffer = String::new();

    write(
        &mut socket,
        Basic::new(0, "bbup-server, connection established"),
    )
    .await?;
    let read_val: Basic = read(&mut socket, &mut buffer).await?;
    println!("Recieved from client: {}", read_val.content);

    let _state = match state.try_lock() {
        Ok(val) => val,
        Err(_) => {
            write(&mut socket, Basic::new(1, "server occupied")).await?;
            return Ok(());
        }
    };

    write(&mut socket, Basic::new(0, "Hello client")).await?;

    Ok(())
}
