#[cfg(test)]
mod tests {
    use parts::engine::Engine;
    use parts::value::Value;

    #[test]
    fn test_bytes_creation_and_indexing() {
        let engine = Engine::new();
        let source = "
            let std = import `@std/std.pts`;
            let buf = std.bytes.new(4).unwrap();
            
            buf[0] = 10;
            buf[1] = 20;
            buf[2] = 30;
            buf[3] = 40;

            let b0 = buf[0];
            let b1 = buf[1];
            let b2 = buf[2];
            let b3 = buf[3];

            return b0 + b1 + b2 + b3;
        ";
        let result = engine.execute(source).unwrap();
        // 10 + 20 + 30 + 40 = 100
        assert_eq!(result, Some(Value::Int(100)));
    }

    #[test]
    fn test_bytes_from_string_and_to_string() {
        let engine = Engine::new();
        let source = "
            let std = import `@std/std.pts`;
            let text = \"Hello, World!\";
            let buf = std.bytes.from_string(text);
            
            let buf_len = buf.len();
            let first_byte = buf[0];
            let recovered = buf.to_string().unwrap();

            if (buf_len == 13) & (first_byte == 72) & (recovered == text) {
                return 1;
            } else {
                return 0;
            }
        ";
        let result = engine.execute(source).unwrap();
        assert_eq!(result, Some(Value::Int(1)));
    }

    #[test]
    fn test_bytes_from_hex_and_to_hex() {
        let engine = Engine::new();
        let source = "
            let std = import `@std/std.pts`;
            let hex_in = \"deadbeef\";
            let buf = std.bytes.from_hex(hex_in).unwrap();
            
            let b0 = buf[0];
            let b1 = buf[1];
            let b2 = buf[2];
            let b3 = buf[3];
            let hex_out = buf.to_hex();

            if (b0 == 222) & (b1 == 173) & (b2 == 190) & (b3 == 239) & (hex_out == \"deadbeef\") {
                return 1;
            } else {
                return 0;
            }
        ";
        let result = engine.execute(source).unwrap();
        assert_eq!(result, Some(Value::Int(1)));
    }

    #[test]
    fn test_bytes_push_and_extend() {
        let engine = Engine::new();
        let source = "
            let std = import `@std/std.pts`;
            let buf = std.bytes.new(0).unwrap();
            
            buf.push(1);
            buf.push(2);
            buf.push(3);

            let other = std.bytes.from_string(\"ABC\");
            buf.extend(other);

            let buf_len = buf.len();
            let b0 = buf[0];
            let b3 = buf[3];
            let b4 = buf[4];
            let b5 = buf[5];

            if (buf_len == 6) & (b0 == 1) & (b3 == 65) & (b4 == 66) & (b5 == 67) {
                return 1;
            } else {
                return 0;
            }
        ";
        let result = engine.execute(source).unwrap();
        assert_eq!(result, Some(Value::Int(1)));
    }

    #[test]
    fn test_bytes_slice_and_fill() {
        let engine = Engine::new();
        let source = "
            let std = import `@std/std.pts`;
            let buf = std.bytes.new(8).unwrap();
            buf.fill(255);

            let sub = buf.slice(2, 6).unwrap();
            let sub_len = sub.len();
            let sub_b0 = sub[0];

            if (sub_len == 4) & (sub_b0 == 255) {
                return 1;
            } else {
                return 0;
            }
        ";
        let result = engine.execute(source).unwrap();
        assert_eq!(result, Some(Value::Int(1)));
    }

    #[test]
    fn test_network_protocol_u16_be_le() {
        let engine = Engine::new();
        let source = "
            let std = import `@std/std.pts`;
            let buf = std.bytes.new(4).unwrap();
            
            \"Port 8080 (0x1F90) in Network Byte Order (Big Endian)\"
            buf.set_u16_be(0, 8080).unwrap();
            \"Port 8080 in Little Endian\"
            buf.set_u16_le(2, 8080).unwrap();

            let byte_0 = buf[0];
            let byte_1 = buf[1];
            let byte_2 = buf[2];
            let byte_3 = buf[3];

            let read_be = buf.get_u16_be(0).unwrap();
            let read_le = buf.get_u16_le(2).unwrap();

            if (byte_0 == 31) & (byte_1 == 144) & (byte_2 == 144) & (byte_3 == 31) & (read_be == 8080) & (read_le == 8080) {
                return 1;
            } else {
                return 0;
            }
        ";
        let result = engine.execute(source).unwrap();
        assert_eq!(result, Some(Value::Int(1)));
    }

    #[test]
    fn test_network_protocol_u32_be_le() {
        let engine = Engine::new();
        let source = "
            let std = import `@std/std.pts`;
            let buf = std.bytes.new(8).unwrap();
            
            \"0x12345678 in Big Endian and Little Endian\"
            buf.set_u32_be(0, 305419896).unwrap();
            buf.set_u32_le(4, 305419896).unwrap();

            let val_be = buf.get_u32_be(0).unwrap();
            let val_le = buf.get_u32_le(4).unwrap();

            let b0 = buf[0];
            let b3 = buf[3];
            let b4 = buf[4];
            let b7 = buf[7];

            if (val_be == 305419896) & (val_le == 305419896) & (b0 == 18) & (b3 == 120) & (b4 == 120) & (b7 == 18) {
                return 1;
            } else {
                return 0;
            }
        ";
        let result = engine.execute(source).unwrap();
        assert_eq!(result, Some(Value::Int(1)));
    }

    #[test]
    fn test_network_protocol_u64_be_le() {
        let engine = Engine::new();
        let source = "
            let std = import `@std/std.pts`;
            let buf = std.bytes.new(16).unwrap();
            
            let timestamp = 1700000000000;
            buf.set_u64_be(0, timestamp).unwrap();
            buf.set_u64_le(8, timestamp).unwrap();

            let read_be = buf.get_u64_be(0).unwrap();
            let read_le = buf.get_u64_le(8).unwrap();

            if (read_be == timestamp) & (read_le == timestamp) {
                return 1;
            } else {
                return 0;
            }
        ";
        let result = engine.execute(source).unwrap();
        assert_eq!(result, Some(Value::Int(1)));
    }

    #[test]
    fn test_binary_packet_encoder_decoder() {
        let engine = Engine::new();
        let source = "
            let std = import `@std/std.pts`;

            let encode_packet(msg_type, payload_str) {
                let payload_bytes = std.bytes.from_string(payload_str);
                let payload_len = payload_bytes.len();
                let packet = std.bytes.new(8 + payload_len).unwrap();

                \"Magic 0x5054\"
                packet.set_u16_be(0, 20564).unwrap();
                \"Msg type\"
                packet.set_u16_be(2, msg_type).unwrap();
                \"Payload length\"
                packet.set_u32_be(4, payload_len).unwrap();
                \"Copy payload\"
                let i = 0;
                for i < payload_len {
                    packet[8 + i] = payload_bytes[i];
                    i = i + 1;
                }
                return packet;
            }

            let decode_packet(packet) {
                let magic = packet.get_u16_be(0).unwrap();
                let msg_type = packet.get_u16_be(2).unwrap();
                let payload_len = packet.get_u32_be(4).unwrap();
                let payload_bytes = packet.slice(8, 8 + payload_len).unwrap();
                let payload_text = payload_bytes.to_string().unwrap();

                return |>
                    magic: magic,
                    msg_type: msg_type,
                    payload_len: payload_len,
                    payload: payload_text
                <|;
            }

            let pkt = encode_packet(101, \"PING_OK\");
            let decoded = decode_packet(pkt);

            let check_magic = decoded.magic == 20564;
            let check_type = decoded.msg_type == 101;
            let check_len = decoded.payload_len == 7;
            let check_payload = decoded.payload == \"PING_OK\";

            if check_magic & check_type & check_len & check_payload {
                return 1;
            } else {
                return 0;
            }
        ";
        let result = engine.execute(source).unwrap();
        assert_eq!(result, Some(Value::Int(1)));
    }

    #[test]
    fn test_bytes_out_of_bounds_result_error() {
        let engine = Engine::new();
        let source = "
            let std = import `@std/std.pts`;
            let buf = std.bytes.new(2).unwrap();
            
            let read_res = buf.get_u32_be(0);
            let write_res = buf.set_u32_be(0, 100);

            if read_res.is_err() & write_res.is_err() {
                return 1;
            } else {
                return 0;
            }
        ";
        let result = engine.execute(source).unwrap();
        assert_eq!(result, Some(Value::Int(1)));
    }

    #[test]
    fn test_bytes_display_and_serialization() {
        let b = Value::bytes(vec![1, 2, 3, 4, 5]);
        assert_eq!(format!("{}", b), "<bytes: 5 bytes>");

        let mut buffer = Vec::new();
        b.encode(&mut buffer);

        let (decoded, _) = Value::decode(&buffer, true, 0);
        assert_eq!(decoded.len(), 1);
        assert_eq!(decoded[0], b);
    }
}
