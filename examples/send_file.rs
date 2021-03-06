#![deny(warnings)]
extern crate futures;
extern crate hyper;
extern crate pretty_env_logger;

use futures::{future, Future};
use futures::sync::oneshot;

use hyper::{Body, Method, Request, Response, Server, StatusCode};
use hyper::service::service_fn;

use std::fs::File;
use std::io::{self, copy/*, Read*/};
use std::thread;

static NOTFOUND: &[u8] = b"Not Found";
static INDEX: &str = "examples/send_file_index.html";


fn main() {
    pretty_env_logger::init();

    let addr = "127.0.0.1:1337".parse().unwrap();

    let server = Server::bind(&addr)
        .serve(|| service_fn(response_examples))
        .map_err(|e| eprintln!("server error: {}", e));

    println!("Listening on http://{}", addr);

    hyper::rt::run(server);
}

type ResponseFuture = Box<Future<Item=Response<Body>, Error=io::Error> + Send>;

fn response_examples(req: Request<Body>) -> ResponseFuture {
    match (req.method(), req.uri().path()) {
        (&Method::GET, "/") | (&Method::GET, "/index.html") => {
            simple_file_send(INDEX)
        },
        (&Method::GET, "/big_file.html") => {
            // Stream a large file in chunks. This requires a
            // little more overhead with two channels, (one for
            // the response future, and a second for the response
            // body), but can handle arbitrarily large files.
            //
            // We use an artificially small buffer, since we have
            // a small test file.
            let (tx, rx) = oneshot::channel();
            thread::spawn(move || {
                let _file = match File::open(INDEX) {
                    Ok(f) => f,
                    Err(_) => {
                        tx.send(Response::builder()
                                .status(StatusCode::NOT_FOUND)
                                .body(NOTFOUND.into())
                                .unwrap())
                            .expect("Send error on open");
                        return;
                    },
                };
                let (_tx_body, rx_body) = Body::channel();
                let res = Response::new(rx_body.into());
                tx.send(res).expect("Send error on successful file read");
                /* TODO: fix once we have futures 0.2 Sink working
                let mut buf = [0u8; 16];
                loop {
                    match file.read(&mut buf) {
                        Ok(n) => {
                            if n == 0 {
                                // eof
                                tx_body.close().expect("panic closing");
                                break;
                            } else {
                                let chunk: Chunk = buf[0..n].to_vec().into();
                                match tx_body.send_data(chunk).wait() {
                                    Ok(t) => { tx_body = t; },
                                    Err(_) => { break; }
                                };
                            }
                        },
                        Err(_) => { break; }
                    }
                }
                */
            });

            Box::new(rx.map_err(|e| io::Error::new(io::ErrorKind::Other, e)))
        },
        (&Method::GET, "/no_file.html") => {
            // Test what happens when file cannot be be found
            simple_file_send("this_file_should_not_exist.html")
        },
        _ => {
            Box::new(future::ok(Response::builder()
                                .status(StatusCode::NOT_FOUND)
                                .body(Body::empty())
                                .unwrap()))
        }
    }

}

fn simple_file_send(f: &str) -> ResponseFuture {
    // Serve a file by reading it entirely into memory. As a result
    // this is limited to serving small files, but it is somewhat
    // simpler with a little less overhead.
    //
    // On channel errors, we panic with the expect method. The thread
    // ends at that point in any case.
    let filename = f.to_string(); // we need to copy for lifetime issues
    let (tx, rx) = oneshot::channel();
    thread::spawn(move || {
        let mut file = match File::open(filename) {
            Ok(f) => f,
            Err(_) => {
                tx.send(Response::builder()
                        .status(StatusCode::NOT_FOUND)
                        .body(NOTFOUND.into())
                        .unwrap())
                    .expect("Send error on open");
                return;
            },
        };
        let mut buf: Vec<u8> = Vec::new();
        match copy(&mut file, &mut buf) {
            Ok(_) => {
                let res = Response::new(buf.into());
                tx.send(res).expect("Send error on successful file read");
            },
            Err(_) => {
                tx.send(Response::builder()
                        .status(StatusCode::INTERNAL_SERVER_ERROR)
                        .body(Body::empty())
                        .unwrap())
                    .expect("Send error on error reading file");
            },
        };
    });

    Box::new(rx.map_err(|e| io::Error::new(io::ErrorKind::Other, e)))
}

