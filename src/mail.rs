//! Interface for accessing mailboxes.
//!
//! Everything mail related is stored in the file system. Each
//! recipient has its own mailbox. Each mailbox contains mails.

use mail_parser::{Message, MimeHeaders};
use mailin::{Action, Handler, Response, SessionBuilder};
use std::path::StripPrefixError;
use std::{
    fs::File,
    io::Read,
    io::Write,
    path::{Path, PathBuf},
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{TcpListener, TcpStream},
    sync::broadcast::Sender,
};

use crate::api::MailboxItem;
use crate::{Args, QueueItem};

fn try_exists(path: &Path) -> Result<bool, MailError> {
    path.try_exists().map_err(|err| MailError {
        kind: MailErrorKind::FileOpen(err),
        path: path.to_path_buf().into(),
    })
}

fn read_dir<T>(
    path: &Path,
    f: impl Fn(std::fs::DirEntry) -> Result<Option<T>, MailError>,
) -> Result<Vec<T>, MailError> {
    path.read_dir()
        .map_err(|err| MailError {
            kind: MailErrorKind::DirRead(err),
            path: path.into(),
        })
        .and_then(|e| {
            e.into_iter()
                .filter_map(|e| {
                    e.map_err(|err| MailError {
                        kind: MailErrorKind::DirRead(err),
                        path: path.into(),
                    })
                    .and_then(&f)
                    .transpose()
                })
                .collect::<Result<Vec<_>, _>>()
        })
}

#[derive(Debug)]
pub struct MailError {
    kind: MailErrorKind,
    path: std::path::PathBuf,
}

impl MailError {
    pub fn io_error(self) -> Option<std::io::Error> {
        match self.kind {
            MailErrorKind::FileOpen(err)
            | MailErrorKind::FileRead(err)
            | MailErrorKind::FileWrite(err)
            | MailErrorKind::FileExists(err)
            | MailErrorKind::FileMetadata(err)
            | MailErrorKind::DirCreate(err)
            | MailErrorKind::DirRead(err) => Some(err),
            MailErrorKind::SerdeRead(_) | MailErrorKind::SerdeWrite(_) => None,
        }
    }
}

#[derive(Debug)]
pub enum MailErrorKind {
    FileOpen(std::io::Error),
    FileRead(std::io::Error),
    FileWrite(std::io::Error),
    FileExists(std::io::Error),
    FileMetadata(std::io::Error),
    SerdeWrite(serde_json::Error),
    SerdeRead(serde_json::Error),
    DirRead(std::io::Error),
    DirCreate(std::io::Error),
}

impl std::error::Error for MailError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match &self.kind {
            MailErrorKind::FileOpen(err) => Some(err),
            MailErrorKind::FileRead(err) => Some(err),
            MailErrorKind::FileWrite(err) => Some(err),
            MailErrorKind::FileExists(err) => Some(err),
            MailErrorKind::FileMetadata(err) => Some(err),
            MailErrorKind::SerdeWrite(err) => Some(err),
            MailErrorKind::SerdeRead(err) => Some(err),
            MailErrorKind::DirRead(err) => Some(err),
            MailErrorKind::DirCreate(err) => Some(err),
        }
    }
}

impl std::fmt::Display for MailError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        match &self.kind {
            MailErrorKind::FileOpen(err) => write!(f, "FileOpen[{}]: {}", self.path.display(), err),
            MailErrorKind::FileRead(err) => write!(f, "FileRead[{}]: {}", self.path.display(), err),
            MailErrorKind::FileWrite(err) => {
                write!(f, "FileWrite[{}]: {}", self.path.display(), err)
            }
            MailErrorKind::FileExists(err) => {
                write!(f, "FileExists[{}]: {}", self.path.display(), err)
            }
            MailErrorKind::FileMetadata(err) => {
                write!(f, "FileMetadata[{}]: {}", self.path.display(), err)
            }
            MailErrorKind::SerdeWrite(err) => {
                write!(f, "SerdeWrite[{}]: {}", self.path.display(), err)
            }
            MailErrorKind::SerdeRead(err) => {
                write!(f, "SerdeRead[{}]: {}", self.path.display(), err)
            }
            MailErrorKind::DirRead(err) => write!(f, "DirRead[{}]: {}", self.path.display(), err),
            MailErrorKind::DirCreate(err) => {
                write!(f, "DirCreate[{}]: {}", self.path.display(), err)
            }
        }
    }
}

pub struct Mailboxes {
    pub path: PathBuf,
}

impl Mailboxes {
    pub fn mailboxes(&self) -> Result<Vec<Mailbox>, MailError> {
        read_dir(&self.path, |entry| {
            entry
                .metadata()
                .map_err(|err| MailError {
                    kind: MailErrorKind::FileMetadata(err),
                    path: entry.path().into(),
                })
                .map(|meta| {
                    if meta.is_dir() {
                        Some(Mailbox { path: entry.path() })
                    } else {
                        None
                    }
                })
        })
    }

    pub fn mailbox(&self, mailbox: &str) -> Result<Option<Mailbox>, MailError> {
        let path = self.path.join(mailbox);

        if try_exists(&path)? {
            let meta = path.metadata().map_err(|err| MailError {
                kind: MailErrorKind::FileMetadata(err),
                path: path.clone(),
            })?;

            if meta.is_dir() {
                Ok(Some(Mailbox { path }))
            } else {
                Ok(None)
            }
        } else {
            Ok(None)
        }
    }
}

pub struct Mailbox {
    path: PathBuf,
}

impl Mailbox {
    pub fn id(&self) -> String {
        self.path.file_name().unwrap().to_str().unwrap().to_string()
    }

    pub fn mails(&self) -> Result<Vec<MailItem>, MailError> {
        read_dir(&self.path, |entry| {
            entry
                .metadata()
                .map_err(|err| MailError {
                    kind: MailErrorKind::FileMetadata(err),
                    path: entry.path().into(),
                })
                .map(|meta| {
                    if meta.is_dir() {
                        Some(MailItem { path: entry.path() })
                    } else {
                        None
                    }
                })
        })
    }

    pub fn mail(&self, mail: &str) -> Result<Option<MailItem>, MailError> {
        let path = self.path.join(mail);

        if try_exists(&path)? {
            let meta = path.metadata().map_err(|err| MailError {
                kind: MailErrorKind::FileMetadata(err),
                path: path.clone(),
            })?;

            if meta.is_dir() {
                Ok(Some(MailItem { path }))
            } else {
                Ok(None)
            }
        } else {
            Ok(None)
        }
    }

    pub fn unread(&self) -> Result<i64, MailError> {
        read_dir(&self.path, |entry| {
            entry
                .metadata()
                .map_err(|err| MailError {
                    kind: MailErrorKind::FileMetadata(err),
                    path: entry.path().into(),
                })
                .and_then(|meta| {
                    if meta.is_dir() {
                        let mail = MailItem { path: entry.path() };
                        let count = if mail.read()? { 0 } else { 1 };

                        Ok(Some(count))
                    } else {
                        Ok(None)
                    }
                })
        })
        .map(|v| v.into_iter().sum())
    }
}

#[derive(Debug)]
pub struct MailItem {
    path: PathBuf,
}

impl MailItem {
    pub fn id(&self) -> String {
        self.path.file_name().unwrap().to_str().unwrap().to_string()
    }

    pub fn new(path: PathBuf, message: &Message, subject: String) -> Result<(), MailError> {
        let me = Self { path };

        me.init(message, subject)
    }

    fn init(&self, message: &Message, subject: String) -> Result<(), MailError> {
        {
            let mut file = File::create(self.metadata_path()).map_err(|err| MailError {
                kind: MailErrorKind::FileOpen(err),
                path: self.metadata_path().into(),
            })?;
            serde_json::to_writer(
                &mut file,
                &Metadata {
                    subject,
                    id: self.path.file_name().unwrap().to_string_lossy().to_string(),
                    from: match message.from() {
                        mail_parser::HeaderValue::Address(addr) => addr
                            .address
                            .as_ref()
                            .map(|v| v.clone().into_owned())
                            .unwrap_or(String::new()),
                        mail_parser::HeaderValue::Text(x) => x.clone().into_owned(),
                        mail_parser::HeaderValue::Group(_)
                        | mail_parser::HeaderValue::AddressList(_)
                        | mail_parser::HeaderValue::GroupList(_)
                        | mail_parser::HeaderValue::TextList(_)
                        | mail_parser::HeaderValue::DateTime(_)
                        | mail_parser::HeaderValue::ContentType(_)
                        | mail_parser::HeaderValue::Empty => String::new(),
                    },
                    date: message.date().map(|date| date.to_rfc3339()),
                },
            )
            .map_err(|err| MailError {
                kind: MailErrorKind::SerdeWrite(err),
                path: self.metadata_path().into(),
            })?;
        }

        {
            let mut file = File::create(self.html_path()).map_err(|err| MailError {
                kind: MailErrorKind::FileOpen(err),
                path: self.html_path().into(),
            })?;

            for idx in &message.html_body {
                file.write_all(
                    message
                        .part(*idx)
                        .unwrap()
                        .text_contents()
                        .unwrap()
                        .as_ref(),
                )
                .map_err(|err| MailError {
                    kind: MailErrorKind::FileWrite(err),
                    path: self.html_path().into(),
                })?;
            }
        }

        {
            let mut file = File::create(self.text_path()).map_err(|err| MailError {
                kind: MailErrorKind::FileOpen(err),
                path: self.text_path().into(),
            })?;

            for idx in &message.text_body {
                file.write_all(
                    message
                        .part(*idx)
                        .unwrap()
                        .text_contents()
                        .unwrap()
                        .as_ref(),
                )
                .map_err(|err| MailError {
                    kind: MailErrorKind::FileWrite(err),
                    path: self.text_path().into(),
                })?;
            }
        }

        {
            let mut file = std::fs::File::create(self.raw_path()).map_err(|err| MailError {
                kind: MailErrorKind::FileOpen(err),
                path: self.raw_path().into(),
            })?;
            file.write_all(&message.raw_message)
                .map_err(|err| MailError {
                    kind: MailErrorKind::FileWrite(err),
                    path: self.raw_path().into(),
                })?;
        }

        {
            let attachment_dir = self.attachments_path();

            std::fs::create_dir(&attachment_dir).map_err(|err| MailError {
                kind: MailErrorKind::DirCreate(err),
                path: attachment_dir.clone(),
            })?;

            for part in message.attachments() {
                let is_file = part
                    .content_disposition()
                    .map(|v| v.is_attachment())
                    .unwrap_or(false);
                let file_name = part.attachment_name();

                if is_file {
                    if let Some(name) = file_name {
                        let path = attachment_dir.join(name);

                        let mut file = std::fs::File::create(&path).map_err(|err| MailError {
                            kind: MailErrorKind::FileOpen(err),
                            path: path.clone(),
                        })?;
                        file.write_all(part.contents()).map_err(|err| MailError {
                            kind: MailErrorKind::FileWrite(err),
                            path: path.clone(),
                        })?;
                    }
                }
            }
        }

        Ok(())
    }

    pub fn metadata_path(&self) -> PathBuf {
        self.path.join("metadata.json")
    }

    pub fn raw_path(&self) -> PathBuf {
        self.path.join("body.raw")
    }

    pub fn html_path(&self) -> PathBuf {
        self.path.join("body.html")
    }

    pub fn text_path(&self) -> PathBuf {
        self.path.join("body.text")
    }

    pub fn attachments_path(&self) -> PathBuf {
        self.path.join("attachments")
    }

    pub fn raw(&self) -> Result<Vec<u8>, MailError> {
        let path = self.raw_path();
        let mut file = File::open(&path).map_err(|err| MailError {
            kind: MailErrorKind::FileOpen(err),
            path: path.clone(),
        })?;
        let mut string = Vec::new();

        file.read_to_end(&mut string).map_err(|err| MailError {
            kind: MailErrorKind::FileWrite(err),
            path: path.clone(),
        })?;

        Ok(string)
    }

    pub fn has_html(&self) -> Result<bool, MailError> {
        try_exists(&self.html_path())
    }

    pub fn html(&self) -> Result<Option<String>, MailError> {
        if self.has_html()? {
            let path = self.html_path();
            let mut file = File::open(&path).map_err(|err| MailError {
                kind: MailErrorKind::FileOpen(err),
                path: path.clone(),
            })?;
            let mut string = Vec::new();

            file.read_to_end(&mut string).map_err(|err| MailError {
                kind: MailErrorKind::FileWrite(err),
                path: path.clone(),
            })?;

            Ok(Some(String::from_utf8(string).unwrap()))
        } else {
            Ok(None)
        }
    }

    pub fn has_text(&self) -> Result<bool, MailError> {
        try_exists(&self.text_path())
    }

    pub fn text(&self) -> Result<Option<String>, MailError> {
        if self.has_text()? {
            let path = self.text_path();
            let mut file = File::open(&path).map_err(|err| MailError {
                kind: MailErrorKind::FileOpen(err),
                path: path.clone(),
            })?;
            let mut string = Vec::new();

            file.read_to_end(&mut string).map_err(|err| MailError {
                kind: MailErrorKind::FileRead(err),
                path: path.clone(),
            })?;
            Ok(Some(String::from_utf8(string).unwrap()))
        } else {
            Ok(None)
        }
    }

    pub fn metadata(&self) -> Result<Metadata, MailError> {
        let path = self.metadata_path();
        let file = File::open(&path).map_err(|err| MailError {
            kind: MailErrorKind::FileOpen(err),
            path: path.clone(),
        })?;
        let reader = std::io::BufReader::new(file);
        let json = serde_json::from_reader(reader).map_err(|err| MailError {
            kind: MailErrorKind::SerdeRead(err),
            path: path.clone(),
        })?;

        Ok(json)
    }

    fn read_path(&self) -> PathBuf {
        self.path.join("read")
    }

    pub fn read(&self) -> Result<bool, MailError> {
        let path = self.read_path();

        path.try_exists().map_err(|err| MailError {
            kind: MailErrorKind::FileExists(err),
            path: path.clone(),
        })
    }

    pub fn set_read(&self) -> Result<(), MailError> {
        let path = self.read_path();

        let mut file = File::create(&path).map_err(|err| MailError {
            kind: MailErrorKind::FileOpen(err),
            path: path.clone(),
        })?;

        let read_at = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
        let string = format!("{}", read_at.as_millis());

        file.write_all(string.as_bytes()).map_err(|err| MailError {
            kind: MailErrorKind::FileWrite(err),
            path: path.clone(),
        })?;

        Ok(())
    }

    pub fn attachments(&self) -> Result<Vec<Attachment>, MailError> {
        let path = self.path.join("attachments");

        read_dir(&path, |e| {
            let val = if e
                .file_type()
                .map_err(|err| MailError {
                    kind: MailErrorKind::FileMetadata(err),
                    path: e.path().into(),
                })?
                .is_file()
            {
                Some(Attachment { path: e.path() })
            } else {
                None
            };

            Ok(val)
        })
    }
}

pub struct Attachment {
    path: PathBuf,
}

impl Attachment {
    pub fn id(&self) -> String {
        self.path.file_name().unwrap().to_str().unwrap().to_string()
    }

    pub fn data(&self) -> Result<Vec<u8>, MailError> {
        let mut file = File::open(&self.path).map_err(|err| MailError {
            kind: MailErrorKind::FileOpen(err),
            path: self.path.clone(),
        })?;
        let mut string = Vec::new();

        file.read_to_end(&mut string).map_err(|err| MailError {
            kind: MailErrorKind::FileRead(err),
            path: self.path.clone(),
        })?;

        Ok(string)
    }

    pub fn relative<'a>(&'a self, base: &'a Path) -> Result<&Path, StripPrefixError> {
        base.strip_prefix(&self.path)
    }
}

#[derive(Debug, serde::Deserialize, serde::Serialize, PartialEq, Eq, Clone)]
pub struct Metadata {
    pub id: String,
    pub subject: String,
    pub from: String,
    pub date: Option<String>,
}

#[derive(Clone)]
struct MyHandler {
    channel: Sender<Arc<QueueItem>>,
    path: Arc<PathBuf>,
    addresses: Vec<String>,
    buffer: Vec<u8>,
}

fn ensure_dir(path: &Path) -> std::io::Result<bool> {
    if path.exists() {
        if path.is_dir() {
            Ok(true)
        } else {
            Ok(false)
        }
    } else {
        std::fs::create_dir(path)?;
        Ok(true)
    }
}

impl Handler for MyHandler {
    fn data_start(&mut self, _: &str, _: &str, _: bool, to: &[String]) -> Response {
        self.addresses = to.to_vec();
        mailin::response::OK
    }

    fn data(&mut self, buf: &[u8]) -> std::io::Result<()> {
        self.buffer.extend_from_slice(buf);

        Ok(())
    }

    fn data_end(&mut self) -> Response {
        let mut f = || -> std::io::Result<()> {
            let message = match mail_parser::Message::parse(&self.buffer) {
                Some(val) => val,
                None => {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "could not parse mail message",
                    ));
                }
            };

            let receivers = if self.addresses.is_empty() {
                match message.to() {
                    mail_parser::HeaderValue::Address(ref addr) => {
                        vec![addr.address.as_ref().unwrap().to_string()]
                    }
                    _ => {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            "missing TO header in mail",
                        ))
                    }
                }
            } else {
                std::mem::replace(&mut self.addresses, Vec::new())
            };

            println!("received email for: {:?}", receivers);

            let since_the_epoch = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
            let id = format!("{}", since_the_epoch.as_millis());
            let subject = message.subject().unwrap_or_else(|| &id).to_string();

            for receiver in receivers {
                let postbox = self.path.join(&receiver);

                let try_block = || {
                    if !ensure_dir(&postbox)? {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::AlreadyExists,
                            "could not create postbox directory",
                        ));
                    }

                    let mail_path = postbox.join(&id);

                    std::fs::create_dir(&mail_path)?;

                    match MailItem::new(mail_path, &message, subject.clone()) {
                        Ok(_) => {
                            println!("stored email for: {}", receiver);
                        }
                        Err(err) => {
                            println!("failed stored email for `{}`: {}", receiver, err);
                            match err.io_error() {
                                Some(io) => return Err(io),
                                None => {
                                    todo!()
                                }
                            }
                        }
                    }

                    let _ = self.channel.send(
                        QueueItem {
                            obj: MailboxItem {
                                subject: subject.to_string(),
                                id: id.clone(),
                                read: false,
                            },
                            receiver,
                        }
                        .into(),
                    );

                    Ok(())
                };

                if let Err(err) = try_block() {
                    std::fs::remove_dir(&postbox)?;
                    return Err(err);
                }
            }

            Ok(())
        };

        let res = f();

        self.buffer.clear();

        match res {
            Ok(_) => mailin::response::OK,
            Err(err) => {
                println!("error: {}", err);
                mailin::response::INTERNAL_ERROR
            }
        }
    }
}

pub async fn listen(
    args: &Args,
    channel: Sender<Arc<QueueItem>>,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("mail server listining on {}", args.listen_mail);

    let listener = TcpListener::bind(&args.listen_mail).await?;

    let handler = MyHandler {
        channel,
        path: Arc::new(args.mailboxes.clone()),
        addresses: Vec::new(),
        buffer: Vec::new(),
    };

    loop {
        let (socket, _) = listener.accept().await?;

        let handler = handler.clone();
        tokio::spawn(async move {
            let _ = process(socket, handler).await;
        });
    }
}

async fn process(mut stream: TcpStream, handler: MyHandler) -> std::io::Result<()> {
    let mut session =
        SessionBuilder::new("mailserver_name").build(stream.peer_addr()?.ip(), handler.clone());

    let (read, mut write) = stream.split();
    let mut read = BufReader::new(read);
    let mut buffer = Vec::new();

    write.write(&session.greeting().buffer()?).await?;

    loop {
        buffer.clear();

        match read.read_until(b'\n', &mut buffer).await {
            Ok(_) => {
                let res = session.process(&buffer);

                match res.action {
                    Action::Reply => {
                        write.write(&res.buffer()?).await?;
                    }
                    Action::Close => {
                        write.write(&res.buffer()?).await?;
                        write.shutdown().await?;
                        break;
                    }
                    Action::NoReply => (),
                    Action::UpgradeTls => (),
                }
            }
            Err(_) => break,
        }
    }

    Ok(())
}
