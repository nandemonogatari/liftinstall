/// rest.rs
///
/// Provides a HTTP/REST server for both frontend<->backend communication, as well
/// as talking to external applications.

use nfd;
use nfd::Response as NfdResponse;

use serde_json;

use futures::future;
use futures::future::FutureResult;

use hyper;
use hyper::{Get, StatusCode, Error as HyperError};
use hyper::header::{ContentLength, ContentType};
use hyper::server::{Http, Service, Request, Response};

use std::net::{SocketAddr, IpAddr, Ipv4Addr};
use std::thread::{self, JoinHandle};
use std::process::exit;
use std::sync::Arc;
use std::sync::mpsc::channel;

use assets;

use installer::InstallerFramework;

#[derive(Serialize)]
struct FileSelection {
    path : Option<String>
}

/// Encapsulates Hyper's state.
pub struct WebServer {
    handle : JoinHandle<()>,
    addr : SocketAddr
}

impl WebServer {
    /// Returns the bound address that the server is running from.
    pub fn get_addr(&self) -> SocketAddr {
        self.addr.clone()
    }

    /// Creates a new web server, bound to a random port on localhost.
    pub fn new(framework : InstallerFramework) -> Result<Self, HyperError> {
        WebServer::with_addr(framework, SocketAddr::new(
            IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 0))
    }

    /// Creates a new web server with the specified address.
    pub fn with_addr(framework : InstallerFramework, addr : SocketAddr)
        -> Result<Self, HyperError> {
        let (sender, receiver) = channel();

        let handle = thread::spawn(move || {
            let shared_framework = Arc::new(framework);

            let server =
                Http::new().bind(&addr, move ||
                    Ok(WebService {
                        framework : shared_framework.clone()
                    })
                ).unwrap();

            sender.send(server.local_addr().unwrap()).unwrap();

            server.run().unwrap();
        });

        let addr = receiver.recv().unwrap();

        Ok(WebServer {
            handle, addr
        })
    }
}

struct WebService {
    framework : Arc<InstallerFramework>
}

impl Service for WebService {
    type Request = Request;
    type Response = Response;
    type Error = hyper::Error;
    type Future =  FutureResult<Self::Response, Self::Error>;

    fn call(&self, req: Self::Request) -> Self::Future {
        future::ok(match (req.method(), req.path()) {
            // This endpoint should be usable directly from a <script> tag during loading.
            // TODO: Handle errors
            (&Get, "/api/config") => {
                let file = enscapsulate_json("config",
                                             &self.framework.get_config().to_json_str().unwrap());

                Response::<hyper::Body>::new()
                    .with_header(ContentLength(file.len() as u64))
                    .with_header(ContentType::json())
                    .with_body(file)
            },
            (&Get, "/api/file-select") => {
                let file_dialog = nfd::open_pick_folder(None).unwrap();
                let file = match file_dialog {
                    NfdResponse::Okay(path) => Some(path),
                    _ => None
                };

                let response = FileSelection {
                    path : file
                };

                let file = serde_json::to_string(&response).unwrap();

                Response::<hyper::Body>::new()
                    .with_header(ContentLength(file.len() as u64))
                    .with_header(ContentType::json())
                    .with_body(file)
            },
            (&Get, "/api/default-path") => {
                let path = self.framework.get_default_path();

                let response = FileSelection {
                    path
                };

                let file = serde_json::to_string(&response).unwrap();

                Response::<hyper::Body>::new()
                    .with_header(ContentLength(file.len() as u64))
                    .with_header(ContentType::json())
                    .with_body(file)
            },
            (&Get, "/api/exit") => {
                exit(0);
            },

            // Static file handler
            (&Get, _) => {
                // At this point, we have a web browser client. Search for a index page
                // if needed
                let mut path : String = req.path().to_owned();
                if path.ends_with("/") {
                    path += "index.html";
                }

                println!("Trying {} => {}", req.path(), path);

                match assets::file_from_string(&path) {
                    Some((content_type, file)) => Response::<hyper::Body>::new()
                        .with_header(ContentLength(file.len() as u64))
                        .with_header(content_type)
                        .with_body(file),
                    None => Response::new()
                        .with_status(StatusCode::NotFound)
                }
            },
            // Fallthrough for POST/PUT/CONNECT/...
            _ => {
                Response::new().with_status(StatusCode::NotFound)
            }
        })
    }
}

/// Encapsulates JSON as a injectable Javascript script.
fn enscapsulate_json(field_name : &str, json : &str) -> String {
    format!("var {} = {};", field_name, json)
}
