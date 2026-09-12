#[cfg(test)]
mod tests {
    use parts::engine::Engine;
    use parts::value::Value;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::os::unix::net::UnixListener;
    use std::thread;

    #[test]
    fn test_tcp_echo_socket_operations() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();

        let server_thread = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0u8; 1024];
            let n = stream.read(&mut buf).unwrap();
            stream.write_all(&buf[..n]).unwrap();
            stream.flush().unwrap();
        });

        let engine = Engine::new();
        let source = format!(
            r#"
            let std = import `@std/std.pts`;
            let net = std.net;

            let addr = "127.0.0.1:{}";
            let tcp_conn = net.tcp_connect;
            let net_write = net.write;
            let net_read = net.read;
            let net_close = net.close;

            let sock_res = tcp_conn(addr);
            if sock_res.is_err() {{
                return 0;
            }}
            let sock = sock_res.unwrap();

            let msg = "Hello from Parts TCP test!";
            let write_res = net_write(sock, msg);
            if write_res.is_err() {{
                net_close(sock);
                return 0;
            }}

            let read_res = net_read(sock, 1024);
            if read_res.is_err() {{
                net_close(sock);
                return 0;
            }}

            let recv_bytes = read_res.unwrap();
            let recv_str = recv_bytes.to_string().unwrap();

            let close_res = net_close(sock);
            if close_res.is_err() {{
                return 0;
            }}

            if recv_str == msg {{
                return 1;
            }} else {{
                return 0;
            }}
            "#,
            port
        );

        let result = engine.execute(&source).unwrap();
        assert_eq!(result, Some(Value::Int(1)));

        server_thread.join().unwrap();
    }

    #[test]
    fn test_tcp_write_bytes_and_multiple_reads() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();

        let server_thread = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0u8; 1024];
            let n = stream.read(&mut buf).unwrap();
            assert_eq!(&buf[..n], &[1, 2, 3, 4, 5]);

            stream.write_all(&[10, 20, 30]).unwrap();
            stream.flush().unwrap();
            stream.write_all(&[40, 50]).unwrap();
            stream.flush().unwrap();
        });

        let engine = Engine::new();
        let source = format!(
            r#"
            let std = import `@std/std.pts`;
            let net = std.net;

            let tcp_conn = net.tcp_connect;
            let net_write = net.write;
            let net_read = net.read;
            let net_close = net.close;

            let addr = "127.0.0.1:{}";
            let sock_res = tcp_conn(addr);
            let sock = sock_res.unwrap();

            let b = std.bytes.new(5).unwrap();
            b[0] = 1;
            b[1] = 2;
            b[2] = 3;
            b[3] = 4;
            b[4] = 5;

            let write_res = net_write(sock, b);
            let bytes_written = write_res.unwrap();

            let r1 = net_read(sock, 3).unwrap();
            let r2 = net_read(sock, 2).unwrap();

            net_close(sock);

            let check_len = (bytes_written == 5) & (r1.len() == 3) & (r2.len() == 2);
            let check_r1 = (r1[0] == 10) & (r1[1] == 20) & (r1[2] == 30);
            let check_r2 = (r2[0] == 40) & (r2[1] == 50);

            if check_len & check_r1 & check_r2 {{
                return 1;
            }} else {{
                return 0;
            }}
            "#,
            port
        );

        let result = engine.execute(&source).unwrap();
        assert_eq!(result, Some(Value::Int(1)));

        server_thread.join().unwrap();
    }

    #[test]
    fn test_tcp_invalid_connection_returns_err() {
        let engine = Engine::new();
        let source = r#"
            let std = import `@std/std.pts`;
            let net = std.net;
            let tcp_conn = net.tcp_connect;
            let net_close = net.close;

            "Connect to a non-existent port on localhost"
            let sock_res = tcp_conn("127.0.0.1:1");
            if sock_res.is_err() {
                return 1;
            } else {
                net_close(sock_res.unwrap());
                return 0;
            }
        "#;

        let result = engine.execute(source).unwrap();
        assert_eq!(result, Some(Value::Int(1)));
    }

    #[test]
    fn test_tcp_operations_on_closed_socket_return_err() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();

        let server_thread = thread::spawn(move || {
            let (_stream, _) = listener.accept().unwrap();
        });

        let engine = Engine::new();
        let source = format!(
            r#"
            let std = import `@std/std.pts`;
            let net = std.net;
            let tcp_conn = net.tcp_connect;
            let net_write = net.write;
            let net_read = net.read;
            let net_close = net.close;

            let sock_res = tcp_conn("127.0.0.1:{}");
            let sock = sock_res.unwrap();

            let close_res = net_close(sock);
            let is_closed_ok = close_res.is_ok();

            let write_after_close = net_write(sock, "data");
            let read_after_close = net_read(sock, 10);

            let write_failed = write_after_close.is_err();
            let read_failed = read_after_close.is_err();

            if is_closed_ok & write_failed & read_failed {{
                return 1;
            }} else {{
                return 0;
            }}
            "#,
            port
        );

        let result = engine.execute(&source).unwrap();
        assert_eq!(result, Some(Value::Int(1)));

        server_thread.join().unwrap();
    }

    #[test]
    fn test_std_root_network_exports() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();

        let server_thread = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0u8; 1024];
            let n = stream.read(&mut buf).unwrap();
            stream.write_all(&buf[..n]).unwrap();
            stream.flush().unwrap();
        });

        let engine = Engine::new();
        let source = format!(
            r#"
            let std = import `@std/std.pts`;
            let tcp_conn = std.tcp_connect;
            let net_write = std.net_write;
            let net_read = std.net_read;
            let net_close = std.net_close;

            let addr = "127.0.0.1:{}";
            let sock = tcp_conn(addr).unwrap();

            let write_res = net_write(sock, "ping");
            let read_res = net_read(sock, 4);
            net_close(sock);

            let msg = read_res.unwrap().to_string().unwrap();
            if (write_res.unwrap() == 4) & (msg == "ping") {{
                return 1;
            }} else {{
                return 0;
            }}
            "#,
            port
        );

        let result = engine.execute(&source).unwrap();
        assert_eq!(result, Some(Value::Int(1)));

        server_thread.join().unwrap();
    }

    #[test]
    fn test_unix_domain_socket_echo_and_bytes() {
        let sock_path = format!("/tmp/parts_test_uds_{}.sock", std::process::id());
        let _ = std::fs::remove_file(&sock_path);

        let listener = UnixListener::bind(&sock_path).unwrap();

        let server_thread = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0u8; 512];
            let n = stream.read(&mut buf).unwrap();
            stream.write_all(&buf[..n]).unwrap();
            stream.flush().unwrap();
        });

        let engine = Engine::new();
        let source = format!(
            r#"
            let std = import `@std/std.pts`;
            let net = std.net;
            let unix_conn = net.unix_connect;
            let net_write = net.write;
            let net_read = net.read;
            let net_close = net.close;

            let path = "{}";
            let sock_res = unix_conn(path);
            if sock_res.is_err() {{
                return 0;
            }}
            let sock = sock_res.unwrap();

            let msg = "Hello Linux UNIX socket!";
            let write_res = net_write(sock, msg);
            let bytes_written = write_res.unwrap();

            let read_res = net_read(sock, 512);
            let recv_bytes = read_res.unwrap();
            let recv_str = recv_bytes.to_string().unwrap();

            net_close(sock);

            if (bytes_written == 24) & (recv_str == msg) {{
                return 1;
            }} else {{
                return 0;
            }}
            "#,
            sock_path
        );

        let result = engine.execute(&source).unwrap();
        assert_eq!(result, Some(Value::Int(1)));

        server_thread.join().unwrap();
        let _ = std::fs::remove_file(&sock_path);
    }

    #[test]
    fn test_unix_domain_socket_invalid_path_returns_err() {
        let engine = Engine::new();
        let source = r#"
            let std = import `@std/std.pts`;
            let unix_conn = std.unix_connect;

            let sock_res = unix_conn("/tmp/parts_non_existent_uds_99999.sock");
            if sock_res.is_err() {
                return 1;
            } else {
                return 0;
            }
        "#;

        let result = engine.execute(source).unwrap();
        assert_eq!(result, Some(Value::Int(1)));
    }
}
