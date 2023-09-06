use std::ops::Deref;

use leptos::*;
use leptos_meta::provide_meta_context;
use leptos_meta::*;
use leptos_router::*;

use crate::{api, QueueItem};

#[cfg(not(feature = "ssr"))]
#[derive(Clone)]
struct MailItemNewEvent(leptos::ReadSignal<Option<QueueItem>>);

#[cfg(feature = "ssr")]
#[derive(Clone)]
struct MailItemNewEvent(leptos::ReadSignal<Option<QueueItem>>);

#[cfg(not(feature = "ssr"))]
fn sse_events() -> MailItemNewEvent {
    use futures::stream::StreamExt;

    let mut source = gloo_net::eventsource::futures::EventSource::new("/sse")
        .expect("couldn't connect to SSE stream");
    let s = create_signal_from_stream(
        source
            .subscribe("message")
            .unwrap()
            .filter_map(|res| async move {
                match res {
                    Ok((_, msg)) => msg.data().as_string().and_then(|data| {
                        match serde_json::from_str::<QueueItem>(&data) {
                            Ok(val) => Some(val),
                            Err(err) => {
                                leptos::logging::error!("failed to parse new mail event: {}", err);
                                None
                            }
                        }
                    }),
                    Err(err) => {
                        leptos::logging::error!("stream for new mail is broken: {}", err);
                        None
                    }
                }
            })
            .boxed_local(),
    );

    on_cleanup(move || source.close());
    MailItemNewEvent(s)
}

#[cfg(feature = "ssr")]
fn sse_events() -> MailItemNewEvent {
    let (s, _) = create_signal(None);
    MailItemNewEvent(s)
}

#[derive(Clone)]
struct SetMailbox(WriteSignal<Option<String>>);

#[derive(Clone)]
struct SetMail(WriteSignal<Option<String>>);

#[derive(Clone)]
struct ReadMail(WriteSignal<Option<String>>);

#[component]
fn Mailboxes() -> impl IntoView {
    let change_event = use_context::<MailItemNewEvent>()
        .expect("to have found the change_event provided")
        .0;

    let (selected_mailbox, set_selected_mailbox) = create_signal(Option::<String>::None);

    provide_context(SetMailbox(set_selected_mailbox));

    let (read_mail, set_read_mail) = create_signal(Option::<String>::None);

    provide_context(ReadMail(set_read_mail));

    let data = create_resource(
        move || (),
        move |_| async move { api::get_mailboxes().await },
    );

    create_effect(move |_| {
        if let Some(item) = change_event.get() {
            data.update(|val| {
                if let Some(Ok(ref mut vec)) = val {
                    match vec.iter_mut().find(|mailbox| mailbox.id == item.receiver) {
                        Some(mailbox) => {
                            mailbox.unread += 1;
                        }
                        None => {
                            let opt = vec
                                .iter()
                                .enumerate()
                                .find(|(_, mailbox)| mailbox.id > item.receiver);

                            let insert = crate::api::Mailbox {
                                id: item.receiver,
                                unread: 1,
                            };

                            match opt {
                                Some((idx, _)) => {
                                    vec.insert(idx, insert);
                                }
                                None => {
                                    vec.push(insert);
                                }
                            }
                        }
                    }
                }
            })
        }
    });

    create_effect(move |_| {
        if let Some(id) = read_mail.get() {
            data.update(|val| {
                if let Some(Ok(ref mut vec)) = val {
                    for mailbox in vec {
                        if mailbox.id == id {
                            mailbox.unread -= 1;
                        }
                    }
                }
            })
        }
    });

    let content = move || {
        data.get().map(|result| match result {
            Err(e) => view! { <p class="error">{e.to_string()}</p> }.into_view(),
            Ok(mailboxes) => {
                if mailboxes.is_empty() {
                    view! { <p class="empty">"No mailboxes found."</p> }.into_view()
                } else {
                    mailboxes
                        .into_iter()
                        .map(|mailbox| {
                            let unread = move || {
                                let unread = mailbox.unread;
                                if unread > 0 {
                                    format!(" ({})", unread)
                                } else {
                                    String::from("")
                                }
                            };
                            let mailbox = mailbox.id;
                            let classes = if selected_mailbox
                                .with(|value| value.as_ref().map(|v| v == &mailbox))
                                .unwrap_or(false)
                            {
                                "selected"
                            } else {
                                ""
                            };
                            view! {
                              <A class=classes href=format!("/{mailbox}")>
                                <span>{mailbox} {unread}</span>
                              </A>
                            }
                        })
                        .collect_view()
                }
            }
        })
    };

    let inner = move || {
        view! { <div>{content}</div> }.on_mount(|html| {
            let el = web_sys::Element::from(html.deref().clone());

            match el.query_selector(".selected") {
                Ok(Some(selected)) => selected.scroll_into_view(),
                _ => {}
            }
        })
    };

    view! {
      <>
        <nav>
          <Suspense fallback=|| {}>
            <div class="box">{inner}</div>
          </Suspense>
        </nav>
        <Outlet/>
      </>
    }
}

#[component]
fn Mailbox() -> impl IntoView {
    let change_event = use_context::<MailItemNewEvent>()
        .expect("to have found the mail_item_new_event provided")
        .0;

    let set_read_mail = use_context::<ReadMail>()
        .expect("to have found the read_mail provided")
        .0;

    let set_mailbox = use_context::<SetMailbox>()
        .expect("to have found the set_mailbox provided")
        .0;

    let (selected_mail, set_selected_mail) = create_signal(Option::<String>::None);

    provide_context(SetMail(set_selected_mail));

    let params = use_params_map();
    let data = create_resource(
        move || params.with(|q| q.get("mailbox").cloned().unwrap_or_default()),
        move |mailbox| async move {
            if mailbox.is_empty() {
                None
            } else {
                set_mailbox.update(|value| *value = Some(mailbox.clone()));
                Some((api::get_mailbox(mailbox.clone()).await, mailbox))
            }
        },
    );

    create_effect(move |_| {
        if let Some(item) = change_event.get() {
            data.update(|val| {
                if let Some(Some((Ok(Some(ref mut vec)), mailbox))) = val {
                    if *mailbox == item.receiver {
                        vec.insert(0, item.obj.clone());
                    }
                }
            })
        }
    });

    let content = move || {
        data.get().map(|val| match val {
            None => view! { <div></div> }.into_view(),
            Some((Err(e), _)) => view! { <p class="error">{e.to_string()}</p> }.into_view(),
            Some((Ok(opt), mailbox)) => match opt {
                Some(items) => {
                    if items.is_empty() {
                        view! { <p class="empty">"No mails found."</p> }.into_view()
                    } else {
                        items
                            .into_iter()
                            .map(move |entry| {
                                let classes = if selected_mail
                                    .with(|value| value.as_ref().map(|v| v == &entry.id))
                                    .unwrap_or(false)
                                {
                                    "selected"
                                } else if !entry.read {
                                    "unread"
                                } else {
                                    ""
                                };

                                let id = entry.id.clone();

                                let handler = move |_| {
                                    if !entry.read {
                                        data.update(|val| {
                                            if let Some(Some((Ok(Some(ref mut vec)), mailbox))) =
                                                val
                                            {
                                                for item in vec {
                                                    if item.id == id {
                                                        item.read = true;
                                                        set_read_mail.set(Some(mailbox.clone()));
                                                        break;
                                                    }
                                                }
                                            }
                                        });
                                    }
                                };

                                view! {
                                  <A
                                    on:click=handler
                                    class=classes
                                    href=format!("/{mailbox}/{}", entry.id)
                                  >
                                    <span>{entry.subject}</span>
                                  </A>
                                }
                            })
                            .collect_view()
                    }
                }
                None => view! { <p class="empty">"Mailbox not found."</p> }.into_view(),
            },
        })
    };

    let inner = move || {
        view! { <div>{content}</div> }.on_mount(|html| {
            let el = web_sys::Element::from(html.deref().clone());

            match el.query_selector(".selected") {
                Ok(Some(selected)) => selected.scroll_into_view(),
                _ => {}
            }
        })
    };

    view! {
      <>
        <nav>
          <Suspense fallback=|| {}>
            <div class="box">{inner}</div>
          </Suspense>

        </nav>
        <Outlet/>
      </>
    }
}

#[component]
fn Mail() -> impl IntoView {
    let params = use_params_map();

    let set_mail = use_context::<SetMail>()
        .expect("to have found the set_mail provided")
        .0;

    let data = create_resource(
        move || {
            (
                params.get().get("mailbox").cloned().unwrap_or_default(),
                params.get().get("mail").cloned().unwrap_or_default(),
            )
        },
        move |(mailbox, mail)| async move {
            if mailbox.is_empty() || mail.is_empty() {
                None
            } else {
                set_mail.update(|value| *value = Some(mail.clone()));
                Some((
                    api::get_mail(mailbox.clone(), mail.clone()).await,
                    mailbox,
                    mail,
                ))
            }
        },
    );

    let content = move || {
        let ty = params
            .with(|val| val.get("ty").map(|ty| ty.to_lowercase()))
            .unwrap_or_else(|| "html".to_string());

        data.get().map(|val| match val {
            None => view! { <div></div> }.into_view(),
            Some((Err(e), _, _)) => view! { <p class="error">{e.to_string()}</p> }.into_view(),
            Some((Ok(None), _, _)) => view! {
              <div class="not-found">
                Mail not found.
              </div>
            }
            .into_view(),
            Some((Ok(Some(data)), mailbox, mail)) => {
                let empty = || {
                    view! {
                      <div class="empty">
                        <p>
                          No text available
                        </p>
                      </div>
                    }
                };

                let content = match ty.as_str() {
                    "html" => {
                        if let Some(html) = data.html {
                            view! { <div class="content-html"></div> }.inner_html(html)
                        } else {
                            empty()
                        }
                    }
                    "text" => {
                        if let Some(text) = data.text {
                            view! { <div class="content-text">{text}</div> }
                        } else {
                            empty()
                        }
                    }
                    "raw" => {
                        if let Some(raw) = data.raw {
                            view! { <div class="content-raw">{raw}</div> }
                        } else {
                            empty()
                        }
                    }
                    _ => {
                        view! {
                          <div>
                            <p>
                              Unknown type
                            </p>
                          </div>
                        }
                    }
                };

                let from = data.metadata.from;
                let subject = data.metadata.subject;

                let attachments = if data.attachments.is_empty() {
                    view! {
                      <i>
                        none
                      </i>
                    }
                    .into_view()
                } else {
                    data.attachments
                        .into_iter()
                        .map(|entry| {
                            view! {
                              <a
                                href=format!("/data/{mailbox}/{mail}/attachments/{entry}")
                                target="_blank"
                              >
                                <span>{entry}</span>
                              </a>
                            }
                        })
                        .collect_view()
                };

                let selectables = ["HTML", "Text", "Raw"]
                    .into_iter()
                    .map(|v| (v, v.to_lowercase()))
                    .collect::<Vec<_>>();

                view! {
                  <div>
                    <div class="info box">
                      <p>
                        <span>
                          <b>
                            From
                          </b>
                          :
                          {" "}
                        </span>
                        {from}
                      </p>
                      <p>
                        <span>
                          <b>
                            Subject
                          </b>
                          :
                          {" "}
                        </span>
                        {subject}
                      </p>
                      <p>
                        <span>
                          <b>
                            Attachments
                          </b>
                          :
                          {" "}
                        </span>
                        {attachments}
                      </p>
                    </div>
                    <div class="selectable box">
                      {selectables
                          .into_iter()
                          .map(|(name, entry)| {
                              let classes = if ty == entry { "selected" } else { "" };
                              view! {
                                <A class=classes href=format!("/{mailbox}/{mail}/{}", entry)>
                                  <span>{name}</span>
                                </A>
                              }
                          })
                          .collect_view()}

                    </div>
                    <div class="content box">{content}</div>
                  </div>
                }
                .into_view()
            }
        })
    };

    view! {
      <main>
        <div class="mail">
          <Suspense fallback=|| {}>{content}</Suspense>
        </div>
      </main>
    }
}

#[component]
pub fn App() -> impl IntoView {
    provide_meta_context();

    let change_event = sse_events();
    provide_context(change_event);

    view! {
      <>
        <Link rel="shortcut icon" type_="image/ico" href="/assets/favicon.ico"/>
        <Stylesheet id="leptos" href="/pkg/mail-blockhole-web.css"/>
        <Meta name="description" content="Mail catcher for debugging purposes written in Leptos."/>
        <Meta name="viewport" content="width=device-width, initial-scale=1.0"/>
        <title>
          Mail Blackhole
        </title>
        <div id="root">
          <Router>
            <Routes>
              <Route path="/" view=Mailboxes>
                <Route path=":mailbox" view=Mailbox>
                  <Route path=":mail?/:ty?" view=Mail/>
                </Route>
                <Route
                  path=""
                  view=move || {
                      view! { <div></div> }
                  }
                />

              </Route>
            </Routes>
          </Router>
        </div>
      </>
    }
}
