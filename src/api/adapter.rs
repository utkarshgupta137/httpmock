use crate::server::data::{ActiveMock, MockDefinition, MockIdentification, MockServerState};
use hyper::body::Bytes;
use hyper::{Body, Error, Method as HyperMethod, Request, StatusCode};
use std::cell::RefCell;
use std::fmt::Debug;
use std::sync::Arc;
use crate::server::handlers::{add_new_mock, read_one, delete_one, delete_all};

thread_local!(
    static TOKIO_RUNTIME: RefCell<tokio::runtime::Runtime> = {
        let runtime = tokio::runtime::Builder::new()
            .enable_all()
            .basic_scheduler()
            .build()
            .expect("Cannot build thread local tokio tuntime");
        RefCell::new(runtime)
    };
);
/// Refer to [regex::Regex](../regex/struct.Regex.html).
pub type Regex = regex::Regex;

/// Represents an HTTP method.
#[derive(Debug)]
pub enum Method {
    GET,
    HEAD,
    POST,
    PUT,
    DELETE,
    CONNECT,
    OPTIONS,
    TRACE,
    PATCH,
}

pub(crate) trait MockServerAdapter {
    fn server_port(&self) -> u16;
    fn server_host(&self) -> String;
    fn server_address(&self) -> String;
    fn create_mock(&self, mock: &MockDefinition) -> Result<MockIdentification, String>;
    fn fetch_mock(&self, mock_id: usize) -> Result<ActiveMock, String>;
    fn delete_mock(&self, mock_id: usize) -> Result<(), String>;
    fn delete_all_mocks(&self) -> Result<(), String>;
}

/// This adapter allows to access the servers management functionality.
///
/// You can create an adapter by calling `ServerAdapter::from_env` to create a new instance.
/// You should never actually need to use this adapter, but you certainly can, if you absolutely
/// need to.
#[derive(Debug)]
pub struct RemoteMockServerAdapter {
    pub(crate) host: String,
    pub(crate) port: u16,
}

impl RemoteMockServerAdapter {
    pub(crate) fn new(host: String, port: u16) -> RemoteMockServerAdapter {
        RemoteMockServerAdapter { host, port }
    }
}

impl MockServerAdapter for RemoteMockServerAdapter {
    fn server_port(&self) -> u16 {
        self.port
    }

    fn server_host(&self) -> String {
        self.host.to_string()
    }

    fn server_address(&self) -> String {
        format!("{}:{}", self.server_host(), self.server_port())
    }

    fn create_mock(&self, mock: &MockDefinition) -> Result<MockIdentification, String> {
        // Serialize to JSON
        let json = serde_json::to_string(mock);
        if let Err(err) = json {
            return Err(format!("cannot serialize mock object to JSON: {}", err));
        }
        let json = json.unwrap();

        // Send the request to the mock server
        let request_url = format!("http://{}/__mocks", &self.server_address());

        let request = Request::builder()
            .method(HyperMethod::POST)
            .uri(request_url)
            .header("Content-Type", "application/json")
            .body(Body::from(json))
            .expect("Cannot build request");

        let response = execute_request(request);
        if let Err(err) = response {
            return Err(format!("cannot send request to mock server: {}", err));
        }

        let (status, body) = response.unwrap();

        // Evaluate the response status
        if status != 201 {
            return Err(format!(
                "could not create mock. Mock server response: status = {}, message = {}",
                status, body
            ));
        }

        // Create response object
        let response: serde_json::Result<MockIdentification> = serde_json::from_str(&body);
        if let Err(err) = response {
            return Err(format!("cannot deserialize mock server response: {}", err));
        }

        return Ok(response.unwrap());
    }

    fn fetch_mock(&self, mock_id: usize) -> Result<ActiveMock, String> {
        // Send the request to the mock server
        let request_url = format!("http://{}/__mocks/{}", &self.server_address(), mock_id);
        let request = Request::builder()
            .method(HyperMethod::GET)
            .uri(request_url)
            .body(Body::empty())
            .expect("Cannot build request");

        let response = execute_request(request);
        if let Err(err) = response {
            return Err(format!("cannot send request to mock server: {}", err));
        }

        let (status, body) = response.unwrap();

        // Evaluate response status code
        if status != 200 {
            return Err(format!(
                "could not create mock. Mock server response: status = {}, message = {}",
                status, body
            ));
        }

        // Create response object
        let response: serde_json::Result<ActiveMock> = serde_json::from_str(&body);
        if let Err(err) = response {
            return Err(format!("cannot deserialize mock server response: {}", err));
        }

        return Ok(response.unwrap());
    }

    fn delete_mock(&self, mock_id: usize) -> Result<(), String> {
        // Send the request to the mock server
        let request_url = format!("http://{}/__mocks/{}", &self.server_address(), mock_id);
        let request = Request::builder()
            .method(HyperMethod::DELETE)
            .uri(request_url)
            .body(Body::empty())
            .expect("Cannot build request");

        let response = execute_request(request);
        if let Err(err) = response {
            return Err(format!("cannot send request to mock server: {}", err));
        }
        let (status, body) = response.unwrap();

        // Evaluate response status code
        if status != 202 {
            return Err(format!(
                "Could not delete mocks from server (status = {}, message = {})",
                status, body
            ));
        }

        return Ok(());
    }

    fn delete_all_mocks(&self) -> Result<(), String> {
        // Send the request to the mock server
        let request_url = format!("http://{}/__mocks", &self.server_address());
        let request = Request::builder()
            .method(HyperMethod::DELETE)
            .uri(request_url)
            .body(Body::empty())
            .expect("Cannot build request");

        let response = execute_request(request);
        if let Err(err) = response {
            return Err(format!("cannot send request to mock server: {}", err));
        }

        let (status, body) = response.unwrap();

        // Evaluate response status code
        if status != 202 {
            return Err(format!(
                "Could not delete mocks from server (status = {}, message = {})",
                status, body
            ));
        }

        return Ok(());
    }
}

pub struct LocalMockServerAdapter {
    pub(crate) host: String,
    pub(crate) port: u16,
    local_state: Arc<MockServerState>,
}

impl LocalMockServerAdapter {
    pub(crate) fn new(host: String, port: u16, local_state: Arc<MockServerState>) -> Self {
        LocalMockServerAdapter { host, port, local_state }
    }
}

impl MockServerAdapter for LocalMockServerAdapter {
    fn server_port(&self) -> u16 {
        self.port
    }

    fn server_host(&self) -> String {
        self.host.to_string()
    }

    fn server_address(&self) -> String {
        format!("{}:{}", self.server_host(), self.server_port())
    }

    fn create_mock(&self, mock: &MockDefinition) -> Result<MockIdentification, String> {
        let id = add_new_mock(&self.local_state, mock.clone())?;
        return Ok(MockIdentification::new(id));
    }

    fn fetch_mock(&self, mock_id: usize) -> Result<ActiveMock, String> {
        return match read_one(&self.local_state, mock_id)? {
            Some(mock) => Ok(mock),
            None => Err("Cannot find mock".to_string())
        };
    }

    fn delete_mock(&self, mock_id: usize) -> Result<(), String> {
        let deleted = delete_one(&self.local_state, mock_id)?;
        return match deleted {
            false => Err("Mock could not deleted".to_string()),
            true => Ok(())
        };
    }

    fn delete_all_mocks(&self) -> Result<(), String> {
        delete_all(&self.local_state)?;
        return Ok(());
    }
}

/// Enables enum to_string conversion
impl std::fmt::Display for Method {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        std::fmt::Debug::fmt(self, f)
    }
}

/// Executes an HTTP request synchronously
fn execute_request(req: Request<Body>) -> Result<(StatusCode, String), Error> {
    return TOKIO_RUNTIME.with(|runtime| {
        let local = tokio::task::LocalSet::new();
        let mut rt = &mut *runtime.borrow_mut();
        return local.block_on(&mut rt, async {
            let client = hyper::Client::new();

            let resp = client.request(req).await.unwrap();
            let status = resp.status();

            let body: Bytes = hyper::body::to_bytes(resp.into_body()).await.unwrap();

            let body_str = String::from_utf8(body.to_vec()).unwrap();

            Ok((status, body_str))
        });
    });
}
