extern crate alloc;
use alloc::vec::Vec;
use alloc::string::String;
use core::sync::atomic::Ordering;
use super::CTRL_C;

const CLIENT_PREFACE: &[u8] = b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n";

const FT_DATA:          u8 = 0x0;
const FT_HEADERS:       u8 = 0x1;
const FT_RST_STREAM:    u8 = 0x3;
const FT_SETTINGS:      u8 = 0x4;
const FT_PING:          u8 = 0x6;
const FT_GOAWAY:        u8 = 0x7;
const FT_WINDOW_UPDATE: u8 = 0x8;
const FT_CONTINUATION:  u8 = 0x9;

const FL_END_STREAM:  u8 = 0x1;
const FL_END_HEADERS: u8 = 0x4;
const FL_ACK:         u8 = 0x1;

const SETTINGS_HEADER_TABLE_SIZE:      u16 = 0x1;
const SETTINGS_ENABLE_PUSH:            u16 = 0x2;
const SETTINGS_MAX_CONCURRENT_STREAMS: u16 = 0x3;
const SETTINGS_INITIAL_WINDOW_SIZE:    u16 = 0x4;
const SETTINGS_MAX_FRAME_SIZE:         u16 = 0x5;

const INIT_WINDOW: u32 = 65535;
const MAX_FRAME:   u32 = 16384;

static HPACK_STATIC: &[(&[u8], &[u8])] = &[
    (b":authority",                  b""),
    (b":method",                     b"GET"),
    (b":method",                     b"POST"),
    (b":path",                       b"/"),
    (b":path",                       b"/index.html"),
    (b":scheme",                     b"http"),
    (b":scheme",                     b"https"),
    (b":status",                     b"200"),
    (b":status",                     b"204"),
    (b":status",                     b"206"),
    (b":status",                     b"304"),
    (b":status",                     b"400"),
    (b":status",                     b"404"),
    (b":status",                     b"500"),
    (b"accept-charset",              b""),
    (b"accept-encoding",             b"gzip, deflate"),
    (b"accept-language",             b""),
    (b"accept-ranges",               b""),
    (b"accept",                      b""),
    (b"access-control-allow-origin", b""),
    (b"age",                         b""),
    (b"allow",                       b""),
    (b"authorization",               b""),
    (b"cache-control",               b""),
    (b"content-disposition",         b""),
    (b"content-encoding",            b""),
    (b"content-language",            b""),
    (b"content-length",              b""),
    (b"content-location",            b""),
    (b"content-range",               b""),
    (b"content-type",                b""),
    (b"cookie",                      b""),
    (b"date",                        b""),
    (b"etag",                        b""),
    (b"expect",                      b""),
    (b"expires",                     b""),
    (b"from",                        b""),
    (b"host",                        b""),
    (b"if-match",                    b""),
    (b"if-modified-since",           b""),
    (b"if-none-match",               b""),
    (b"if-range",                    b""),
    (b"if-unmodified-since",         b""),
    (b"last-modified",               b""),
    (b"link",                        b""),
    (b"location",                    b""),
    (b"max-forwards",                b""),
    (b"proxy-authenticate",          b""),
    (b"proxy-authorization",         b""),
    (b"range",                       b""),
    (b"referer",                     b""),
    (b"refresh",                     b""),
    (b"retry-after",                 b""),
    (b"server",                      b""),
    (b"set-cookie",                  b""),
    (b"strict-transport-security",   b""),
    (b"transfer-encoding",           b""),
    (b"user-agent",                  b""),
    (b"vary",                        b""),
    (b"via",                         b""),
    (b"www-authenticate",            b""),
];

fn hpack_static_index(name: &[u8], value: &[u8]) -> Option<usize> {
    HPACK_STATIC.iter().enumerate()
        .find(|(_, &(n, v))| n == name && v == value)
        .map(|(i, _)| i + 1)
}

fn hpack_static_name_index(name: &[u8]) -> Option<usize> {
    HPACK_STATIC.iter().enumerate()
        .find(|(_, &(n, _))| n == name)
        .map(|(i, _)| i + 1)
}

fn hpack_encode_int(prefix_bits: u8, value: usize, buf: &mut Vec<u8>, first_byte_prefix: u8) {
    let max = (1usize << prefix_bits) - 1;
    if value < max {
        buf.push(first_byte_prefix | value as u8);
    } else {
        buf.push(first_byte_prefix | max as u8);
        let mut v = value - max;
        loop {
            if v < 128 { buf.push(v as u8); break; }
            buf.push((v & 0x7F) as u8 | 0x80);
            v >>= 7;
        }
    }
}

fn hpack_encode_literal(buf: &mut Vec<u8>, s: &[u8]) {
    hpack_encode_int(7, s.len(), buf, 0x00);
    buf.extend_from_slice(s);
}

fn hpack_encode_header(buf: &mut Vec<u8>, name: &[u8], value: &[u8]) {
    if let Some(idx) = hpack_static_index(name, value) {
        buf.push(0x80 | idx as u8);
        return;
    }
    if let Some(idx) = hpack_static_name_index(name) {
        hpack_encode_int(4, idx, buf, 0x00);
    } else {
        buf.push(0x00);
        hpack_encode_literal(buf, name);
    }
    hpack_encode_literal(buf, value);
}

pub fn hpack_encode_request(method: &str, scheme: &str, authority: &str, path: &str) -> Vec<u8> {
    let mut out = Vec::new();
    hpack_encode_header(&mut out, b":method",    method.as_bytes());
    hpack_encode_header(&mut out, b":scheme",    scheme.as_bytes());
    hpack_encode_header(&mut out, b":path",      path.as_bytes());
    hpack_encode_header(&mut out, b":authority", authority.as_bytes());
    hpack_encode_header(&mut out, b"user-agent", b"MikuOS/0.1");
    hpack_encode_header(&mut out, b"accept",     b"*/*");
    out
}

fn hpack_decode_int(data: &[u8], pos: &mut usize, prefix_bits: u8) -> usize {
    if *pos >= data.len() { return 0; }
    let mask  = (1u8 << prefix_bits) - 1;
    let first = (data[*pos] & mask) as usize;
    *pos += 1;
    if first < mask as usize { return first; }
    let mut value = first;
    let mut shift = 0usize;
    while *pos < data.len() {
        let b = data[*pos]; *pos += 1;
        value += ((b & 0x7F) as usize) << shift;
        shift += 7;
        if b & 0x80 == 0 { break; }
    }
    value
}

fn hpack_decode_string(data: &[u8], pos: &mut usize) -> Vec<u8> {
    if *pos >= data.len() { return Vec::new(); }
    let huffman = data[*pos] & 0x80 != 0;
    let len     = hpack_decode_int(data, pos, 7);
    if *pos + len > data.len() { return Vec::new(); }
    let s = &data[*pos..*pos + len];
    *pos += len;
    if huffman { huffman_decode(s) } else { Vec::from(s) }
}

fn decode_headers_block(block: &[u8]) -> (u16, Option<String>) {
    let mut pos      = 0usize;
    let mut status   = 0u16;
    let mut location = None;

    while pos < block.len() {
        let b = block[pos];

        let (name, value) = if b & 0x80 != 0 {
            let idx = hpack_decode_int(block, &mut pos, 7);
            if idx == 0 || idx > HPACK_STATIC.len() { continue; }
            (Vec::from(HPACK_STATIC[idx-1].0), Vec::from(HPACK_STATIC[idx-1].1))
        } else if b & 0x40 != 0 {
            let idx = hpack_decode_int(block, &mut pos, 6);
            let n = if idx > 0 && idx <= HPACK_STATIC.len() {
                Vec::from(HPACK_STATIC[idx-1].0)
            } else {
                hpack_decode_string(block, &mut pos)
            };
            let v = hpack_decode_string(block, &mut pos);
            (n, v)
        } else if b & 0x20 != 0 {
            hpack_decode_int(block, &mut pos, 5);
            continue;
        } else {
            let idx = hpack_decode_int(block, &mut pos, 4);
            let n = if idx > 0 && idx <= HPACK_STATIC.len() {
                Vec::from(HPACK_STATIC[idx-1].0)
            } else {
                hpack_decode_string(block, &mut pos)
            };
            let v = hpack_decode_string(block, &mut pos);
            (n, v)
        };

        if name == b":status" {
            status = core::str::from_utf8(&value).ok()
                .and_then(|s| s.parse().ok()).unwrap_or(0);
        } else if name == b"location" {
            if let Ok(s) = core::str::from_utf8(&value) {
                location = Some(String::from(s));
            }
        }
    }
    (status, location)
}

fn make_frame(ft: u8, flags: u8, sid: u32, payload: &[u8], out: &mut Vec<u8>) {
    let len = payload.len();
    out.push((len >> 16) as u8);
    out.push((len >> 8)  as u8);
    out.push(len         as u8);
    out.push(ft);
    out.push(flags);
    out.push(((sid >> 24) & 0x7F) as u8);
    out.push((sid >> 16) as u8);
    out.push((sid >> 8)  as u8);
    out.push(sid         as u8);
    out.extend_from_slice(payload);
}

fn settings_frame(settings: &[(u16, u32)]) -> Vec<u8> {
    let mut payload = Vec::new();
    for &(id, val) in settings {
        payload.push((id  >> 8) as u8); payload.push(id  as u8);
        payload.push((val >> 24) as u8); payload.push((val >> 16) as u8);
        payload.push((val >> 8)  as u8); payload.push(val         as u8);
    }
    let mut out = Vec::new();
    make_frame(FT_SETTINGS, 0, 0, &payload, &mut out);
    out
}

fn settings_ack() -> Vec<u8> {
    let mut out = Vec::new();
    make_frame(FT_SETTINGS, FL_ACK, 0, &[], &mut out);
    out
}

fn window_update(sid: u32, inc: u32) -> Vec<u8> {
    let p = [(( inc >> 24) & 0x7F) as u8, (inc >> 16) as u8, (inc >> 8) as u8, inc as u8];
    let mut out = Vec::new();
    make_frame(FT_WINDOW_UPDATE, 0, sid, &p, &mut out);
    out
}

pub struct H2Response {
    pub status:   u16,
    pub body:     Vec<u8>,
    pub location: Option<String>,
}

pub fn h2_request(
    tls:    &mut super::tls::TlsStream,
    method: &str,
    scheme: &str,
    host:   &str,
    path:   &str,
    body:   Option<&[u8]>,
) -> Option<H2Response> {
    if CTRL_C.load(Ordering::SeqCst) { return None; }

    tls.send(CLIENT_PREFACE);
    tls.send(&settings_frame(&[
        (SETTINGS_HEADER_TABLE_SIZE,      4096),
        (SETTINGS_ENABLE_PUSH,            0),
        (SETTINGS_MAX_CONCURRENT_STREAMS, 100),
        (SETTINGS_INITIAL_WINDOW_SIZE,    INIT_WINDOW),
        (SETTINGS_MAX_FRAME_SIZE,         MAX_FRAME),
    ]));
    tls.send(&window_update(0, 1 << 20));

    let hpack = hpack_encode_request(method, scheme, host, path);
    let es    = if body.is_none() { FL_END_STREAM | FL_END_HEADERS } else { FL_END_HEADERS };
    let mut hf = Vec::new();
    make_frame(FT_HEADERS, es, 1, &hpack, &mut hf);
    tls.send(&hf);

    if let Some(b) = body {
        let mut df = Vec::new();
        make_frame(FT_DATA, FL_END_STREAM, 1, b, &mut df);
        tls.send(&df);
    }

    let mut buf:           Vec<u8>      = Vec::new();
    let mut status         = 0u16;
    let mut body_out:      Vec<u8>      = Vec::new();
    let mut location:      Option<String> = None;
    let mut headers_block: Vec<u8>      = Vec::new();
    let mut got_headers    = false;
    let mut done           = false;
    let mut idle           = 0usize;

    loop {
        if CTRL_C.load(Ordering::SeqCst) { return None; }
        if done { break; }

        let chunk = tls.recv_chunk(500);
        if chunk.is_empty() {
            if tls.is_closed() { break; }
            idle += 1;
            if idle > 200_000 { break; }
            continue;
        }
        idle = 0;
        buf.extend_from_slice(chunk);

        let mut pos = 0usize;
        loop {
            if pos + 9 > buf.len() { break; }
            let len = ((buf[pos] as usize) << 16)
                    | ((buf[pos+1] as usize) << 8)
                    |  (buf[pos+2] as usize);
            if pos + 9 + len > buf.len() { break; }

            let ftype     = buf[pos+3];
            let flags     = buf[pos+4];
            let stream_id = (((buf[pos+5] & 0x7F) as u32) << 24)
                          | ((buf[pos+6] as u32) << 16)
                          | ((buf[pos+7] as u32) << 8)
                          |  (buf[pos+8] as u32);
            let payload   = &buf[pos+9..pos+9+len].to_vec();
            pos          += 9 + len;

            match ftype {
                FT_SETTINGS if flags & FL_ACK == 0 => {
                    tls.send(&settings_ack());
                    crate::log!("h2: SETTINGS ack sent");
                }
                FT_HEADERS if stream_id == 1 => {
                    headers_block.extend_from_slice(payload);
                    if flags & FL_END_HEADERS != 0 {
                        let (s, loc) = decode_headers_block(&headers_block);
                        status       = s;
                        location     = loc;
                        headers_block.clear();
                        got_headers  = true;
                        crate::log!("h2: status={}", status);
                    }
                    if flags & FL_END_STREAM != 0 { done = true; }
                }
                FT_CONTINUATION if stream_id == 1 => {
                    headers_block.extend_from_slice(payload);
                    if flags & FL_END_HEADERS != 0 {
                        let (s, loc) = decode_headers_block(&headers_block);
                        status       = s;
                        location     = loc;
                        headers_block.clear();
                        got_headers  = true;
                    }
                }
                FT_DATA if stream_id == 1 => {
                    if flags & 0x08 != 0 && !payload.is_empty() {
                        let pad = payload[0] as usize;
                        let end = payload.len().saturating_sub(pad);
                        if end > 1 { body_out.extend_from_slice(&payload[1..end]); }
                    } else {
                        body_out.extend_from_slice(payload);
                    }
                    if flags & FL_END_STREAM != 0 { done = true; }
                }
                FT_RST_STREAM => {
                    let code = if payload.len() >= 4 {
                        u32::from_be_bytes([payload[0], payload[1], payload[2], payload[3]])
                    } else { 0 };
                    crate::log_err!("h2: RST_STREAM code={}", code);
                    done = true;
                }
                FT_GOAWAY => {
                    let code = if payload.len() >= 8 {
                        u32::from_be_bytes([payload[4], payload[5], payload[6], payload[7]])
                    } else { 0 };
                    crate::log_err!("h2: GOAWAY code={}", code);
                    done = true;
                }
                FT_PING if flags & FL_ACK == 0 => {
                    let mut pf = Vec::new();
                    make_frame(FT_PING, FL_ACK, 0, payload, &mut pf);
                    tls.send(&pf);
                }
                FT_WINDOW_UPDATE => {}
                _ => {}
            }
        }

        if pos > 0 {
            buf.drain(..pos);
        }
    }

    if !got_headers { return None; }
    Some(H2Response { status, body: body_out, location })
}

fn huffman_decode(src: &[u8]) -> Vec<u8> {
    static TABLE: &[(u32, u8, u8)] = &[
        (0x1ff8,13,b'\x20'),(0x7fffd8,23,b'!'), (0xfffffe2,28,b'"'),(0xfffffe3,28,b'#'),
        (0xfffffe4,28,b'$'),(0xfffffe5,28,b'%'),(0xfffffe6,28,b'&'),(0xfffffe7,28,b'\''),
        (0xfffffe8,28,b'('),(0xfffffe9,28,b')'),(0xffffffea,28,b'*'),(0xffffffeb,28,b'+'),
        (0xfffe6,20,b','),(0x7ffd,15,b'-'),(0x1f,5,b'.'),(0x23,6,b'/'),
        (0xb,4,b'0'),(0xc,4,b'1'),(0xd,4,b'2'),(0xe,4,b'3'),(0xf,4,b'4'),
        (0x10,4,b'5'),(0x11,4,b'6'),(0x12,4,b'7'),(0x13,4,b'8'),(0x14,4,b'9'),
        (0x3a,6,b':'),(0x3b,6,b';'),(0xfffe7,20,b'<'),(0x3c,6,b'='),(0xfffe8,20,b'>'),
        (0x3d,6,b'?'),(0xffffff9,28,b'@'),
        (0x15,5,b'A'),(0xf8,8,b'B'),(0x16,5,b'C'),(0x17,5,b'D'),(0x18,5,b'E'),
        (0xf9,8,b'F'),(0x19,5,b'G'),(0x1a,5,b'H'),(0x1b,5,b'I'),(0xf9,8,b'J'),
        (0x1c,5,b'K'),(0xf8,8,b'L'),(0x1d,5,b'M'),(0x1e,5,b'N'),(0x1f,5,b'O'),
        (0x20,5,b'P'),(0xfa,8,b'Q'),(0x21,5,b'R'),(0x22,5,b'S'),(0x23,5,b'T'),
        (0x24,5,b'U'),(0xfb,8,b'V'),(0x25,5,b'W'),(0xfc,8,b'X'),(0xfd,8,b'Y'),
        (0xfe,8,b'Z'),(0x26,5,b'a'),(0x27,5,b'b'),(0x28,5,b'c'),(0x29,5,b'd'),
        (0x2a,5,b'e'),(0x2b,5,b'f'),(0x2c,5,b'g'),(0x2d,5,b'h'),(0x2e,5,b'i'),
        (0x2f,5,b'j'),(0x30,5,b'k'),(0x31,5,b'l'),(0x32,5,b'm'),(0x33,5,b'n'),
        (0x34,5,b'o'),(0x35,5,b'p'),(0x36,5,b'q'),(0x37,5,b'r'),(0x38,5,b's'),
        (0x39,5,b't'),(0x3a,5,b'u'),(0x3b,5,b'v'),(0x3c,5,b'w'),(0xfe,8,b'x'),
        (0x3d,5,b'y'),(0xff,8,b'z'),
    ];

    let mut bits: u64  = 0;
    let mut nbits: u32 = 0;
    let mut out        = Vec::new();

    for &byte in src {
        bits   = (bits << 8) | byte as u64;
        nbits += 8;
        'outer: loop {
            for &(code, len, ch) in TABLE.iter() {
                if nbits >= len as u32 {
                    let shift = nbits - len as u32;
                    if shift >= 64 { continue; }
                    if (bits >> shift) as u32 == code {
                        out.push(ch);
                        bits  = if shift == 0 { 0 } else { bits & ((1u64 << shift) - 1) };
                        nbits -= len as u32;
                        continue 'outer;
                    }
                }
            }
            break;
        }
    }
    out
}
