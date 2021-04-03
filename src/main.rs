use anyhow::{bail, Context, Result};
use structopt::StructOpt;
use clap::arg_enum;
use std::io::Cursor;
use std::io::{Error, ErrorKind};
use std::net::{IpAddr, SocketAddr};
use tokio::io::{AsyncReadExt, AsyncWriteExt, Interest};
use tokio::net::{tcp::WriteHalf, TcpListener, TcpStream};
use log::{info, error, debug, warn};

arg_enum!{
    #[derive(PartialEq, Debug, Clone)]
    pub enum ProxyProtocol {
        Disabled,
        V1,
        V2,
    }
}

#[derive(Debug, StructOpt, Clone)]
#[structopt(name="r2lay", about="A simple TCP relay made in Rust.")]
struct Opt {
    /// Enable Proxy Protocol 
    /// 
    /// Add Proxy Protocol header to each connection to the server.
    #[structopt(short="P", long, possible_values = &ProxyProtocol::variants(), case_insensitive = true, default_value = "disabled")]
    proxy_protocol: ProxyProtocol,

    /// The listening TCP address with IP(v4/v6) and port
    #[structopt(parse(try_from_str))]
    proxy_addr: SocketAddr,

    /// Back-end TCP address with IP(v4/v6) and port
    #[structopt(parse(try_from_str))]
    server_addr: SocketAddr,
}


#[tokio::main]
async fn main() -> Result<()> {
    println!("Hello, world!");
    let opt = Opt::from_args();
    pretty_env_logger::init();
    let listener = TcpListener::bind(&opt.proxy_addr).await?;

    loop {
        match listener.accept().await {
            Ok((socket, addr)) => {
                let opt = opt.clone();
                tokio::spawn(async move {
                    info!("{:?} - new connection", addr);
                    match handle(socket, opt).await {
                        Ok(()) => info!("{:?} - connection closed!", addr),
                        Err(err) => {
                            if let Some(e) = err.downcast_ref::<Error>() {
                                match e.kind() {
                                    ErrorKind::BrokenPipe => warn!("{:?} - broken pipe!", addr),
                                    ErrorKind::ConnectionRefused => {
                                        warn!("{:?} - broken pipe!", addr)
                                    }
                                    ErrorKind::ConnectionAborted => {
                                        warn!("{:?} - connection abort!", addr)
                                    }
                                    ErrorKind::ConnectionReset => {
                                        warn!("{:?} - connection reset!", addr)
                                    }
                                    _ => error!("{:?} - unexpected error:\n{:?}", addr, e),
                                }
                            }
                        }
                    }
                });
            }
            Err(e) => println!("{:?} - bad handshake!", e),
        }
    }

    Ok(())
}

async fn handle(mut client: TcpStream, opt: Opt) -> Result<()> {
    let mut server = TcpStream::connect(&opt.server_addr).await?;
    let (mut s_rx, mut s_tx) = server.split();
    match &opt.proxy_protocol {
        ProxyProtocol::Disabled => {},
        ProxyProtocol::V1 => proxyv1_header(&client, &mut s_tx).await?,
        ProxyProtocol::V2 => proxyv2_header(&client, &mut s_tx).await?,
    };
    let (mut c_rx, mut c_tx) = client.split();
    let mut buf_server = [0; 4096];
    let mut buf_client = [0; 4096];
    loop {
        tokio::select! {
            bytes = c_rx.read(&mut buf_client[..]) => {
                match bytes {
                    Ok(0) => break,
                    Ok(n) => {
                        debug!("SEND: {:?}", std::str::from_utf8(&buf_client[..n])?);
                        s_tx.write_all(&buf_client[..n]).await?;
                    }
                    _ => break,
                }
            }
            bytes = s_rx.read(&mut buf_server[..]) => {
                match bytes {
                    Ok(0) => break,
                    Ok(n) => {
                        if n > 0 {
                            debug!("RECV: {:?}", std::str::from_utf8(&buf_server[..n])?);
                            c_tx.write_all(&buf_server[..n]).await?;
                        }
                    }
                    _ => break,
                }
            }
        }
    }
    Ok(())
}

async fn proxyv2_header(client: &TcpStream, server_tx: &mut WriteHalf<'_>) -> Result<()> {
    let proxy_addr = client.local_addr()?;
    let client_addr = client.peer_addr()?;
    let mut header: Cursor<Vec<u8>> = Cursor::new(Vec::new());
    // PROXY PROTOCOL SIGNATURE
    header
        .write_all(b"\x0D\x0A\x0D\x0A\x00\x0D\x0A\x51\x55\x49\x54\x0A")
        .await?;
    header.write_all(b"\x21").await?; // Version + Command (here: v2 - PROXY)
    match client_addr.ip() {
        IpAddr::V4(ipv4) => {
            header.write_all(b"\x11").await?; // AF_INET using SOCK_STREAM (TCPv4)
            header.write_all(&12u16.to_be_bytes()).await?; // Len: 12
            header.write_all(&ipv4.octets()).await?; // src_addr in Network bytes order
        }
        IpAddr::V6(ipv6) => {
            header.write_all(b"\x21").await?; // AF_INET6 using SOCK_STREAM (TCPv6)
            header.write_all(&36u16.to_be_bytes()).await?; // Len: 36
            header.write_all(&ipv6.octets()).await?; // src_addr in Network bytes order
        }
    };
    match proxy_addr.ip() {
        IpAddr::V4(ipv4) => {
            header.write_all(&ipv4.octets()).await?; // dst_addr in Network bytes order
        }
        IpAddr::V6(ipv6) => {
            header.write_all(&ipv6.octets()).await?; // dst_addr in Network bytes order
        }
    };
    header.write_all(&client_addr.port().to_be_bytes()).await?; // src_port in Network bytes order
    header.write_all(&proxy_addr.port().to_be_bytes()).await?; // dst_port in Network bytes order
    server_tx.write_all(&header.into_inner()).await?; // Write header to socket...
    Ok(())
}

async fn proxyv1_header(client: &TcpStream, server_tx: &mut WriteHalf<'_>) -> Result<()> {
    let proxy_addr = client.local_addr()?;
    let client_addr = client.peer_addr()?;
    match client_addr.ip() {
        IpAddr::V4(ipv4) => {
            server_tx
                .write_all(
                    format!(
                        "PROXY TCP4 {} {} {} {}\r\n",
                        ipv4.to_string(),
                        proxy_addr.ip().to_string(),
                        client_addr.port(),
                        proxy_addr.port()
                    )
                    .as_bytes(),
                )
                .await?;
        }
        IpAddr::V6(ipv6) => {
            server_tx
                .write_all(
                    format!(
                        "PROXY TCP6 {} {} {} {}\r\n",
                        ipv6.to_string(),
                        proxy_addr.ip().to_string(),
                        client_addr.port(),
                        proxy_addr.port()
                    )
                    .as_bytes(),
                )
                .await?;
        }
    };
    Ok(())
}
