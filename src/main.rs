#![feature(async_closure)]
#[allow(unused_variables)]

use lazy_static::lazy_static;
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
        &r"(https?):\/\/?(([-a-zA-Z0-9@:%._\+~#=]{1,256}\.?){1,6}\b)([-a-zA-Z0-9()@:%_\+.~#?&\/\/=]*)"
    .replace(" ", "")).unwrap();

    pub static ref LETTERS_REGEX: Regex = Regex::new(
        r"([A-z]*)"
    ).unwrap();
);


#[derive(Template)]
#[template(path = "image.html")]
struct StatusImageTemplate<'a> {
    status: &'a Status,
    content: &'a String,
    media: String,
    media_width: u32,
    media_height: u32,
}

#[derive(Template)]
#[template(path = "text.html")]
struct StatusTextTemplate<'a> {
    status: &'a Status,
    content: &'a String,

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
        let mut content = String::from("");
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

            if path == "" {
                let temp = BareTemplate{};
                content = temp.render().unwrap();
            } else {
                let s = &status_from_url(&path).await.unwrap().json;
                let none = &"".to_string();
                let post_content = match &s.plain_content {
                    Some(a) => a,
                    None => none,
                };
                match s.media_attachments.get(0) {
                    Some(a) => {
                        let (media_width, media_height) = match &a.meta {
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
                        let temp = StatusImageTemplate {
                            status: s,
                            content: post_content,
                            media: a.url.clone(),
                            media_width: media_width,
                            media_height: media_height,
                        };
                        content = temp.render().unwrap();
                    }
                    None => {
                        let temp = StatusTextTemplate {
                            status: s,
                            content: post_content,
                        };
                        content = temp.render().unwrap();
                    }
                }

            }
        });

        Response::builder(OxStatus::OK)
        .with_header(HeaderName::CONTENT_TYPE, "text/html")
        .unwrap()
        .with_body(
            content
        )
    });
    server.set_global_timeout(Duration::from_secs(10));
    server.listen(("localhost", 8087)).unwrap();
}


async fn status_from_url(url: &str) -> Result<MegResponse<Status>, String> {
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

    println!("\n{}\n",id);
    let instance_type: SNS;
    // is there any letters in the id?
    if LETTERS_REGEX.is_match(&id) {
        // how many characters?
        if id.len() <= 10 {
            // pisskey
            instance_type = megalodon::SNS::Misskey;
        } else {
            // pleroma
            instance_type = megalodon::SNS::Pleroma;
        }
    } else {
        // mastodon
        instance_type = megalodon::SNS::Mastodon;
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
