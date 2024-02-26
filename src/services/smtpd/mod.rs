use std::io;
use std::io::{prelude::*, Error};
use std::task::Poll;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio_stream::Stream;

pub struct SmtpServer {
    send_channel: mpsc::Sender<SmtpMail>,
    recv_channel: mpsc::Receiver<SmtpMail>,
}

enum SMTPError {}

impl SmtpServer {
    pub fn new() -> SmtpServer {
        let (send, recv) = mpsc::channel(512);
        SmtpServer {
            send_channel: send,
            recv_channel: recv,
        }
    }

    pub fn to_smtp_connection(&mut self, stream: TcpStream) -> SmtpConnection {
        SmtpConnection {
            stream,
            send_channel: self.send_channel.clone(),
            hostname: None,
            mailfrom: None,
            rcpt: None,
            message: String::new(),
            state: SmtpState::Command,
        }
    }

    pub async fn start_listener_thread(&mut self) -> Result<(), Error> {
        let listener = TcpListener::bind("127.0.0.1:8080").await?;
        // let listener = try!(TcpListener::bind(addr));
        let send_channel = self.send_channel.clone();

        loop {
            let (stream, _) = listener.accept().await?;

            let mut conn = SmtpConnection::with_channel(stream, send_channel.clone());
            let _ = conn.handle_connection();
        }
    }
}

impl Stream for SmtpServer {
    type Item = SmtpMail;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        match self.recv_channel.poll_recv(cx) {
            Poll::Ready(item) => Poll::Ready(item),
            Poll::Pending => Poll::Pending,
        }
    }
}

pub struct SmtpConnection {
    stream: TcpStream,
    send_channel: mpsc::Sender<SmtpMail>,
    hostname: Option<String>,
    mailfrom: Option<String>,
    rcpt: Option<Vec<String>>,
    message: String,
    state: SmtpState,
}

impl SmtpConnection {
    pub fn with_channel(stream: TcpStream, send: mpsc::Sender<SmtpMail>) -> SmtpConnection {
        SmtpConnection {
            stream,
            send_channel: send.clone(),
            hostname: None,
            mailfrom: None,
            rcpt: None,
            message: String::new(),
            state: SmtpState::Command,
        }
    }

    pub async fn handle_connection(&mut self) -> io::Result<()> {
        self.stream.write_all(b"220 Rust smtpd v0.1.0\r\n").await;
        let (reader, _) = self.stream.split();
        let reader = BufReader::new(reader);
        for line in reader.lines().next_line().await? {
            let line = line.as_str();
            match self.line_received(&line).await {
                Ok(_) => {
                    if self.state == SmtpState::Quit {
                        break;
                    }
                }
                Err(_) => {
                    break;
                }
            }
        };
        Ok(())
    }

    pub async fn line_received(&mut self, line: &str) -> std::io::Result<()> {
        match self.state {
            SmtpState::Command => {
                let space_pos = line.find(" ").unwrap_or(line.len());
                let (command, arg) = line.split_at(space_pos);
                let arg = arg.trim();

                match &*command.to_uppercase() {
                    "HELO" | "EHLO" => {
                        if !arg.is_empty() {
                            self.hostname = Some(arg.to_string());
                            self.stream
                                .write_all(format!("250 Hello {}\r\n", arg).as_bytes())
                                .await;
                        } else {
                            self.stream
                                .write_all(b"501 Syntax: HELO hostname\r\n")
                                .await;
                        }
                    }
                    "MAIL" => {
                        // Syntax MAIL From: <user@example.com>

                        let lower_arg = arg.to_lowercase();
                        if lower_arg.starts_with("from:") {
                            let angle_brackets: &[_] = &['<', '>'];
                            let address = lower_arg
                                .trim_start_matches("from:")
                                .trim()
                                .trim_matches(angle_brackets)
                                .trim();

                            self.mailfrom = Some(address.to_string());
                            self.stream.write_all(b"250 OK\r\n").await;
                        } else {
                            self.stream
                                .write_all(b"501 Syntax: MAIL From: <address>\r\n")
                                .await;
                        }
                    }
                    "RCPT" => {
                        // Syntax RCPT To: <user@example.com>

                        match self.mailfrom {
                            Some(_) => {
                                let lower_arg = arg.to_lowercase();
                                if lower_arg.starts_with("to:") {
                                    let angle_brackets: &[_] = &['<', '>'];
                                    let address = lower_arg
                                        .trim_start_matches("to:")
                                        .trim()
                                        .trim_matches(angle_brackets)
                                        .trim();

                                    if self.rcpt.is_some() {
                                        self.rcpt.as_mut().unwrap().push(address.to_string())
                                    } else {
                                        self.rcpt = Some(vec![address.to_string()]);
                                    }
                                    
                                    self.stream.write_all(b"250 OK\r\n").await;
                                } else {
                                    self.stream
                                        .write_all(b"501 Syntax: RCPT To: <address>\r\n")
                                        .await;
                                }
                            }
                            None => {
                                self.stream
                                    .write_all(b"503 Error: Send MAIL first\r\n")
                                    .await;
                            }
                        }
                    }
                    "DATA" => {
                        if self.hostname.is_none() {
                            self.stream
                                .write_all(b"503 Error: Send HELO/EHLO first\r\n")
                                .await;
                            return Ok(());
                        }

                        if self.rcpt.is_none() {
                            self.stream
                                .write_all(b"503 Error: Send RCPT first\r\n")
                                .await;
                            return Ok(());
                        }

                        self.state = SmtpState::Data;
                        self.stream
                            .write_all(b"354 End data with <CRLF>.<CRLF>\r\n")
                            .await;
                    }
                    "NOOP" => {
                        if arg.is_empty() {
                            self.stream.write_all(b"250 OK\r\n").await;
                        } else {
                            self.stream.write_all(b"501 Syntax: NOOP\r\n").await;
                        }
                    }
                    "RSET" => {
                        self.mailfrom = None;
                        self.rcpt = None;
                        self.message = String::new();

                        self.stream.write_all(b"250 OK\r\n").await;
                    }
                    "QUIT" => {
                        self.stream.write_all(b"221 Have a nice day!\r\n").await;
                        self.state = SmtpState::Quit;
                    }
                    x => {
                        self.stream
                            .write_all(format!("500 Error: Unknown command '{}'\r\n", x).as_bytes())
                            .await;
                    }
                }
            }
            SmtpState::Data => {
                // Write message
                if line.trim() == "." {
                    self.stream.write_all(b"250 OK\r\n").await;
                    let mail = SmtpMail {
                        from: self.mailfrom.clone().unwrap_or(String::new()),
                        rcpt: self.rcpt.clone().unwrap_or(vec![String::new()]),
                        message_body: self.message.clone(),
                    };
                    self.send_channel.send(mail).await.unwrap()
                } else {
                    self.message.push_str(line);
                    self.message.push_str("\n");
                }
            }
            SmtpState::Quit => {}
        }

        Ok(())
    }
}

#[derive(PartialEq)]
enum SmtpState {
    Command,
    Data,
    Quit,
}

pub struct SmtpMail {
    pub from: String,
    pub rcpt: Vec<String>,
    pub message_body: String,
}
