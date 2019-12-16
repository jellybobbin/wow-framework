use std::ops::Index;
use futures::future::{self,Future};
use hyper::{Response, Body, Request, Method, StatusCode,Error};
use std::collections::BTreeMap;
use crate::routing::tree::Node;
use std::path::Path;
use crate::routing::path::clean_path;

/// Param is a single URL parameter, consisting of a key and a value.
#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub key: String,
    pub value: String,
}

impl Param {
    pub fn new(key: &str, value: &str) -> Param {
        Param {
            key: key.to_string(),
            value: value.to_string(),
        }
    }
}

/// Params is a Param-slice, as returned by the Route.
/// The slice is ordered, the first URL parameter is also the first slice value.
/// It is therefore safe to read values by the index.
#[derive(Debug, PartialEq)]
pub struct Params(pub Vec<Param>);

impl Params {
    /// ByName returns the value of the first Param which key matches the given name.
    /// If no matching Param is found, an empty string is returned.
    pub fn get(&self, name: &str) -> Option<&str> {
        match self.0.iter().find(|param| param.key == name) {
            Some(param) => Some(&param.value),
            None => None,
        }
    }

    /// Empty `Params`
    pub fn new() -> Params {
        Params(Vec::new())
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn push(&mut self, p: Param) {
        self.0.push(p);
    }
}

impl Index<usize> for Params {
    type Output = str;

    fn index(&self, i: usize) -> &Self::Output {
        &(self.0)[i].value
    }
}

pub type BoxFut = Box<dyn Future<Output = Result<Response<Body>,Error>> + Send>;

pub trait Handle {
    fn handle(&self, req: Request<Body>, ps: Params) -> BoxFut;
}

impl<F> Handle for F
    where
        F: Fn(Request<Body>, Params) -> BoxFut,
{
    fn handle(&self, req: Request<Body>, ps: Params) -> BoxFut {
        (*self)(req, ps)
    }
}

pub type Handler = Box<dyn Handle + Send + Sync>;



#[allow(dead_code)]
pub struct Route<T> {
    pub trees: BTreeMap<String, Node<T>>,

    // Enables automatic redirection if the current route can't be matched but a
    // handler for the path with (without) the trailing slash exists.
    // For example if /foo/ is requested but a route only exists for /foo, the
    // client is redirected to /foo with http status code 301 for GET requests
    // and 307 for all other request methods.
    pub redirect_trailing_slash: bool,

    // If enabled, the Route tries to fix the current request path, if no
    // handle is registered for it.
    // First superfluous path elements like ../ or // are removed.
    // Afterwards the Route does a case-insensitive lookup of the cleaned path.
    // If a handle can be found for this route, the Route makes a redirection
    // to the corrected path with status code 301 for GET requests and 307 for
    // all other request methods.
    // For example /FOO and /..//Foo could be redirected to /foo.
    // RedirectTrailingSlash is independent of this option.
    pub redirect_fixed_path: bool,

    // If enabled, the Route checks if another method is allowed for the
    // current route, if the current request can not be routed.
    // If this is the case, the request is answered with 'Method Not Allowed'
    // and HTTP status code 405.
    // If no other Method is allowed, the request is delegated to the NotFound
    // handler.
    pub handle_method_not_allowed: bool,

    // If enabled, the Route automatically replies to OPTIONS requests.
    // Custom OPTIONS handlers take priority over automatic replies.
    pub handle_options: bool,

    // Configurable handler which is called when no matching route is
    // found.
    pub not_found: Option<T>,

    // Configurable handler which is called when a request
    // cannot be routed and HandleMethodNotAllowed is true.
    // The "Allow" header with allowed request methods is set before the handler
    // is called.
    pub method_not_allowed: Option<T>,

    // Function to handle panics recovered from http handlers.
    // It should be used to generate a error page and return the http error code
    // 500 (Internal Server Error).
    // The handler can be used to keep your server from crashing because of
    // unrecovered panics.
    pub panic_handler: Option<T>,
}

impl<T> Route<T> {
    /// New returns a new initialized Route.
    /// Path auto-correction, including trailing slashes, is enabled by default.
    pub fn new() -> Route<T> {
        Route {
            trees: BTreeMap::new(),
            redirect_trailing_slash: true,
            redirect_fixed_path: true,
            handle_method_not_allowed: true,
            handle_options: true,
            not_found: None,
            method_not_allowed: None,
            panic_handler: None,
        }
    }

    /// get is a shortcut for Route.handle("GET", path, handle)
    pub fn get(&mut self, path: &str, handle: T) {
        self.handle("GET", path, handle);
    }

    /// head is a shortcut for Route.handle("HEAD", path, handle)
    pub fn head(&mut self, path: &str, handle: T) {
        self.handle("HEAD", path, handle);
    }

    /// options is a shortcut for Route.handle("OPTIONS", path, handle)
    pub fn options(&mut self, path: &str, handle: T) {
        self.handle("OPTIONS", path, handle);
    }

    /// post is a shortcut for Route.handle("POST", path, handle)
    pub fn post(&mut self, path: &str, handle: T) {
        self.handle("POST", path, handle);
    }

    /// put is a shortcut for Route.handle("PUT", path, handle)
    pub fn put(&mut self, path: &str, handle: T) {
        self.handle("PUT", path, handle);
    }

    /// patch is a shortcut for Route.handle("PATCH", path, handle)
    pub fn patch(&mut self, path: &str, handle: T) {
        self.handle("PATCH", path, handle);
    }

    /// delete is a shortcut for Route.handle("DELETE", path, handle)
    pub fn delete(&mut self, path: &str, handle: T) {
        self.handle("DELETE", path, handle);
    }

    /// Unimplemented. Perhaps something like
    ///
    /// # Example
    ///
    /// ```ignore
    /// Route.group(vec![middelware], |Route| {
    ///     Route.get("/something", somewhere);
    ///     Route.post("/something", somewhere);
    /// })
    /// ```
    pub fn group() {
        unimplemented!()
    }

    /// Handle registers a new request handle with the given path and method.
    ///
    /// For GET, POST, PUT, PATCH and DELETE requests the respective shortcut
    /// functions can be used.
    ///
    /// This function is intended for bulk loading and to allow the usage of less
    /// frequently used, non-standardized or custom methods (e.g. for internal
    /// communication with a proxy).
    pub fn handle(&mut self, method: &str, path: &str, handle: T) {
        if !path.starts_with("/") {
            panic!("path must begin with '/' in path '{}'", path);
        }

        self.trees
            .entry(method.to_string())
            .or_insert(Node::new())
            .add_route(path, handle);
    }

    /// Lookup allows the manual lookup of a method + path combo.
    ///
    /// This is e.g. useful to build a framework around this Route.
    ///
    /// If the path was found, it returns the handle function and the path parameter
    /// values. Otherwise the third return value indicates whether a redirection to
    /// the same path with an extra / without the trailing slash should be performed.
    pub fn lookup(&mut self, method: &str, path: &str) -> (Option<&T>, Params, bool) {
        self.trees
            .get_mut(method)
            .and_then(|n| Some(n.get_value(path)))
            .unwrap_or((None, Params::new(), false))
    }

    pub fn allowed(&self, path: &str, req_method: &str) -> String {
        let mut allow = String::new();
        if path == "*" {
            for method in self.trees.keys() {
                if method == "OPTIONS" {
                    continue;
                }

                if allow.is_empty() {
                    allow.push_str(method);
                } else {
                    allow.push_str(", ");
                    allow.push_str(method);
                }
            }
        } else {
            for method in self.trees.keys() {
                if method == req_method || method == "OPTIONS" {
                    continue;
                }

                self.trees.get(method).map(|tree| {
                    let (handle, _, _) = tree.get_value(path);

                    if handle.is_some() {
                        if allow.is_empty() {
                            allow.push_str(method);
                        } else {
                            allow.push_str(", ");
                            allow.push_str(method);
                        }
                    }
                });
            }
        }

        if allow.len() > 0 {
            allow += ", OPTIONS";
        }

        allow
    }
}



impl Route<Handler> {
    /*pub fn serve_files(&mut self, path: &str, root: &'static str) {
        if path.as_bytes().len() < 10 || &path[path.len() - 10..] != "*//*filepath" {
            panic!("path must end with *//*filepath in path '{}'", path);
        }
        let root_path = Path::new(root);
        let get_files = move |_, ps: Params| -> BoxFut {
            let filepath = ps.get("filepath").unwrap();
            simple_file_send(root_path.join(&filepath[1..]).to_str().unwrap())
        };

        self.get(path, Box::new(get_files));
    }*/

    pub fn serve_http(&self, req: Request<Body>) -> BoxFut {
        let root = self.trees.get(req.method().as_str());
        if let Some(root) = root {
            let (handle, ps, tsr) = root.get_value(req.uri().path());

            if let Some(handle) = handle {
                // return handle(req, response, ps);
                return handle.handle(req, ps);
            } else if req.method() != &Method::CONNECT && req.uri().path() != "/" {
                let code = if req.method() != &Method::GET {
                    // StatusCode::from_u16(307).unwrap()
                    307
                } else {
                    // StatusCode::from_u16(301).unwrap()
                    301
                };

                if tsr && self.redirect_trailing_slash {
                    let path = if req.uri().path().len() > 1 && req.uri().path().ends_with("/") {
                        req.uri().path()[..req.uri().path().len() - 1].to_string()
                    } else {
                        req.uri().path().to_string() + "/"
                    };

                    // response.headers_mut().insert(header::LOCATION, header::HeaderValue::from_str(&path).unwrap());
                    // *response.status_mut() = code;
                    let response = Response::builder()
                        .header("Location", path.as_str())
                        .status(code)
                        .body(Body::empty())
                        .unwrap();
                    return Box::new(future::ok(response));
                }

                if self.redirect_fixed_path {
                    let (fixed_path, found) = root.find_case_insensitive_path(
                        &clean_path(req.uri().path()),
                        self.redirect_trailing_slash,
                    );

                    if found {
                        //  response.headers_mut().insert(header::LOCATION, header::HeaderValue::from_str(&fixed_path).unwrap());
                        // *response.status_mut() = code;
                        let response = Response::builder()
                            .header("Location", fixed_path.as_str())
                            .status(code)
                            .body(Body::empty())
                            .unwrap();
                        return Box::new(future::ok(response));
                    }
                }
            }
        }

        if req.method() == &Method::OPTIONS && self.handle_options {
            let allow = self.allowed(req.uri().path(), req.method().as_str());
            if allow.len() > 0 {
                // *response.headers_mut().get_mut("allow").unwrap() = header::HeaderValue::from_str(&allow).unwrap();
                let response = Response::builder()
                    .header("Allow", allow.as_str())
                    .body(Body::empty())
                    .unwrap();
                return Box::new(future::ok(response));
            }
        } else {
            if self.handle_method_not_allowed {
                let allow = self.allowed(req.uri().path(), req.method().as_str());

                if allow.len() > 0 {
                    let mut response = Response::builder()
                        .header("Allow", allow.as_str())
                        .body(Body::empty())
                        .unwrap();

                    if let Some(ref method_not_allowed) = self.method_not_allowed {
                        return method_not_allowed.handle(req, Params::new());
                    } else {
                        *response.status_mut() = StatusCode::METHOD_NOT_ALLOWED;
                        *response.body_mut() = Body::from("METHOD_NOT_ALLOWED");
                    }

                    return Box::new(future::ok(response));
                }
            }
        }

        // Handle 404
        if let Some(ref not_found) = self.not_found {
            return not_found.handle(req, Params::new());
        } else {
            // *response.status_mut() = StatusCode::NOT_FOUND;
            let response = Response::builder()
                .status(404)
                .body("NOT_FOUND".into())
                .unwrap();
            return Box::new(future::ok(response));
        }
    }
}

/*fn simple_file_send(f: &str) -> BoxFut {
    // Serve a file by asynchronously reading it entirely into memory.
    // Uses tokio_fs to open file asynchronously, then tokio_io to read into
    // memory asynchronously.
    let filename = f.to_string(); // we need to copy for lifetime issues
    Box::new(
        tokio_fs::file::File::open(filename)
            .and_then(|file| {
                let buf: Vec<u8> = Vec::new();
                tokio_io::io::read_to_end(file, buf)
                    .and_then(|item| Ok(Response::new(item.1.into())))
                    .or_else(|_| {
                        Ok(Response::builder()
                            .status(StatusCode::INTERNAL_SERVER_ERROR)
                            .body(Body::empty())
                            .unwrap())
                    })
            })
            .or_else(|_| {
                Ok(Response::builder()
                    .status(StatusCode::NOT_FOUND)
                    .body(Body::from("NOT_FOUND"))
                    .unwrap())
            }),
    )
}*/


