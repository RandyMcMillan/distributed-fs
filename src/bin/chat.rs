use tokio::{net::TcpListener, io::AsyncWriteExt, io::BufReader, io::AsyncBufReadExt, sync::broadcast, io::AsyncReadExt };
use std::str;
use std::io;
use std::net::SocketAddr;

#[tokio::main]
async fn main() -> io::Result<()> {
    let listener = TcpListener::bind("192.168.254.13:8080").await?;

    let (tx, _rx) = broadcast::channel::<(String, SocketAddr)>(20);

    loop {
        let (mut socket, addr) = listener.accept().await?;
        println!("new client {}", addr);

        let tx = tx.clone();
        let mut rx = tx.subscribe();

        tokio::spawn(async move {
            let mut buffer = [0u8; 1024];
            let bytes_read  = socket.read(&mut buffer).await.unwrap();

            let username = str::from_utf8(&buffer[..bytes_read - 2]).unwrap();

            let (reader, mut writer) = socket.split(); 
            let mut reader = BufReader::new(reader);
            let mut line = String::new();

            loop {
                tokio::select! {
                    bytes_read = reader.read_line(&mut line) => {
                        if bytes_read.unwrap() == 0 {
                            break
                        }

                        let msg = format!("{}: {}", username, line.clone());

                        tx.send((msg, addr)).unwrap();
                        line.clear();
                    }, 
                    msg = rx.recv() => {
                        let (msg, other_addr) = msg.unwrap();

                        if addr != other_addr {
                            writer.write_all(msg.as_bytes()).await.unwrap();
                        }
                    }
                }
            }
        });
    }
}

