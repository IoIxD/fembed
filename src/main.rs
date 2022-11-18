#![feature(async_closure)]
#![allow(unused_variables)]
#![allow(unused_assignments)]

use lazy_static::lazy_static;
use megalodon::detector;
use megalodon::{SNS, entities::Status, response::Response as MegResponse};
use regex::{Regex, Captures};
use tokio::runtime::Builder;
use oxhttp::Server;
use oxhttp::model::{Response,Status as OxStatus,HeaderName};
use std::time::Duration;
use std::result::Result;
use askama::*;

lazy_static!(
    pub static ref URL_REGEX: Regex = Regex::new(
        &r"(https?)://?(([-a-zA-Z0-9@:%._\+~#=]{1,256}\.?){1,6}\b)([-a-zA-Z0-9()@:%_\+.~#?&//=]*)"
    .replace(" ", "")).unwrap();

    pub static ref HTML_REGEX: Regex = Regex::new(
        &r"</?(.*?)>"
    ).unwrap();

    pub static ref EMOTE_REGEX: Regex = Regex::new(
        r":(.*?):"
    ).unwrap();

    pub static ref LETTERS_REGEX: Regex = Regex::new(
        r"([A-z]*)"
    ).unwrap();


);


#[derive(Template)]
#[template(path = "image.html")]
struct StatusImageTemplate<'a> {
    status: &'a Status,
    path: &'a str,
    parts: &'a URLParts,
    display_name: &'a String,
    content: &'a String,
    media: &'a String,
    media_width: u32,
    media_height: u32,
    media_type: &'a str,
    mime_type: &'a str,
}

#[derive(Template)]
#[template(path = "text.html")]
struct StatusTextTemplate<'a> {
    status: &'a Status,
    path: &'a str,
    parts: &'a URLParts,
    display_name: &'a String,
    content: &'a String,
}

#[derive(Template)]
#[template(path = "error.html")]
struct ErrorTemplate<'a> {
    content: &'a String,
    path: &'a str,
}

#[derive(Template)]
#[template(path = "no_url.html")]
struct BareTemplate {
}


#[tokio::main]
async fn main() {
    tokio::select! {
        _ = serve_page() => {

        }
    }
}

async fn serve_page() {
    let mut server = Server::new(|request| -> Response {
        let path = request.url().path().to_string();
        let mut path = path.chars();
        path.next();
        let path = path.as_str();
        if path == "favicon.ico" {
            return Response::builder(OxStatus::OK).with_body("");
        }

        let rt = Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            let content: String = async {
                if path == "" {
                    let temp = BareTemplate{};
                    return temp.render().unwrap();
                };
                let parts = match dissect_url(&path).await {
                    Ok(a) => a,
                    Err(err) => {
                        let temp = ErrorTemplate{
                            content: &err.to_string(),
                            path,
                        };
                        return temp.render().unwrap();
                    }
                };
                let s = &status_from_url(&parts).await;
                let s = match s {
                    Ok(b) => &b.json,
                    Err(err) => {
                        let temp = ErrorTemplate{
                            content: &err.to_string(),
                            path,
                        };
                        return temp.render().unwrap();
                    }
                };

                let display_name = &(EMOTE_REGEX.replace_all(&s.account.display_name, "").to_string());

                let post_content = &s.content;
                let post_content = &(post_content.replace("\"", ""));
                let post_content = &(HTML_REGEX.replace_all(post_content, "").to_string());
                let post_content = &(EMOTE_REGEX.replace_all(post_content, "").to_string());

                match s.media_attachments.get(0) {

                    Some(b) => {
                        let (mut media_type, mut mime_type): (&str, &str) = ("", "");

                        let (media_width, media_height) = match &b.meta {
                            Some(a) => {
                                match &a.original {
                                    Some(a) => {
                                        let media_width = match a.width {
                                            Some(a) => a,
                                            None => 1024,
                                        };
                                        let media_height = match a.height {
                                            Some(a) => a,
                                            None => 64,
                                        };
                                        (media_width, media_height)
                                    },
                                    None => {
                                        let media_width = match a.width {
                                            Some(a) => a,
                                            None => 1024,
                                        };
                                        let media_height = match a.height {
                                            Some(a) => a,
                                            None => 64,
                                        };
                                        (media_width, media_height)
                                    },
                                }
                            }
                            None => (64, 64)
                        };
                        let media = &b.url;
                        if media.ends_with(".mp4") {
                            media_type = "video";
                            mime_type = "mp4";
                        } else if media.ends_with(".webm") {
                            media_type = "video";
                            mime_type = "webm";
                        } else {
                            media_type = "image";
                        }

                        let temp = StatusImageTemplate {
                            status: &s,
                            path: &path.replace(":/","://"),
                            parts: &parts,
                            display_name,
                            content: post_content,
                            media,
                            media_width,
                            media_height: media_height,
                            media_type: media_type,
                            mime_type: mime_type,
                        };
                        return temp.render().unwrap();
                    }
                    None => {
                        let temp = StatusTextTemplate {
                            status: &s,
                            path: &path.replace(":/","://"),
                            parts: &parts,
                            display_name: display_name,
                            content: post_content,
                        };
                        return temp.render().unwrap();
                    }
                };
            }.await;

            Response::builder(OxStatus::OK)
            .with_header(HeaderName::CONTENT_TYPE, "text/html")
            .unwrap()
            .with_body(
                content
            )
        })
    });
    server.set_global_timeout(Duration::from_secs(10));
    server.listen(("localhost", 8087)).unwrap();

}

#[derive(Clone)]
struct URLParts {
    //protocol: String,
    instance: String,
    id: String,
    base_url: String,
}

async fn dissect_url(url: &str) -> Result<URLParts, String> {
    let captures: Captures = match URL_REGEX.captures(url) {
        Some(a) => a,
        None => {
            return Err(format!("{} does not match the url regex.",url));
        }
    };
    let mut parts: Vec<&str> = Vec::new();
    let mut i = 0;
    for capture in captures.iter() {
        match capture {
            Some(a) => {
                if i != 0 {
                    let s = a.as_str();
                    if s.contains("/") {
                        let parts_ = s.split("/");
                        for part in parts_ {
                            if part != "" {
                                parts.push(part);
                            }
                        }
                    } else {
                        parts.push(s);
                    }
                }
                i += 1;
            }
            None => {
                parts.push("");
            }
        }
    }
    if parts.len() <= 3 {
        return Err(format!("invalid url {}. must have a protocol, instance, an id at the end.",url));
    }

    // we expect the following structure:
    let protocol = parts.get(0).unwrap().to_string(); // protocol
    let instance = parts.get(2).unwrap().to_string(); // instance name
    let id = parts.get(parts.len()-1).unwrap().to_string(); // post id

    let base_url = String::from(format!("{}://{}",protocol,instance));

    Ok(URLParts{
        //protocol,
        instance,
        id,
        base_url,
    })
}

async fn status_from_url(parts: &URLParts) -> Result<MegResponse<Status>, String> {
    let parts = parts.clone();
    let (id, base_url) = (parts.id, parts.base_url);

    let instance_type: SNS = match detector(&base_url).await {
        Ok(a) => a,
        Err(err) => {
            if err.to_string().ends_with("line 1 column 1") {
                megalodon::SNS::Misskey
            } else {
                return Err(err.to_string());
            }
        }
    };
    match instance_type {
        megalodon::SNS::Misskey => {
            return Err(String::from("misskey is not yet supported sorry."));
        }
        _ => {

        }
    }

    let client = megalodon::generator(
        instance_type,
        base_url,
        None,
        None
    );

    let status = client.get_status(id).await;

    match status {
        Ok(a) => Ok(a),
        Err(err) => Err(format!("{}",err))
    }

}
