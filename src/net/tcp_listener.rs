use core::sync::atomic::Ordering;
use super::CTRL_C;
use super::eth::{EthFrame, ETHERTYPE_ARP, ETHERTYPE_IP};
use super::ipv4;
use super::tcp::{TcpSocket, TcpState, TcpSegment, FLAG_SYN, FLAG_ACK, FLAG_RST, FLAG_FIN, FLAG_PSH, build as tcp_build, checksum as tcp_checksum};

const RX_BUF: usize = 4096;

pub struct TcpConn {
    socket: TcpSocket,
}

impl TcpConn {
    pub fn send(&mut self, data: &[u8]) -> bool {
        self.socket.send(data)
    }

    pub fn recv_wait(&mut self, timeout_iters: usize) -> &[u8] {
        self.socket.recv_wait(timeout_iters)
    }

    pub fn recv_all(&mut self, timeout_iters: usize) -> &[u8] {
        self.socket.recv_all(timeout_iters)
    }

    pub fn close(&mut self) {
        self.socket.close();
    }

    pub fn peer_ip(&self) -> [u8; 4] {
        self.socket.remote_ip
    }
    pub fn peer_port(&self) -> u16 {
        self.socket.remote_port
    }
    pub fn is_connected(&self) -> bool {
        self.socket.is_connected()
    }
}

pub struct TcpListener {
    pub port: u16,
}

impl TcpListener {
    pub fn bind(port: u16) -> Self {
        Self { port }
    }

    pub fn accept(&self) -> Option<TcpConn> {
        let local_ip  = super::get_ip();
        let local_mac = super::get_mac();

        let mut raw: [[u8; 1520]; 4] = [[0; 1520]; 4];
        let mut raw_lens = [0usize; 4];
        let mut raw_n = 0usize;
        {
            let mut state = super::NET.lock();
            if let Some(drv) = state.driver.as_mut() {
                drv.recv(&mut |buf| {
                    if raw_n < 4 {
                        let l = buf.len().min(1520);
                        raw[raw_n][..l].copy_from_slice(&buf[..l]);
                        raw_lens[raw_n] = l;
                        raw_n += 1;
                    }
                });
            }
            state.rx_count += raw_n as u64;
        }

        for i in 0..raw_n {
            let buf = &raw[i][..raw_lens[i]];

            let frame = match EthFrame::parse(buf) { Some(f) => f, None => continue };

            if frame.ethertype == ETHERTYPE_ARP {
                let mut state = super::NET.lock();
                let mc = state.mac;
                let ic = state.ip;
                let mut rep = [0u8; 64];
                let rlen = super::arp::handle(&frame, &mc, &ic, &mut state.arp, &mut rep);
                if rlen > 0 {
                    let rc = rep;
                    if let Some(drv) = state.driver.as_mut() { drv.send(&rc[..rlen]); }
                }
                continue;
            }

            if frame.ethertype != ETHERTYPE_IP { continue; }

            let ip = match ipv4::Ipv4Header::parse(frame.payload) { Some(h) => h, None => continue };
            if ip.proto != ipv4::PROTO_TCP { continue; }

            let tcp_payload = ip.payload(frame.payload);
            let seg = match TcpSegment::parse(tcp_payload) { Some(s) => s, None => continue };

            if seg.dst_port != self.port { continue; }
            if seg.flags & FLAG_SYN == 0 || seg.flags & FLAG_ACK != 0 { continue; }

            return self.complete_handshake(
                &frame, &ip, &seg, local_ip, local_mac,
            );
        }

        None
    }

    fn complete_handshake(
        &self,
        frame:     &EthFrame,
        ip:        &ipv4::Ipv4Header,
        syn:       &TcpSegment,
        local_ip:  [u8; 4],
        local_mac: [u8; 6],
    ) -> Option<TcpConn> {
        let isn: u32 = (crate::vfs::procfs::uptime_ticks() as u32)
            .wrapping_mul(0x9E3779B9)
            ^ u32::from_be_bytes(ip.src);

        let client_ip  = ip.src;
        let client_mac = frame.src;
        let client_port = syn.src_port;

        let ack_num = syn.seq.wrapping_add(1);
        self.send_tcp(
            self.port, client_port,
            isn, ack_num,
            FLAG_SYN | FLAG_ACK, 8192, &[],
            &local_ip, &client_ip, &local_mac, &client_mac,
        );

        for _ in 0..2_000_000 {
            if CTRL_C.load(Ordering::SeqCst) { return None; }

            let mut raw: [[u8; 1520]; 4] = [[0; 1520]; 4];
            let mut raw_lens = [0usize; 4];
            let mut raw_n = 0usize;
            {
                let mut state = super::NET.lock();
                if let Some(drv) = state.driver.as_mut() {
                    drv.recv(&mut |buf| {
                        if raw_n < 4 {
                            let l = buf.len().min(1520);
                            raw[raw_n][..l].copy_from_slice(&buf[..l]);
                            raw_lens[raw_n] = l;
                            raw_n += 1;
                        }
                    });
                }
            }

            for i in 0..raw_n {
                let buf = &raw[i][..raw_lens[i]];
                let f = match EthFrame::parse(buf) { Some(f) => f, None => continue };
                if f.ethertype != ETHERTYPE_IP { continue; }
                let ih = match ipv4::Ipv4Header::parse(f.payload) { Some(h) => h, None => continue };
                if ih.proto != ipv4::PROTO_TCP { continue; }
                if ih.src != client_ip { continue; }
                let tp = ih.payload(f.payload);
                let seg = match TcpSegment::parse(tp) { Some(s) => s, None => continue };
                if seg.src_port != client_port { continue; }
                if seg.dst_port != self.port   { continue; }

                if seg.flags & FLAG_ACK != 0 && seg.flags & FLAG_SYN == 0 {
                    let mut sock = TcpSocket::new();
                    sock.state       = TcpState::Established;
                    sock.local_ip    = local_ip;
                    sock.local_mac   = local_mac;
                    sock.local_port  = self.port;
                    sock.remote_ip   = client_ip;
                    sock.remote_mac  = client_mac;
                    sock.remote_port = client_port;
                    sock.seq         = isn.wrapping_add(1);
                    sock.ack         = ack_num;

                    crate::log!("tcp_listener: accepted {}:{} -> port {}",
                        client_ip[0], client_ip[1], self.port);

                    return Some(TcpConn { socket: sock });
                }
            }

            core::hint::spin_loop();
        }

        crate::log_err!("tcp_listener: handshake timeout (no ACK received)");
        None
    }

    #[allow(clippy::too_many_arguments)]
    fn send_tcp(
        &self,
        src_port: u16, dst_port: u16,
        seq: u32, ack: u32,
        flags: u8, window: u16, payload: &[u8],
        src_ip: &[u8; 4], dst_ip: &[u8; 4],
        src_mac: &[u8; 6], dst_mac: &[u8; 6],
    ) {
        let mut tcp_buf = [0u8; 1480];
        let tcp_len = tcp_build(
            src_port, dst_port, seq, ack, flags, window,
            payload, src_ip, dst_ip, &mut tcp_buf,
        );
        if tcp_len == 0 { return; }

        let mut ip_buf = [0u8; 1500];
        let ip_len = ipv4::build(src_ip, dst_ip, ipv4::PROTO_TCP, &tcp_buf[..tcp_len], &mut ip_buf);
        if ip_len == 0 { return; }

        let mut eth_buf = [0u8; 1520];
        let eth_len = EthFrame::build(dst_mac, src_mac, ETHERTYPE_IP, &ip_buf[..ip_len], &mut eth_buf);
        if eth_len == 0 { return; }

        let mut state = super::NET.lock();
        if let Some(drv) = state.driver.as_mut() {
            drv.send(&eth_buf[..eth_len]);
        }
        state.tx_count += 1;
    }
}
