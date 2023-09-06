use leptos::*;
use serde::{Deserialize, Serialize};

#[cfg(feature = "ssr")]
use crate::mail as fs;

#[cfg(feature = "ssr")]
pub fn mailboxes_path() -> Result<fs::Mailboxes, ServerFnError> {
    use_context::<std::path::PathBuf>()
        .ok_or_else(|| ServerFnError::ServerError("Missing context: mailbox path".into()))
        .map(|path| fs::Mailboxes { path })
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq, Clone)]
pub struct Mailbox {
    pub id: String,
    pub unread: i64,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq, Clone)]
pub struct MailboxItem {
    pub subject: String,
    pub id: String,
    pub read: bool,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq, Clone)]
pub struct Mail {
    pub html: Option<String>,
    pub text: Option<String>,
    pub raw: Option<String>,
    pub attachments: Vec<String>,
    pub metadata: Metadata,
}

#[derive(Debug, serde::Deserialize, serde::Serialize, PartialEq, Eq, Clone)]
pub struct Metadata {
    pub id: String,
    pub subject: String,
    pub from: String,
    pub date: Option<String>,
}

#[server(GetMailboxes, "/api")]
pub async fn get_mailboxes() -> Result<Vec<Mailbox>, ServerFnError> {
    let mailboxes = mailboxes_path()?;

    let mut vec = mailboxes
        .mailboxes()?
        .into_iter()
        .map(|m| {
            Ok(Mailbox {
                id: m.id(),
                unread: m.unread()?,
            })
        })
        .collect::<Result<Vec<_>, crate::mail::MailError>>()?;

    vec.sort_by(|a, b| a.id.cmp(&b.id));

    Ok(vec)
}

#[server(GetMailbox, "/api")]
pub async fn get_mailbox(mailbox: String) -> Result<Option<Vec<MailboxItem>>, ServerFnError> {
    let mailboxes = mailboxes_path()?;

    let data = match mailboxes.mailbox(&mailbox)? {
        Some(mailbox) => {
            let mut vec = mailbox
                .mails()?
                .into_iter()
                .map(|mail| {
                    let read = mail.read()?;
                    mail.metadata()
                        .map(|metadata| MailboxItem {
                            subject: metadata.subject,
                            id: metadata.id,
                            read,
                        })
                        .map_err(|e| ServerFnError::ServerError(format!("{}", e)))
                })
                .collect::<Result<Vec<_>, _>>()?;

            vec.sort_by(|a, b| b.id.cmp(&a.id));

            Some(vec)
        }
        None => None,
    };

    Ok(data)
}

#[server(GetMail, "/api")]
pub async fn get_mail(mailbox: String, mail: String) -> Result<Option<Mail>, ServerFnError> {
    let mailboxes = mailboxes_path()?;

    let mail = mailboxes
        .mailbox(&mailbox)?
        .and_then(|mailbox| mailbox.mail(&mail).transpose())
        .transpose()?
        .map(|mail| {
            if !mail.read()? {
                mail.set_read()?;
            }

            Ok::<_, fs::MailError>(Mail {
                html: mail.html()?,
                text: mail.text()?,
                raw: String::from_utf8(mail.raw()?).ok(),
                attachments: mail.attachments()?.into_iter().map(|a| a.id()).collect(),
                metadata: {
                    let val = mail.metadata()?;
                    Metadata {
                        id: val.id,
                        subject: val.subject,
                        from: val.from,
                        date: val.date,
                    }
                },
            })
        })
        .transpose()?;

    Ok(mail)
}
