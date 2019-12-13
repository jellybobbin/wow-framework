use std::convert::Infallible;
use std::net::SocketAddr;
use hyper::{Body, Request, Response, Server, Error};
use hyper::service::{make_service_fn, service_fn};
use futures::future::Future;


#[tokio::main]
pub async fn main() -> Result<(),Error>{
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    let make_svc = make_service_fn(|_conn| async {
        // service_fn converts our function into a `Service`
        Ok::<_, Infallible>(service_fn( async move |req:Request<Body>| {
            println!("{:?}",req.uri());
            Ok(Response::new("Hello, World".into()))
        }))
    });
    let server =
        Server::bind(&addr)
        .serve(make_svc);

    if let Err(e) = server.await {
        eprintln!("server error: {}", e);
    };
    Ok(())
}
#[allow(dead_code)]
async fn hello_world(_req: Request<Body>) -> Result<Response<Body>, Infallible> {
    let router = Router::new();
    router.handle(_req)
}

struct Router{}
impl Router{
    fn new() -> Router{
        Router{}
    }
    fn handle(&self,_req: Request<Body>) -> Result<Response<Body>, Infallible>{
        Ok(Response::new("Hello, World".into()))
    }

}