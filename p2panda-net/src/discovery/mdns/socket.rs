// SPDX-License-Identifier: AGPL-3.0-or-later

use std::net::{IpAddr, Ipv4Addr, SocketAddrV4};

use anyhow::{Context, Result};
use hickory_proto::op::Message;
use hickory_proto::serialize::binary::BinEncodable;
use socket2::{Domain, Protocol, Socket, Type};
use tokio::net::UdpSocket;
use tracing::error;

const MDNS_IPV4: Ipv4Addr = Ipv4Addr::new(224, 0, 0, 251);
const MDNS_PORT: u16 = 5353;

pub fn socket_v4() -> Result<UdpSocket> {
    let socket =
        Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP)).context("Socket::new")?;
    socket
        .set_reuse_address(true)
        .context("set_reuse_address")?;
    #[cfg(unix)]
    socket.set_reuse_port(true).context("set_reuse_port")?;
    socket
        .bind(&SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, MDNS_PORT).into())
        .context("bind")?;
    socket
        .set_multicast_loop_v4(true)
        .context("set_multicast_loop_v4")?;
    socket
        .join_multicast_v4(&MDNS_IPV4, &Ipv4Addr::UNSPECIFIED)
        .context("join_multicast_v4")?;
    socket
        .set_multicast_ttl_v4(16)
        .context("set_multicast_ttl_v4")?;
    socket.set_nonblocking(true).context("set_nonblocking")?;
    UdpSocket::from_std(std::net::UdpSocket::from(socket)).context("from_std")
}

pub async fn send(socket: &UdpSocket, message: Message) {
    let bytes = match message.to_bytes() {
        Ok(bytes) => bytes,
        Err(err) => {
            error!("failed encoding DNS message: {}", err);
            return;
        }
    };

    if let Err(err) = socket
        .send_to(&bytes, (IpAddr::from(MDNS_IPV4), MDNS_PORT))
        .await
    {
        error!("failed sending mdns message on udp socket: {}", err);
    }
}
