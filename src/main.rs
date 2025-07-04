use socket2::{Domain, Protocol, Socket, Type};
use std::net::{Ipv4Addr, SocketAddrV4, UdpSocket};
use std::{thread, time::Duration};
use if_addrs::{get_if_addrs, IfAddr};
use rand::Rng;

fn main() -> std::io::Result<()> {
    // Generate a random number at the beginning
    let my_id: u32 = rand::thread_rng().r#gen();
    println!("My ID: {}", my_id);
    
    let multicast_addr = Ipv4Addr::new(239, 255, 0, 1);
    let port = 6000;
    let multicast_sockaddr = SocketAddrV4::new(multicast_addr, port);

    // Get all usable interfaces
    let interfaces: Vec<Ipv4Addr> = get_if_addrs()
        .expect("Could not get interfaces")
        .into_iter()
        .filter(|iface| !iface.is_loopback())
        .filter_map(|iface| match iface.addr {
            IfAddr::V4(ipv4) => Some(ipv4.ip),
            _ => None,
        })
        .collect();

    println!("Available interfaces: {:?}", interfaces);

    // Create receiver socket and join multicast group on all interfaces
    let recv_socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
    recv_socket.set_reuse_address(true)?;
    recv_socket.set_reuse_port(true)?;
    
    // Enable IP_MULTICAST_ALL to receive on all interfaces
    recv_socket.set_multicast_all_v4(true)?;
    
    // Bind to the multicast port on all interfaces
    let bind_addr = SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, port);
    recv_socket.bind(&bind_addr.into())?;
    
    // Join multicast group on all interfaces
    for iface_ip in &interfaces {
        recv_socket.join_multicast_v4(&multicast_addr, iface_ip)?;
        println!("Joined multicast group {} on interface {}", multicast_addr, iface_ip);
    }
    
    let recv_socket: UdpSocket = recv_socket.into();
    recv_socket.set_nonblocking(true)?;

    // Clone necessary data for the sender thread
    let sender_id = my_id;
    let sender_interfaces = interfaces.clone();
    
    // Create sender thread
    thread::spawn(move || loop {
        let msg = format!("ID: {}", sender_id);

        for iface_ip in &sender_interfaces {
            // Create a socket for each interface
            let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP)).unwrap();
            socket.set_multicast_if_v4(iface_ip).unwrap();
            socket.set_multicast_loop_v4(false).unwrap(); // don't receive our own send

            // Bind is optional for sending, but good for control (we use ephemeral port)
            let local_addr = SocketAddrV4::new(*iface_ip, 0);
            socket.bind(&local_addr.into()).unwrap();

            let std_sock: std::net::UdpSocket = socket.into();
            std_sock.send_to(msg.as_bytes(), multicast_sockaddr).unwrap();
        }

        thread::sleep(Duration::from_secs(2));
    });

    // Main thread receives multicast messages
    let mut buf = [0u8; 1024];
    loop {
        match recv_socket.recv_from(&mut buf) {
            Ok((len, addr)) => {
                let msg = String::from_utf8_lossy(&buf[..len]);
                
                // Parse the received ID from the message
                if let Some(received_id_str) = msg.strip_prefix("ID: ") {
                    if let Ok(received_id) = received_id_str.trim().parse::<u32>() {
                        // Ignore our own messages
                        if received_id != my_id {
                            println!("Received from {}: {}", addr, msg);
                        }
                    }
                }
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                // No data available, sleep briefly
                thread::sleep(Duration::from_millis(100));
            }
            Err(e) => {
                eprintln!("Error receiving: {}", e);
            }
        }
    }
}
