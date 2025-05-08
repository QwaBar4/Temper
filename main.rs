use pnet_datalink::{interfaces, Channel::Ethernet};
use pnet_datalink::{DataLinkSender, DataLinkReceiver};
use pnet_datalink::NetworkInterface;
use pnet_packet::Packet;
use pnet_packet::arp::{ArpHardwareTypes, ArpOperations, ArpPacket, MutableArpPacket};
use pnet_packet::ethernet::{EtherTypes, EthernetPacket, MutableEthernetPacket};
use ipnetwork::Ipv4Network;
use std::io;
use std::thread;
use std::str::FromStr;
use std::net::Ipv4Addr;
use std::time::Duration;
use std::sync::{Arc, Mutex};
use pnet_base::MacAddr;
use termion::{terminal_size, clear};
use termion::color;

fn scan(
    _interface: &NetworkInterface,
    ipv4_network: Ipv4Network,
    mut rx: Box<dyn DataLinkReceiver>,
    tx: Arc<Mutex<Box<dyn DataLinkSender>>>,
    target_ips: Vec<Ipv4Addr>,
    source_mac: MacAddr,
) -> Vec<(Ipv4Addr, String)> {
	println!("Begin scan");
    let tx_clone = Arc::clone(&tx);
    thread::spawn(move || {
        for target_ip in target_ips {
            let mut eth_buffer = [0u8; 42];
            let mut eth_packet = MutableEthernetPacket::new(&mut eth_buffer).unwrap();

            eth_packet.set_destination(MacAddr::from_str("FF:FF:FF:FF:FF:FF").unwrap());
            eth_packet.set_source(source_mac);
            eth_packet.set_ethertype(EtherTypes::Arp);

            let mut arp_buffer = [0u8; 28];
            let mut arp_packet = MutableArpPacket::new(&mut arp_buffer).unwrap();

            arp_packet.set_hardware_type(ArpHardwareTypes::Ethernet);
            arp_packet.set_protocol_type(EtherTypes::Ipv4);
            arp_packet.set_hw_addr_len(6);
            arp_packet.set_proto_addr_len(4);
            arp_packet.set_operation(ArpOperations::Request);
            arp_packet.set_sender_hw_addr(source_mac);
            arp_packet.set_sender_proto_addr(ipv4_network.ip());
            arp_packet.set_target_hw_addr(MacAddr::from_str("00:00:00:00:00:00").unwrap());
            arp_packet.set_target_proto_addr(target_ip);

            eth_packet.set_payload(arp_packet.packet());
            let mut tx = tx_clone.lock().unwrap();
            match tx.send_to(eth_packet.packet(), None) {
                Some(Ok(_)) => (),
                Some(Err(e)) => eprintln!("Failed to send to {}: {}", target_ip, e),
                None => eprintln!("Failed to send to {}: No result", target_ip),
            }
        }
    });

    let start = std::time::Instant::now();
    let timeout = Duration::from_secs(10);
    let mut cnt = 0;
    let mut ack_ips: Vec<(Ipv4Addr, String)> = Vec::new();
    while start.elapsed() < timeout {
        match rx.next() {
            Ok(packet) => {
                let ethernet = EthernetPacket::new(packet).unwrap();
                if ethernet.get_ethertype() == EtherTypes::Arp {
                    if let Some(arp) = ArpPacket::new(ethernet.payload()) {
                        if arp.get_operation() == ArpOperations::Reply {
                            let hw_addr = arp.get_sender_hw_addr().to_string();
                            cnt += 1;
                            println!(
                                "{}. Found: {} ({})",
                                cnt,
                                arp.get_sender_proto_addr(),
                                hw_addr
                            );
                            ack_ips.push((arp.get_sender_proto_addr(), hw_addr));
                        }
                    }
                }
            }
            Err(e) => eprintln!("Packet error: {}", e),
        }
    }
    if ack_ips.is_empty() { println!("No ips found"); }
    ack_ips
}

fn send(
	index: i32,
    ack_addr: &[(Ipv4Addr, String)],
    _interface: &NetworkInterface,
    ipv4_network: Ipv4Network,
    tx: Arc<Mutex<Box<dyn DataLinkSender>>>,
    source_mac: MacAddr,
    packets_per_ip: usize,
    delay_ms: u64,
) {
    let usize_index = index as usize;
    let mac_addr = ack_addr[usize_index].1.clone();
    let ip_addr = ack_addr[usize_index].0;

    let tx_clone = Arc::clone(&tx);
	
    println!("Sending data to: {}, wait {} seconds", ip_addr, (packets_per_ip as u64) * delay_ms / 100);
    thread::spawn(move || {
        for _seq in 1..=packets_per_ip {
            let mut eth_buffer = [0u8; 42];
            let mut eth_packet = MutableEthernetPacket::new(&mut eth_buffer).unwrap();

            eth_packet.set_destination(MacAddr::from_str(&mac_addr).unwrap());
            eth_packet.set_source(source_mac);
            eth_packet.set_ethertype(EtherTypes::Arp);

            let mut arp_buffer = [0u8; 28];
            let mut arp_packet = MutableArpPacket::new(&mut arp_buffer).unwrap();

            arp_packet.set_hardware_type(ArpHardwareTypes::Ethernet);
            arp_packet.set_protocol_type(EtherTypes::Ipv4);
            arp_packet.set_hw_addr_len(6);
            arp_packet.set_proto_addr_len(4);
            arp_packet.set_operation(ArpOperations::Request);
            arp_packet.set_sender_hw_addr(source_mac);
            arp_packet.set_sender_proto_addr(ipv4_network.ip());
            arp_packet.set_target_hw_addr(MacAddr::from_str(&mac_addr).unwrap());
            arp_packet.set_target_proto_addr(ip_addr);

            eth_packet.set_payload(arp_packet.packet());

            let mut tx = tx_clone.lock().unwrap();
            match tx.send_to(eth_packet.packet(), None) {
                Some(Ok(_)) => (),
                Some(Err(e)) => eprintln!("Failed to send to {}: {}", ip_addr, e),
                None => eprintln!("Failed to send to {}: No result", ip_addr),
            }
            if _seq < packets_per_ip {
                thread::sleep(Duration::from_millis(delay_ms));
            }
        }
    });
}


fn set_packets_per_ip(_packets_per_ip: usize) -> usize{
	let mut packet_str = String::new();
	println!("Please set your amount of packets here: ");
	io::stdin()
		.read_line(&mut packet_str)
		.expect("Failed to read line");
		    
	let packets_int: usize = packet_str.trim().parse().expect("Please type a number!");
	packets_int
}

fn set_delay(_delay_ms: u64) -> u64{
	let mut delay_str = String::new();
	println!("Please set time between packets here: ");
	io::stdin()
		.read_line(&mut delay_str)
		.expect("Failed to read line");
		    
	let delay_int: u64 = delay_str.trim().parse().expect("Please type a number!");
	delay_int
}


fn center_ascii(ascii: &str) {
    if let Ok((width, _)) = terminal_size() {
        for line in ascii.lines() {
            let line_len = line.chars().count();
            let padding = (width as usize - line_len) / 2;
            println!("{}{:>width$}{}", color::Fg(color::Red), line, color::Fg(color::Reset), width = line_len + padding);
        }
    } else {
        println!("{}{}{}",color::Fg(color::Red), ascii, color::Fg(color::Reset));
    }
}

fn main() {
	println!("{}", clear::All);
	let name = "
  ┏┳┓           
   ┃ ┏┓┏┳┓┏┓┏┓┏┓
   ┻ ┗ ┛┗┗┣┛┗ ┛ 
           ┛       
";
	center_ascii(name);
	println!("{}Created by: QwaBar4{}", color::Fg(color::Green), color::Fg(color::Reset));
    let interface = interfaces()
        .into_iter()
        .find(|iface| !iface.is_loopback() && iface.ips.iter().any(|ip| ip.is_ipv4()))
        .expect("No suitable network interface found");

    let ipv4_network = interface
        .ips
        .iter()
        .find(|ip| ip.is_ipv4())
        .map(|ip| {
            let ipv4_addr = ip.ip().to_string().parse::<Ipv4Addr>().unwrap();
            Ipv4Network::new(ipv4_addr, ip.prefix()).unwrap()
        })
        .unwrap();

    let target_ips: Vec<Ipv4Addr> = ipv4_network
        .iter()
        .filter(|ip| *ip != ipv4_network.network() && *ip != ipv4_network.broadcast())
        .collect();

    let (tx, _rx) = match pnet_datalink::channel(&interface, Default::default()) {
        Ok(Ethernet(tx, rx)) => (tx, rx),
        Ok(_) => panic!("Unsupported channel type"),
        Err(e) => panic!("Channel error: {}", e),
    };

    let source_mac = interface.mac.expect("Interface has no MAC address");
    
    let mut ack_ips: Vec<(Ipv4Addr, String)> = Vec::new();
    let tx_arc = Arc::new(Mutex::new(tx));
	
	let mut packets_per_ip: usize = 100000;
	let mut delay_ms: u64 = 2;
	
	loop{
		let mut input = String::new();
		println!("Choose option: ");
		println!("1 - Scan ips");
		println!("2 - Choose ip");
		println!("3 - Change packets amount");
		println!("4 - Change time between each packet");
		println!("99 - Leave");
		
		io::stdin()
		    .read_line(&mut input)
		    .expect("Failed to read line");
		    
		let choose: usize = input.trim().parse().expect("Please type a number!");
		match choose {
			1 => {
					let (new_tx, new_rx) = match pnet_datalink::channel(&interface, Default::default()) {
						Ok(Ethernet(tx, rx)) => (tx, rx),
						_ => {
							println!("Failed to create new channel");
							continue;
						}
					};
					*tx_arc.lock().unwrap() = new_tx;
					ack_ips = scan(
						&interface,
						ipv4_network,
						new_rx,
						tx_arc.clone(),
						target_ips.clone(),
						source_mac,
					);
					thread::sleep(Duration::from_millis(100));
				}
			2 => {
					if ack_ips.is_empty() {
						println!("You can't choose ip before scan");
					} else {
							println!("Please input number of ip that will be pushed: ");
						    let mut input = String::new();
							io::stdin()
								.read_line(&mut input)
								.expect("Failed to read line");
								
							let index: i32 = -1 + input.trim().parse::<i32>().expect("Please type a number!");
						send(
							index,
							&ack_ips,
							&interface,
							ipv4_network,
							tx_arc.clone(),
							source_mac,
							packets_per_ip,
							delay_ms,
						);
					}
					thread::sleep(Duration::from_millis(100));
				}
			3 => {
					println!("Current packet amount: {}", packets_per_ip);
					packets_per_ip = set_packets_per_ip(packets_per_ip);
				},
			4 => {
					println!("Current delay: {}ms", delay_ms);
					delay_ms = set_delay(delay_ms);
				},
			99 => {
					return;
				},
			_ => println!("No such option"),
		}
	}
    
}
