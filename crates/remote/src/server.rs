//! The local socket: the app listens on 127.0.0.1 and writes the port and a
//! random token to a connection file only the user can read; clients read
//! that file to connect.

use std::hash::{BuildHasher, Hasher};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::protocol::{Envelope, Reply, Request};

pub const FILE_NAME: &str = "remote.json";
/// How long a client waits for the app to answer.
const ANSWER_TIMEOUT: Duration = Duration::from_secs(20);
/// Longest request line accepted.
const MAX_LINE: u64 = 1 << 20;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Connection {
    pub port: u16,
    pub token: String,
}

impl Connection {
    pub fn path(data_dir: &Path) -> PathBuf {
        data_dir.join(FILE_NAME)
    }

    pub fn read(data_dir: &Path) -> io::Result<Self> {
        let text = std::fs::read_to_string(Self::path(data_dir))?;
        serde_json::from_str(&text).map_err(io::Error::other)
    }

    fn write(&self, data_dir: &Path) -> io::Result<()> {
        std::fs::create_dir_all(data_dir)?;
        let path = Self::path(data_dir);
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
        let mut file = options.open(&path)?;
        file.write_all(serde_json::to_string(self)?.as_bytes())
    }
}

/// A random token from the standard library's randomly keyed hasher.
fn token() -> String {
    use std::fmt::Write as _;
    let mut token = String::new();
    for i in 0..2u64 {
        let mut h = std::collections::hash_map::RandomState::new().build_hasher();
        h.write_u64(i);
        h.write_u128(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos()),
        );
        h.write_u32(std::process::id());
        let _ = write!(token, "{:016x}", h.finish());
    }
    token
}

/// A request waiting for the app's answer.
#[derive(Debug)]
pub struct Call {
    pub request: Request,
    reply: Sender<Reply>,
}

impl Call {
    pub fn answer(self, result: Result<serde_json::Value, String>) {
        // The client may have given up; nothing to do then.
        let _ = self.reply.send(result.into());
    }
}

#[derive(Debug)]
pub struct Server {
    calls: Receiver<Call>,
    connection: Connection,
    data_dir: PathBuf,
    stop: Arc<AtomicBool>,
}

impl Server {
    /// Listens on a free local port and writes the connection file.
    /// `wake` is called after each request arrives, so the app can process
    /// it without waiting for its next frame.
    pub fn start(data_dir: &Path, wake: impl Fn() + Send + Sync + 'static) -> io::Result<Self> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
        let connection = Connection {
            port: listener.local_addr()?.port(),
            token: token(),
        };
        connection.write(data_dir)?;
        let (tx, calls) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let wake: Arc<dyn Fn() + Send + Sync> = Arc::new(wake);
        let token = connection.token.clone();
        let stopping = Arc::clone(&stop);
        std::thread::Builder::new()
            .name("remote".into())
            .spawn(move || {
                for stream in listener.incoming() {
                    if stopping.load(Ordering::Relaxed) {
                        break;
                    }
                    let Ok(stream) = stream else { continue };
                    let (tx, wake, token) = (tx.clone(), Arc::clone(&wake), token.clone());
                    let _ = std::thread::Builder::new()
                        .name("remote-client".into())
                        .spawn(move || serve(stream, &tx, &*wake, &token));
                }
            })?;
        Ok(Self {
            calls,
            connection,
            data_dir: data_dir.to_owned(),
            stop,
        })
    }

    pub fn connection(&self) -> &Connection {
        &self.connection
    }

    /// The next request waiting, if any.
    pub fn next_call(&self) -> Option<Call> {
        self.calls.try_recv().ok()
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        // Wake the accept loop so it sees the flag.
        let _ = TcpStream::connect((Ipv4Addr::LOCALHOST, self.connection.port));
        if Connection::read(&self.data_dir).is_ok_and(|c| c == self.connection) {
            let _ = std::fs::remove_file(Connection::path(&self.data_dir));
        }
    }
}

fn serve(stream: TcpStream, calls: &Sender<Call>, wake: &dyn Fn(), token: &str) {
    let Ok(mut writer) = stream.try_clone() else {
        return;
    };
    let mut reader = BufReader::new(stream);
    loop {
        let mut line = String::new();
        match (&mut reader).take(MAX_LINE).read_line(&mut line) {
            Ok(0) | Err(_) => return,
            Ok(_) => {}
        }
        let reply = answer(&line, calls, wake, token);
        let Ok(mut text) = serde_json::to_string(&reply) else {
            return;
        };
        text.push('\n');
        if writer.write_all(text.as_bytes()).is_err() {
            return;
        }
    }
}

fn answer(line: &str, calls: &Sender<Call>, wake: &dyn Fn(), token: &str) -> Reply {
    let envelope: Envelope = match serde_json::from_str(line) {
        Ok(e) => e,
        Err(e) => return Reply::from(Err(format!("bad request: {e}"))),
    };
    if envelope.token != token {
        return Reply::from(Err("wrong token".to_owned()));
    }
    let (reply, answer) = mpsc::channel();
    if calls
        .send(Call {
            request: envelope.request,
            reply,
        })
        .is_err()
    {
        return Reply::from(Err("Backspn is closing".to_owned()));
    }
    wake();
    answer.recv_timeout(ANSWER_TIMEOUT).unwrap_or_else(|_| {
        Reply::from(Err(
            "Backspn did not answer; is AI control allowed in Preferences?".to_owned(),
        ))
    })
}

/// A client of the app's socket.
#[derive(Debug)]
pub struct Client {
    token: String,
    reader: BufReader<TcpStream>,
    writer: TcpStream,
}

impl Client {
    pub fn connect(connection: &Connection) -> io::Result<Self> {
        let stream = TcpStream::connect((Ipv4Addr::LOCALHOST, connection.port))?;
        stream.set_read_timeout(Some(ANSWER_TIMEOUT + Duration::from_secs(5)))?;
        Ok(Self {
            token: connection.token.clone(),
            writer: stream.try_clone()?,
            reader: BufReader::new(stream),
        })
    }

    pub fn call(&mut self, request: Request) -> io::Result<Result<serde_json::Value, String>> {
        let envelope = Envelope {
            token: self.token.clone(),
            request,
        };
        let mut text = serde_json::to_string(&envelope)?;
        text.push('\n');
        self.writer.write_all(text.as_bytes())?;
        let mut line = String::new();
        if self.reader.read_line(&mut line)? == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "Backspn closed the connection",
            ));
        }
        let reply: Reply = serde_json::from_str(&line)?;
        Ok(if reply.ok {
            Ok(reply.result.unwrap_or_default())
        } else {
            Err(reply.error.unwrap_or_default())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("backspn-remote-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn a_client_talks_to_the_app_through_the_socket() {
        let dir = dir("talk");
        let server = Server::start(&dir, || {}).unwrap();
        let connection = Connection::read(&dir).unwrap();
        assert_eq!(&connection, server.connection());
        assert_eq!(connection.token.len(), 32);
        let app = std::thread::spawn(move || {
            loop {
                if let Some(call) = server.next_call() {
                    assert_eq!(call.request, Request::GetSetStatus);
                    call.answer(Ok(serde_json::json!({"state": "playing"})));
                    return server;
                }
                std::thread::sleep(Duration::from_millis(2));
            }
        });
        let mut client = Client::connect(&connection).unwrap();
        let answer = client.call(Request::GetSetStatus).unwrap();
        assert_eq!(answer, Ok(serde_json::json!({"state": "playing"})));
        drop(app.join().unwrap());
        assert!(!Connection::path(&dir).exists(), "removed on close");
    }

    #[test]
    fn a_wrong_token_is_refused() {
        let dir = dir("token");
        let server = Server::start(&dir, || {}).unwrap();
        let wrong = Connection {
            token: "guess".into(),
            ..server.connection().clone()
        };
        let mut client = Client::connect(&wrong).unwrap();
        assert_eq!(
            client.call(Request::StopSet).unwrap(),
            Err("wrong token".to_owned())
        );
        assert!(server.next_call().is_none());
    }

    #[cfg(unix)]
    #[test]
    fn only_the_user_can_read_the_connection_file() {
        use std::os::unix::fs::PermissionsExt;
        let dir = dir("mode");
        let _server = Server::start(&dir, || {}).unwrap();
        let mode = std::fs::metadata(Connection::path(&dir))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }
}
