use crate::http::{HttpEndpoint, HttpClient};

use std::convert::TryFrom;
use std::convert::TryInto;

/// A simple REST client for requesting resources using HTTP `GET`.
pub struct RestClient {
	endpoint: HttpEndpoint,
	client: HttpClient,
}

impl RestClient {
	/// Creates a new REST client connected to the given endpoint.
	pub fn new(endpoint: HttpEndpoint) -> std::io::Result<Self> {
		let client = HttpClient::connect(&endpoint)?;
		Ok(Self { endpoint, client })
	}

	/// Requests a resource encoded in `F` format and interpreted as type `T`.
	async fn request_resource<F, T>(&mut self, resource_path: &str) -> std::io::Result<T>
	where F: TryFrom<Vec<u8>, Error = std::io::Error> + TryInto<T, Error = std::io::Error> {
		let host = format!("{}:{}", self.endpoint.host(), self.endpoint.port());
		let uri = format!("{}/{}", self.endpoint.path().trim_end_matches("/"), resource_path);
		self.client.get::<F>(&uri, &host).await?.try_into()
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::http::BinaryResponse;
	use crate::http::client_tests::{HttpServer, MessageBody};

	/// Parses binary data as a string-encoded `u32`.
	impl TryInto<u32> for BinaryResponse {
		type Error = std::io::Error;

		fn try_into(self) -> std::io::Result<u32> {
			match std::str::from_utf8(&self.0) {
				Err(e) => Err(std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
				Ok(s) => match u32::from_str_radix(s, 10) {
					Err(e) => Err(std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
					Ok(n) => Ok(n),
				}
			}
		}
	}

	#[tokio::test]
	async fn request_unknown_resource() {
		let server = HttpServer::responding_with_not_found();
		let mut client = RestClient::new(server.endpoint()).unwrap();

		match client.request_resource::<BinaryResponse, u32>("/").await {
			Err(e) => assert_eq!(e.kind(), std::io::ErrorKind::NotFound),
			Ok(_) => panic!("Expected error"),
		}
	}

	#[tokio::test]
	async fn request_malformed_resource() {
		let server = HttpServer::responding_with_ok(MessageBody::Content("foo"));
		let mut client = RestClient::new(server.endpoint()).unwrap();

		match client.request_resource::<BinaryResponse, u32>("/").await {
			Err(e) => assert_eq!(e.kind(), std::io::ErrorKind::InvalidData),
			Ok(_) => panic!("Expected error"),
		}
	}

	#[tokio::test]
	async fn request_valid_resource() {
		let server = HttpServer::responding_with_ok(MessageBody::Content(42));
		let mut client = RestClient::new(server.endpoint()).unwrap();

		match client.request_resource::<BinaryResponse, u32>("/").await {
			Err(e) => panic!("Unexpected error: {:?}", e),
			Ok(n) => assert_eq!(n, 42),
		}
	}
}
