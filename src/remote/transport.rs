use super::{Request, Response};
use anyhow::{Context, Result, ensure};
use std::{
    fs::{self, File},
    io::ErrorKind,
    os::unix::{
        fs::{DirBuilderExt, FileTypeExt, OpenOptionsExt, PermissionsExt},
        net::{SocketAddr, UnixDatagram},
    },
    path::{Path, PathBuf},
    time::Duration,
};

pub struct Server {
    socket: UnixDatagram,
    _lock: File,
}

pub struct Incoming {
    pub request: Request,
    address: SocketAddr,
}

fn control_directory(directory: &Path) -> Result<PathBuf> {
    let directory = directory
        .canonicalize()
        .context("Cannot locate library / Каталог библиотеки недоступен")?;
    let direct = directory.join(".almavorn-control");
    if direct.as_os_str().as_encoded_bytes().len() < 85 {
        return Ok(direct);
    }
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in directory.as_os_str().as_encoded_bytes() {
        hash = (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3);
    }
    Ok(dirs::runtime_dir()
        .context("Path is too long and a user runtime directory is unavailable")?
        .join(format!("almavorn-{hash:016x}")))
}

impl Server {
    pub fn bind(directory: &Path) -> Result<Self> {
        fs::create_dir_all(directory).context("Cannot create library directory")?;
        let private = control_directory(directory)?;
        match fs::DirBuilder::new().mode(0o700).create(&private) {
            Ok(()) => {}
            Err(error) if error.kind() == ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error).context("Cannot create private command directory"),
        }
        let metadata = fs::symlink_metadata(&private)?;
        ensure!(
            metadata.is_dir() && metadata.permissions().mode() & 0o077 == 0,
            "Unsafe command directory / Каталог команд должен быть закрыт для других пользователей"
        );
        let lock_path = private.join("instance.lock");
        if let Ok(metadata) = fs::symlink_metadata(&lock_path) {
            ensure!(metadata.is_file(), "Command lock must be a regular file");
        }
        let lock = File::options()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .open(lock_path)?;
        lock.try_lock().context("Almavorn is already running for this library / Almavorn уже запущен для этой библиотеки")?;
        let path = private.join("commands.sock");
        match fs::symlink_metadata(&path) {
            Ok(metadata) => {
                ensure!(
                    metadata.file_type().is_socket(),
                    "Command path is occupied by another file; it was preserved"
                );
                // The exclusive instance lock proves that this is a stale socket.
                fs::remove_file(&path)?;
            }
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        let socket = UnixDatagram::bind(&path).context("Cannot open local command socket")?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
        socket.set_nonblocking(true)?;
        Ok(Self {
            socket,
            _lock: lock,
        })
    }

    pub fn receive(&self) -> Result<Option<Incoming>> {
        let mut buffer = [0; 4096];
        let (size, address) = match self.socket.recv_from(&mut buffer) {
            Ok(message) => message,
            Err(error) if error.kind() == ErrorKind::WouldBlock => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        match serde_json::from_slice(&buffer[..size]) {
            Ok(request) => Ok(Some(Incoming { request, address })),
            Err(_) => {
                self.reply(&address, Err(anyhow::anyhow!("Invalid Almavorn command")));
                Ok(None)
            }
        }
    }

    pub fn respond(&self, incoming: &Incoming, result: Result<String>) {
        self.reply(&incoming.address, result);
    }

    fn reply(&self, address: &SocketAddr, result: Result<String>) {
        let response = match result {
            Ok(message) => Response { ok: true, message },
            Err(error) => Response {
                ok: false,
                message: format!("{error:#}"),
            },
        };
        if let Some(path) = address.as_pathname()
            && let Ok(bytes) = serde_json::to_vec(&response)
        {
            let _ = self.socket.send_to(&bytes, path);
        }
    }
}

pub fn send(directory: &Path, request: &Request) -> Result<String> {
    let path = control_directory(directory)?.join("commands.sock");
    let temporary = tempfile::Builder::new()
        .prefix("almavorn-command-")
        .tempdir()?;
    let socket = UnixDatagram::bind(temporary.path().join("reply.sock"))?;
    socket.connect(path).context(
        "Almavorn is not running for this library / Запустите Almavorn для выбранной библиотеки",
    )?;
    socket.set_read_timeout(Some(Duration::from_secs(3)))?;
    socket.set_write_timeout(Some(Duration::from_secs(1)))?;
    socket.send(&serde_json::to_vec(request)?)?;
    let mut buffer = vec![0; 65536];
    let size = socket.recv(&mut buffer).context("Almavorn did not acknowledge the command; its execution is unknown / Плеер не подтвердил команду; результат неизвестен")?;
    let response: Response =
        serde_json::from_slice(&buffer[..size]).context("Invalid reply from Almavorn")?;
    ensure!(response.ok, "{}", response.message);
    Ok(response.message)
}
