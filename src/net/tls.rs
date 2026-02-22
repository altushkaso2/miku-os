use core::sync::atomic::Ordering;
use super::CTRL_C;
use super::tcp::TcpSocket;
use super::tls_crypto::{
    Aes128, cbc_encrypt, cbc_decrypt, tls_pad, tls_unpad,
    sha1, sha256, Sha256State, hmac_sha1, hmac_sha256, prf_sha256,
};
use super::tls_bignum::bn_from_bytes_be;
use super::tls_rsa::{parse_rsa_public_key, rsa_pkcs1_encrypt};

const RT_CHANGE_CIPHER_SPEC: u8 = 20;
const RT_ALERT:              u8 = 21;
const RT_HANDSHAKE:          u8 = 22;
const RT_APP_DATA:           u8 = 23;

const HT_CLIENT_HELLO:        u8 = 1;
const HT_SERVER_HELLO:        u8 = 2;
const HT_CERTIFICATE:         u8 = 11;
const HT_SERVER_HELLO_DONE:   u8 = 14;
const HT_CLIENT_KEY_EXCHANGE: u8 = 16;
const HT_FINISHED:            u8 = 20;

const TLS12: [u8; 2] = [0x03, 0x03];

const CS_RSA_AES128_SHA:    [u8; 2] = [0x00, 0x2F];
const CS_RSA_AES128_SHA256: [u8; 2] = [0x00, 0x3C];

const ALERT_CLOSE_NOTIFY:          u8 = 0;
const ALERT_HANDSHAKE_FAILURE:     u8 = 40;
const ALERT_PROTOCOL_VERSION:      u8 = 70;
const ALERT_INSUFFICIENT_SECURITY: u8 = 71;
const ALERT_INTERNAL_ERROR:        u8 = 80;

fn rdtsc() -> u64 {
    let lo: u32;
    let hi: u32;
    unsafe {
        core::arch::asm!("rdtsc", out("eax") lo, out("edx") hi, options(nostack, nomem));
    }
    ((hi as u64) << 32) | lo as u64
}

fn fill_random(buf: &mut [u8]) {
    let mut state = rdtsc()
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    for b in buf.iter_mut() {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        *b = (state >> 33) as u8;
    }
}

fn make_record(rtype: u8, body: &[u8], out: &mut [u8]) -> usize {
    let len = body.len();
    out[0] = rtype;
    out[1] = 0x03;
    out[2] = 0x03;
    out[3] = (len >> 8) as u8;
    out[4] = len as u8;
    out[5..5 + len].copy_from_slice(body);
    5 + len
}

fn make_handshake(htype: u8, body: &[u8], out: &mut [u8]) -> usize {
    let len = body.len();
    out[0] = htype;
    out[1] = (len >> 16) as u8;
    out[2] = (len >> 8) as u8;
    out[3] = len as u8;
    out[4..4 + len].copy_from_slice(body);
    4 + len
}

fn alert_desc(code: u8) -> &'static str {
    match code {
        ALERT_CLOSE_NOTIFY           => "close_notify",
        40                           => "handshake_failure",
        ALERT_PROTOCOL_VERSION       => "protocol_version",
        ALERT_INSUFFICIENT_SECURITY  => "insufficient_security",
        ALERT_INTERNAL_ERROR         => "internal_error",
        20                           => "bad_record_mac",
        22                           => "record_overflow",
        42                           => "bad_certificate",
        43                           => "unsupported_certificate",
        44                           => "certificate_revoked",
        45                           => "certificate_expired",
        46                           => "certificate_unknown",
        47                           => "illegal_parameter",
        48                           => "unknown_ca",
        50                           => "decode_error",
        51                           => "decrypt_error",
        _                            => "unknown",
    }
}

fn compute_mac(mac_len: usize, key: &[u8; 32], data: &[u8]) -> [u8; 32] {
    let mut out = [0u8; 32];
    if mac_len == 20 {
        let m = hmac_sha1(&key[..20], data);
        out[..20].copy_from_slice(&m);
    } else {
        let m = hmac_sha256(key, data);
        out[..32].copy_from_slice(&m);
    }
    out
}

pub struct TlsStream {
    tcp: TcpSocket,
    client_random:   [u8; 32],
    server_random:   [u8; 32],
    master_secret:   [u8; 48],
    mac_len:         usize,
    selected_cipher: u16,
    client_mac_key:  [u8; 32],
    server_mac_key:  [u8; 32],
    client_key:      [u8; 16],
    server_key:      [u8; 16],
    client_seq:      u64,
    server_seq:      u64,
    cipher_active:   bool,
    raw:             [u8; 17408],
    raw_len:         usize,
    hs_buf:          [u8; 16384],
    hs_len:          usize,
    pub rx_buf:      [u8; 8192],
    pub rx_len:      usize,
}

impl TlsStream {
    fn tcp_fill(&mut self, need: usize, timeout: usize) -> bool {
        for _ in 0..timeout {
            if CTRL_C.load(Ordering::SeqCst) { return false; }
            if self.raw_len >= need { return true; }
            if self.tcp.peer_closed { return false; }

            let mut temp = [0u8; 4096];
            let mut tlen = 0usize;
            self.tcp.recv_one_into(&mut temp, &mut tlen);
            
            if tlen > 0 {
                let copy = tlen.min(self.raw.len() - self.raw_len);
                self.raw[self.raw_len..self.raw_len + copy].copy_from_slice(&temp[..copy]);
                self.raw_len += copy;
            }

            if self.raw_len >= need { return true; }
            core::hint::spin_loop();
        }
        self.raw_len >= need
    }

    fn consume(&mut self, n: usize) {
        if n >= self.raw_len {
            self.raw_len = 0;
        } else {
            self.raw.copy_within(n..self.raw_len, 0);
            self.raw_len -= n;
        }
    }

    fn send_record(&mut self, rtype: u8, data: &[u8]) {
        if !self.cipher_active {
            let mut rec = [0u8; 4096];
            let n = make_record(rtype, data, &mut rec);
            self.tcp.send(&rec[..n]);
            return;
        }

        let mut mac_input = [0u8; 2048];
        mac_input[0..8].copy_from_slice(&self.client_seq.to_be_bytes());
        mac_input[8]  = rtype;
        mac_input[9]  = 0x03;
        mac_input[10] = 0x03;
        mac_input[11] = (data.len() >> 8) as u8;
        mac_input[12] = data.len() as u8;
        mac_input[13..13 + data.len()].copy_from_slice(data);
        let mac = compute_mac(self.mac_len, &self.client_mac_key, &mac_input[..13 + data.len()]);
        let ml = self.mac_len;

        let mut plain = [0u8; 2048];
        plain[..data.len()].copy_from_slice(data);
        plain[data.len()..data.len() + ml].copy_from_slice(&mac[..ml]);
        let plain_len = data.len() + ml;

        let mut padded = [0u8; 2048];
        let padded_len = tls_pad(&plain[..plain_len], &mut padded);

        let mut iv = [0u8; 16];
        fill_random(&mut iv);

        let mut enc = [0u8; 2048];
        cbc_encrypt(&self.client_key, &iv, &padded[..padded_len], &mut enc);

        let total = 16 + padded_len;
        let mut rec = [0u8; 2100];
        rec[0] = rtype;
        rec[1] = 0x03;
        rec[2] = 0x03;
        rec[3] = (total >> 8) as u8;
        rec[4] = total as u8;
        rec[5..21].copy_from_slice(&iv);
        rec[21..21 + padded_len].copy_from_slice(&enc[..padded_len]);
        self.tcp.send(&rec[..5 + total]);
        self.client_seq += 1;
    }

    fn decrypt_record(&mut self, data_off: usize, data_len: usize) -> Option<usize> {
        let ml = self.mac_len;
        if data_len < 16 + ml {
            return None;
        }

        let rtype = self.raw[0];
        let iv: [u8; 16] = self.raw[data_off..data_off + 16].try_into().ok()?;
        let cipher = &self.raw[data_off + 16..data_off + data_len];

        let mut plain = [0u8; 17408];
        cbc_decrypt(&self.server_key, &iv, cipher, &mut plain);
        let unpadded = tls_unpad(&plain[..cipher.len()])?;

        if unpadded.len() < ml { return None; }

        let plain_data = &unpadded[..unpadded.len() - ml];
        let mac_got    = &unpadded[unpadded.len() - ml..];

        let mut mac_input = [0u8; 17408];
        mac_input[0..8].copy_from_slice(&self.server_seq.to_be_bytes());
        mac_input[8]  = rtype;
        mac_input[9]  = 0x03;
        mac_input[10] = 0x03;
        mac_input[11] = (plain_data.len() >> 8) as u8;
        mac_input[12] = plain_data.len() as u8;
        mac_input[13..13 + plain_data.len()].copy_from_slice(plain_data);

        let mac_exp = compute_mac(ml, &self.server_mac_key, &mac_input[..13 + plain_data.len()]);
        if mac_got != &mac_exp[..ml] {
            crate::log_err!("tls: decrypt_record: bad MAC (server_seq={}, cipher=0x{:04X})",
                self.server_seq, self.selected_cipher);
            return None;
        }

        self.server_seq += 1;

        let copy = plain_data.len().min(self.rx_buf.len() - self.rx_len);
        self.rx_buf[self.rx_len..self.rx_len + copy].copy_from_slice(&plain_data[..copy]);
        self.rx_len += copy;
        Some(copy)
    }

    fn derive_keys(&mut self, premaster: &[u8; 48]) {
        let mut seed = [0u8; 64];
        seed[..32].copy_from_slice(&self.client_random);
        seed[32..].copy_from_slice(&self.server_random);
        prf_sha256(premaster, b"master secret", &seed, &mut self.master_secret);

        let mut seed2 = [0u8; 64];
        seed2[..32].copy_from_slice(&self.server_random);
        seed2[32..].copy_from_slice(&self.client_random);

        let ml = self.mac_len;
        let kb_need = ml * 2 + 32;

        let mut kb = [0u8; 128];
        prf_sha256(&self.master_secret, b"key expansion", &seed2, &mut kb[..kb_need]);

        self.client_mac_key[..ml].copy_from_slice(&kb[..ml]);
        self.server_mac_key[..ml].copy_from_slice(&kb[ml..ml * 2]);
        self.client_key.copy_from_slice(&kb[ml * 2..ml * 2 + 16]);
        self.server_key.copy_from_slice(&kb[ml * 2 + 16..ml * 2 + 32]);
    }

    fn finished_verify(master: &[u8; 48], label: &[u8], hs_hash: &[u8; 32]) -> [u8; 12] {
        let mut out = [0u8; 12];
        prf_sha256(master, label, hs_hash, &mut out);
        out
    }

    pub fn connect(host: &str, ip: [u8; 4], port: u16) -> Option<Self> {
        let tcp = TcpSocket::connect(ip, port)?;
        let mut stream = TlsStream {
            tcp,
            client_random:   [0u8; 32],
            server_random:   [0u8; 32],
            master_secret:   [0u8; 48],
            mac_len:         20,
            selected_cipher: 0x002F,
            client_mac_key:  [0u8; 32],
            server_mac_key:  [0u8; 32],
            client_key:      [0u8; 16],
            server_key:      [0u8; 16],
            client_seq:      0,
            server_seq:      0,
            cipher_active:   false,
            raw:             [0u8; 17408],
            raw_len:         0,
            hs_buf:          [0u8; 16384],
            hs_len:          0,
            rx_buf:          [0u8; 8192],
            rx_len:          0,
        };
        stream.do_handshake(host)?;
        Some(stream)
    }

    fn do_handshake(&mut self, host: &str) -> Option<()> {
        let mut hs_hash = Sha256State::new();

        let unix_time = (crate::vfs::procfs::uptime_ticks() / 18) as u32;
        self.client_random[0..4].copy_from_slice(&unix_time.to_be_bytes());
        fill_random(&mut self.client_random[4..]);

        let sni_bytes = host.as_bytes();
        let mut ch_body = [0u8; 600];
        let mut p = 0usize;

        ch_body[p..p+2].copy_from_slice(&TLS12);                      p += 2;
        ch_body[p..p+32].copy_from_slice(&self.client_random);        p += 32;
        ch_body[p] = 0;                                               p += 1;

        ch_body[p..p+2].copy_from_slice(&[0, 6]);                     p += 2;
        ch_body[p..p+2].copy_from_slice(&CS_RSA_AES128_SHA256);       p += 2;
        ch_body[p..p+2].copy_from_slice(&CS_RSA_AES128_SHA);          p += 2;
        ch_body[p..p+2].copy_from_slice(&[0x00, 0xFF]);               p += 2;

        ch_body[p..p+2].copy_from_slice(&[1, 0]);                     p += 2;

        let sni_ext_len  = sni_bytes.len() + 9;
        let sig_ext_len  = 8;
        let total_ext_len = sni_ext_len + sig_ext_len;

        ch_body[p..p+2].copy_from_slice(&(total_ext_len as u16).to_be_bytes()); p += 2;

        ch_body[p..p+2].copy_from_slice(&[0, 0]);                             p += 2;
        ch_body[p..p+2].copy_from_slice(&((sni_bytes.len() + 5) as u16).to_be_bytes()); p += 2;
        ch_body[p..p+2].copy_from_slice(&((sni_bytes.len() + 3) as u16).to_be_bytes()); p += 2;
        ch_body[p] = 0; p += 1;
        ch_body[p..p+2].copy_from_slice(&(sni_bytes.len() as u16).to_be_bytes()); p += 2;
        ch_body[p..p + sni_bytes.len()].copy_from_slice(sni_bytes); p += sni_bytes.len();

        ch_body[p..p+2].copy_from_slice(&[0, 13]); p += 2;
        ch_body[p..p+2].copy_from_slice(&[0, 4]);  p += 2;
        ch_body[p..p+2].copy_from_slice(&[0, 2]);  p += 2;
        ch_body[p..p+2].copy_from_slice(&[4, 1]);  p += 2;

        let mut hs_msg = [0u8; 700];
        let hs_len = make_handshake(HT_CLIENT_HELLO, &ch_body[..p], &mut hs_msg);
        hs_hash.update(&hs_msg[..hs_len]);
        self.send_record(RT_HANDSHAKE, &hs_msg[..hs_len]);
        crate::log!("tls: ClientHello sent (SNI={}, suites=[0x003C, 0x002F])", host);

        let mut server_cert = [0u8; 8192];
        let mut cert_len    = 0usize;
        let mut got_shd     = false;

        'server_hello: for _ in 0..50_000 {
            if CTRL_C.load(Ordering::SeqCst) { return None; }

            if !self.tcp_fill(5, 2000) {
                if self.tcp.peer_closed {
                    crate::log_err!("tls: server closed TCP during ServerHello");
                    return None;
                }
                continue;
            }

            let rtype   = self.raw[0];
            let rec_len = u16::from_be_bytes([self.raw[3], self.raw[4]]) as usize;

            if !self.tcp_fill(5 + rec_len, 2000) {
                if self.tcp.peer_closed {
                    crate::log_err!("tls: server closed TCP mid-record");
                    return None;
                }
                continue;
            }

            if rtype == RT_ALERT {
                let level = self.raw[5];
                let desc  = self.raw[6];
                crate::log_err!("tls: Alert from server: level={} code={} ({})",
                    level, desc, alert_desc(desc));
                return None;
            }

            if rtype == RT_HANDSHAKE {
                if self.hs_len + rec_len <= self.hs_buf.len() {
                    self.hs_buf[self.hs_len..self.hs_len + rec_len].copy_from_slice(&self.raw[5..5 + rec_len]);
                    self.hs_len += rec_len;
                } else {
                    crate::log_err!("tls: handshake buffer overflow");
                    return None;
                }

                let mut hp = 0;
                while hp + 4 <= self.hs_len {
                    let htype = self.hs_buf[hp];
                    let hlen  = ((self.hs_buf[hp + 1] as usize) << 16)
                              | ((self.hs_buf[hp + 2] as usize) << 8)
                              |  (self.hs_buf[hp + 3] as usize);

                    if hp + 4 + hlen > self.hs_len { break; }

                    let hmsg = &self.hs_buf[hp..hp + 4 + hlen];
                    hs_hash.update(hmsg);

                    match htype {
                        HT_SERVER_HELLO => {
                            if hlen >= 38 {
                                self.server_random.copy_from_slice(&hmsg[6..38]);
                                let sid_len = hmsg[38] as usize;
                                let cs_off  = 38 + 1 + sid_len;
                                if cs_off + 2 <= hmsg.len() {
                                    let cs = u16::from_be_bytes([hmsg[cs_off], hmsg[cs_off + 1]]);
                                    self.selected_cipher = cs;
                                    self.mac_len = if cs == 0x003C { 32 } else { 20 };
                                    crate::log!("tls: ServerHello: cipher=0x{:04X} mac_len={}", cs, self.mac_len);
                                }
                            }
                        }
                        HT_CERTIFICATE => {
                            if hlen > 6 {
                                let c_len = ((hmsg[7] as usize) << 16)
                                          | ((hmsg[8] as usize) << 8)
                                          |  (hmsg[9] as usize);
                                cert_len = c_len.min(8192).min(hmsg.len().saturating_sub(10));
                                if cert_len > 0 {
                                    server_cert[..cert_len].copy_from_slice(&hmsg[10..10 + cert_len]);
                                    crate::log!("tls: Certificate received ({} bytes)", cert_len);
                                }
                            }
                        }
                        HT_SERVER_HELLO_DONE => {
                            crate::log!("tls: ServerHelloDone received");
                            got_shd = true;
                        }
                        _ => {}
                    }
                    hp += 4 + hlen;
                }
                if hp > 0 {
                    self.hs_buf.copy_within(hp..self.hs_len, 0);
                    self.hs_len -= hp;
                }
            }

            self.consume(5 + rec_len);
            if got_shd { break 'server_hello; }
        }

        if cert_len == 0 {
            crate::log_err!("tls: no certificate received");
            return None;
        }

        let rsa_key = parse_rsa_public_key(&server_cert[..cert_len])?;
        crate::log!("tls: RSA key parsed ({} byte modulus)", rsa_key.n_len);

        let mut premaster = [0u8; 48];
        premaster[0..2].copy_from_slice(&TLS12);
        fill_random(&mut premaster[2..]);

        let mut enc_pm = [0u8; 256];
        let enc_len = rsa_pkcs1_encrypt(&rsa_key, &premaster, &mut enc_pm);
        if enc_len == 0 {
            crate::log_err!("tls: RSA encrypt failed");
            return None;
        }

        let mut cke_body = [0u8; 260];
        cke_body[0..2].copy_from_slice(&(enc_len as u16).to_be_bytes());
        cke_body[2..2 + enc_len].copy_from_slice(&enc_pm[..enc_len]);

        let mut hs_cke = [0u8; 300];
        let hs_cke_len = make_handshake(HT_CLIENT_KEY_EXCHANGE, &cke_body[..2 + enc_len], &mut hs_cke);
        hs_hash.update(&hs_cke[..hs_cke_len]);
        self.send_record(RT_HANDSHAKE, &hs_cke[..hs_cke_len]);

        self.derive_keys(&premaster);

        self.send_record(RT_CHANGE_CIPHER_SPEC, &[1]);
        self.cipher_active = true;

        let hs_digest = hs_hash.clone_finalize();
        let vd = Self::finished_verify(&self.master_secret, b"client finished", &hs_digest);
        let mut fin_body = [0u8; 20];
        let fin_len = make_handshake(HT_FINISHED, &vd, &mut fin_body);
        self.send_record(RT_HANDSHAKE, &fin_body[..fin_len]);
        crate::log!("tls: ClientFinished sent");

        let mut got_ccs = false;
        let mut got_fin = false;

        'server_fin: for _ in 0..50_000 {
            if CTRL_C.load(Ordering::SeqCst) { return None; }

            if !self.tcp_fill(5, 2000) {
                if self.tcp.peer_closed {
                    crate::log_err!("tls: server closed TCP before Finished");
                    break 'server_fin;
                }
                continue;
            }

            let rtype   = self.raw[0];
            let rec_len = u16::from_be_bytes([self.raw[3], self.raw[4]]) as usize;

            if !self.tcp_fill(5 + rec_len, 2000) {
                if self.tcp.peer_closed { break 'server_fin; }
                continue;
            }

            if rtype == RT_ALERT {
                if got_ccs {
                    if let Some(dec_len) = self.decrypt_record(5, rec_len) {
                        let alert_off = self.rx_len - dec_len;
                        let level = self.rx_buf[alert_off];
                        let desc  = self.rx_buf[alert_off + 1];
                        self.rx_len -= dec_len;
                        crate::log_err!("tls: Alert during Finished (enc): {} (level {})",
                            alert_desc(desc), level);
                    }
                } else {
                    let level = self.raw[5];
                    let desc  = self.raw[6];
                    crate::log_err!("tls: Alert during Finished: level={} code={} ({})",
                        level, desc, alert_desc(desc));
                }
                self.consume(5 + rec_len);
                break 'server_fin;
            }

            if rtype == RT_CHANGE_CIPHER_SPEC {
                crate::log!("tls: Server ChangeCipherSpec received");
                got_ccs = true;
            }

            if rtype == RT_HANDSHAKE && got_ccs {
                if self.decrypt_record(5, rec_len).is_some() {
                    crate::log!("tls: ServerFinished decrypted OK");
                    got_fin = true;
                } else {
                    crate::log_err!("tls: ServerFinished decrypt failed");
                }
            }

            self.consume(5 + rec_len);
            if got_fin { break 'server_fin; }
        }

        if got_fin {
            crate::log!("tls: handshake complete (cipher=0x{:04X})", self.selected_cipher);
            Some(())
        } else {
            crate::log_err!("tls: handshake failed (got_ccs={} got_fin={})", got_ccs, got_fin);
            None
        }
    }

    pub fn send(&mut self, data: &[u8]) -> bool {
        const CHUNK: usize = 1024;
        let mut off = 0;
        while off < data.len() {
            if CTRL_C.load(Ordering::SeqCst) { return false; }
            let end = (off + CHUNK).min(data.len());
            self.send_record(RT_APP_DATA, &data[off..end]);
            off = end;
        }
        true
    }

    pub fn recv_all(&mut self, timeout_iters: usize) -> &[u8] {
        let mut idle = 0usize;
        loop {
            if CTRL_C.load(Ordering::SeqCst) { break; }

            if !self.tcp_fill(5, 100) {
                if self.tcp.peer_closed { break; }
                idle += 1;
                if idle > timeout_iters { break; }
                continue;
            }

            let rtype   = self.raw[0];
            let rec_len = u16::from_be_bytes([self.raw[3], self.raw[4]]) as usize;

            if !self.tcp_fill(5 + rec_len, 1000) { break; }

            idle = 0;

            match rtype {
                RT_APP_DATA | RT_ALERT => {
                    if let Some(dec_len) = self.decrypt_record(5, rec_len) {
                        if rtype == RT_ALERT {
                            let alert_off = self.rx_len - dec_len;
                            let level = self.rx_buf[alert_off];
                            let desc  = self.rx_buf[alert_off + 1];
                            self.rx_len -= dec_len;
                            if desc != ALERT_CLOSE_NOTIFY {
                                crate::log_err!("tls: recv_all Alert: {} (level {})",
                                    alert_desc(desc), level);
                            }
                            break;
                        }
                    } else {
                        break;
                    }
                }
                _ => {}
            }

            self.consume(5 + rec_len);
        }
        &self.rx_buf[..self.rx_len]
    }

    pub fn close(&mut self) {
        self.send_record(RT_ALERT, &[1, ALERT_CLOSE_NOTIFY]);
        self.tcp.close();
    }

    pub fn clear_rx(&mut self) {
        self.rx_len = 0;
    }

    pub fn cipher_name(&self) -> &'static str {
        match self.selected_cipher {
            0x003C => "TLS_RSA_WITH_AES_128_CBC_SHA256",
            0x002F => "TLS_RSA_WITH_AES_128_CBC_SHA",
            _      => "unknown",
        }
    }
}
