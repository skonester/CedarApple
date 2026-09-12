#[cfg(test)]
mod tests {
    use parts::engine::Engine;
    use parts::value::Value;

    #[test]
    fn test_ws_unmasked_text_frame() {
        let engine = Engine::new();
        let source = "
            let std = import `@std/std.pts`;
            let ws = std.ws;

            let frame = ws.text_frame(\"Hello WebSocket!\", false, std.bytes.new(0).unwrap());
            let encoded_res = ws.encode_frame(frame);
            let encoded_bytes = encoded_res.unwrap();

            let decoded_res = ws.decode_frame(encoded_bytes, 0);
            let decoded = decoded_res.unwrap();
            let d_frame = decoded.frame;
            let payload_text = d_frame.payload.to_string().unwrap();

            let check_fin = d_frame.fin;
            let check_opcode = d_frame.opcode == ws.OP_TEXT;
            let check_masked = (d_frame.masked == false);
            let check_payload = payload_text == \"Hello WebSocket!\";

            if check_fin & check_opcode & check_masked & check_payload {
                return 1;
            } else {
                return 0;
            }
        ";
        let result = engine.execute(source).unwrap();
        assert_eq!(result, Some(Value::Int(1)));
    }

    #[test]
    fn test_ws_masked_text_frame_client_to_server() {
        let engine = Engine::new();
        let source = "
            let std = import `@std/std.pts`;
            let ws = std.ws;

            let mask_key = std.bytes.new(4).unwrap();
            mask_key[0] = 18;
            mask_key[1] = 52;
            mask_key[2] = 86;
            mask_key[3] = 120;

            let frame = ws.text_frame(\"Hello from client!\", true, mask_key);
            let encoded_bytes = ws.encode_frame(frame).unwrap();

            let decoded = ws.decode_frame(encoded_bytes, 0).unwrap();
            let d_frame = decoded.frame;
            let payload_text = d_frame.payload.to_string().unwrap();

            let check_fin = d_frame.fin;
            let check_opcode = d_frame.opcode == ws.OP_TEXT;
            let check_masked = d_frame.masked;
            let check_payload = payload_text == \"Hello from client!\";

            if check_fin & check_opcode & check_masked & check_payload {
                return 1;
            } else {
                return 0;
            }
        ";
        let result = engine.execute(source).unwrap();
        assert_eq!(result, Some(Value::Int(1)));
    }

    #[test]
    fn test_ws_binary_frame() {
        let engine = Engine::new();
        let source = "
            let std = import `@std/std.pts`;
            let ws = std.ws;

            let payload = std.bytes.new(4).unwrap();
            payload.set_u16_be(0, 8080).unwrap();
            payload.set_u16_be(2, 9090).unwrap();

            let frame = ws.binary_frame(payload, false, std.bytes.new(0).unwrap());
            let encoded_bytes = ws.encode_frame(frame).unwrap();

            let decoded = ws.decode_frame(encoded_bytes, 0).unwrap();
            let d_frame = decoded.frame;

            let p0 = d_frame.payload.get_u16_be(0).unwrap();
            let p1 = d_frame.payload.get_u16_be(2).unwrap();

            if (d_frame.opcode == ws.OP_BINARY) & (p0 == 8080) & (p1 == 9090) {
                return 1;
            } else {
                return 0;
            }
        ";
        let result = engine.execute(source).unwrap();
        assert_eq!(result, Some(Value::Int(1)));
    }

    #[test]
    fn test_ws_medium_extended_length_frame() {
        // Frame with 256 bytes payload (uses 16-bit extended length indicator 126)
        let engine = Engine::new();
        let source = "
            let std = import `@std/std.pts`;
            let ws = std.ws;

            let payload = std.bytes.new(256).unwrap();
            payload.fill(65);

            let frame = ws.binary_frame(payload, false, std.bytes.new(0).unwrap());
            let encoded_bytes = ws.encode_frame(frame).unwrap();

            let decoded = ws.decode_frame(encoded_bytes, 0).unwrap();
            let d_frame = decoded.frame;
            let d_len = d_frame.payload.len();
            let first_byte = d_frame.payload[0];
            let last_byte = d_frame.payload[255];

            if (d_len == 256) & (first_byte == 65) & (last_byte == 65) {
                return 1;
            } else {
                return 0;
            }
        ";
        let result = engine.execute(source).unwrap();
        assert_eq!(result, Some(Value::Int(1)));
    }

    #[test]
    fn test_ws_ping_pong_control_frames() {
        let engine = Engine::new();
        let source = "
            let std = import `@std/std.pts`;
            let ws = std.ws;

            let ping_data = std.bytes.from_string(\"heartbeat\");
            let ping = ws.ping_frame(ping_data, false, std.bytes.new(0).unwrap());
            let enc_ping = ws.encode_frame(ping).unwrap();
            let dec_ping = ws.decode_frame(enc_ping, 0).unwrap();

            let is_ping = dec_ping.frame.opcode == ws.OP_PING;
            let ping_body = dec_ping.frame.payload.to_string().unwrap();

            let pong_data = std.bytes.from_string(\"heartbeat\");
            let pong = ws.pong_frame(pong_data, false, std.bytes.new(0).unwrap());
            let enc_pong = ws.encode_frame(pong).unwrap();
            let dec_pong = ws.decode_frame(enc_pong, 0).unwrap();

            let is_pong = dec_pong.frame.opcode == ws.OP_PONG;
            let pong_body = dec_pong.frame.payload.to_string().unwrap();

            if is_ping & (ping_body == \"heartbeat\") & is_pong & (pong_body == \"heartbeat\") {
                return 1;
            } else {
                return 0;
            }
        ";
        let result = engine.execute(source).unwrap();
        assert_eq!(result, Some(Value::Int(1)));
    }

    #[test]
    fn test_ws_close_frame_with_code_and_reason() {
        let engine = Engine::new();
        let source = "
            let std = import `@std/std.pts`;
            let ws = std.ws;

            let frame = ws.close_frame(ws.CLOSE_NORMAL, \"session ended\", false, std.bytes.new(0).unwrap());
            let enc_bytes = ws.encode_frame(frame).unwrap();

            let decoded = ws.decode_frame(enc_bytes, 0).unwrap();
            let is_close = decoded.frame.opcode == ws.OP_CLOSE;

            let close_info = ws.parse_close_payload(decoded.frame.payload);
            let code_match = close_info.code == ws.CLOSE_NORMAL;
            let reason_match = close_info.reason == \"session ended\";

            if is_close & code_match & reason_match {
                return 1;
            } else {
                return 0;
            }
        ";
        let result = engine.execute(source).unwrap();
        assert_eq!(result, Some(Value::Int(1)));
    }

    #[test]
    fn test_ws_protocol_violations_rejected() {
        // RFC 6455 Section 5.5: Control frames must not be fragmented and payload <= 125
        let engine = Engine::new();
        let source = "
            let std = import `@std/std.pts`;
            let ws = std.ws;

            \"1. Fragmented control frame should fail encoding\"
            let frag_ping = ws.create_frame(false, ws.OP_PING, std.bytes.new(4).unwrap(), false, std.bytes.new(0).unwrap());
            let frag_res = ws.encode_frame(frag_ping);

            \"2. Oversized control frame should fail encoding\"
            let big_ping = ws.create_frame(true, ws.OP_PING, std.bytes.new(126).unwrap(), false, std.bytes.new(0).unwrap());
            let big_res = ws.encode_frame(big_ping);

            if frag_res.is_err() & big_res.is_err() {
                return 1;
            } else {
                return 0;
            }
        ";
        let result = engine.execute(source).unwrap();
        assert_eq!(result, Some(Value::Int(1)));
    }

    #[test]
    fn test_ws_sequential_multi_frame_stream() {
        // Decodes multiple frames sequentially from a single byte stream
        let engine = Engine::new();
        let source = "
            let std = import `@std/std.pts`;
            let ws = std.ws;

            let f1 = ws.text_frame(\"Frame1\", false, std.bytes.new(0).unwrap());
            let f2 = ws.text_frame(\"Frame2\", false, std.bytes.new(0).unwrap());

            let b1 = ws.encode_frame(f1).unwrap();
            let b2 = ws.encode_frame(f2).unwrap();

            let stream = std.bytes.new(b1.len() + b2.len()).unwrap();
            let i = 0;
            for i < b1.len() {
                stream[i] = b1[i];
                i = i + 1;
            }
            let j = 0;
            for j < b2.len() {
                stream[b1.len() + j] = b2[j];
                j = j + 1;
            }

            let dec1 = ws.decode_frame(stream, 0).unwrap();
            let t1 = dec1.frame.payload.to_string().unwrap();
            let next_off = dec1.next_offset;

            let dec2 = ws.decode_frame(stream, next_off).unwrap();
            let t2 = dec2.frame.payload.to_string().unwrap();

            if (t1 == \"Frame1\") & (t2 == \"Frame2\") {
                return 1;
            } else {
                return 0;
            }
        ";
        let result = engine.execute(source).unwrap();
        assert_eq!(result, Some(Value::Int(1)));
    }

    #[test]
    fn test_ws_handshake_rfc_6455_verification() {
        // Tests handshake key and verification against RFC 6455 Section 1.3 example key
        let engine = Engine::new();
        let source = "
            let std = import `@std/std.pts`;
            let ws = std.ws;

            let client_key = \"dGhlIHNhbXBsZSBub25jZQ==\";
            let client_req = ws.create_client_handshake(\"server.example.com\", \"/chat\", client_key);
            let server_resp = ws.create_server_handshake(client_key);

            let verified = ws.verify_server_handshake(server_resp, client_key).unwrap();

            if verified {
                return 1;
            } else {
                return 0;
            }
        ";
        let result = engine.execute(source).unwrap();
        assert_eq!(result, Some(Value::Int(1)));
    }

    #[test]
    fn test_ws_match_dispatch_in_parts() {
        // Pattern matches on received frame opcodes in Parts
        let engine = Engine::new();
        let source = "
            let std = import `@std/std.pts`;
            let ws = std.ws;

            let handle_ws_message(frame) {
                return match frame.opcode {
                    1 => 10,
                    2 => 20,
                    8 => 80,
                    9 => 90,
                    10 => 100,
                    _ => 0
                };
            }

            let tf = ws.text_frame(\"test\", false, std.bytes.new(0).unwrap());
            let bf = ws.binary_frame(std.bytes.new(1).unwrap(), false, std.bytes.new(0).unwrap());
            let pf = ws.ping_frame(std.bytes.new(0).unwrap(), false, std.bytes.new(0).unwrap());
            let r_a = handle_ws_message(tf);
            let r_b = handle_ws_message(bf);
            let r_c = handle_ws_message(pf);

            let term_a = r_a * 100;
            let term_b = r_b * 10;
            return term_a + term_b + r_c;
        ";
        let result = engine.execute(source).unwrap();
        assert_eq!(result, Some(Value::Int(1290)));
    }

    #[test]
    fn test_ws_live_echo_server_wss() {
        let engine = Engine::new();
        let source = "
            let std = import `@std/std.pts`;
            let ws = std.ws;

            \"Connect to live WebSocket server over WSS\"
            let client_res = ws.connect_wss(\"echo.websocket.org\", \"/\");
            let is_connected = client_res.is_ok();
            if (is_connected == false) {
                return 0;
            }
            let client = client_res.unwrap();

            \"Send masked text message to echo server\"
            let mask_key = std.bytes.new(4).unwrap();
            mask_key[0] = 11;
            mask_key[1] = 22;
            mask_key[2] = 33;
            mask_key[3] = 44;

            let msg = \"Hello from Parts WebSocket Client!\";
            let frame = ws.text_frame(msg, true, mask_key);
            let send_res = ws.client_send(client, frame);

            \"Receive echo response\"
            let recv_res = ws.client_recv(client);
            let check_recv = recv_res.is_ok();
            if (check_recv == false) {
                ws.client_close(client);
                return 0;
            }

            let dec_info = recv_res.unwrap();
            let echo_text = dec_info.frame.payload.to_string().unwrap();

            ws.client_close(client);

            \"Verify server response is valid and non-empty\"
            let is_empty = (echo_text == \"\");
            let is_valid_payload = (is_empty == false);

            if is_connected & check_recv & is_valid_payload {
                return 1;
            } else {
                return 0;
            }
        ";
        let result = engine.execute(source).unwrap();
        assert_eq!(result, Some(Value::Int(1)));
    }
}
