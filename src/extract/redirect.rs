use hyper::header;
use crate::{Body, Response};
use super::IntoResponse;

#[derive(Default)]
pub struct Redirect {
    status: u16,
    location: Option<String>,
    choices: Option<Body>,
}

impl Redirect {
    /// `HTTP 301` Moved Permanently redirect message indicates that the
    /// resource has moved to a new URL that is specified within the HTTP
    /// response.
    ///
    /// Clients and caching servers need to update their internal data to
    /// reflect the new Location and no longer use the original URL.
    ///
    /// Client is allowed to change the request method after redirect.
    pub fn moved_permanently<S: AsRef<str>>(location: S) -> Self {
        Self {
            status: 301,
            location: Some(location.as_ref().to_string()),
            ..Default::default()
        }
    }

    /// `HTTP 308` Permanent Redirect redirect message indicates that the
    /// resource has moved to a new URL that is specified within the HTTP
    /// response with the Location header.
    pub fn permanent_redirect<S: AsRef<str>>(location: S) -> Self {
        Self {
            status: 308,
            location: Some(location.as_ref().to_string()),
            ..Default::default()
        }
    }

    /// `HTTP 302` Found redirect message indicates that the requested
    /// resource has been temporarily moved and that a second, otherwise
    /// identical HTTP request will be made to fetch the resource.
    ///
    /// Client is allowed to change the request method after redirect.
    pub fn found<S: AsRef<str>>(location: S) -> Self {
        Self {
            status: 302,
            location: Some(location.as_ref().to_string()),
            ..Default::default()
        }
    }

    /// `HTTP 303` See Other redirect message indicates that the result
    /// of the HTTP request is viewable at an alternate Location, which can
    /// be accessed using a GET method.
    pub fn see_other<S: AsRef<str>>(location: S) -> Self {
        Self {
            status: 303,
            location: Some(location.as_ref().to_string()),
            ..Default::default()
        }
    }

    /// `HTTP 307` Temporary Redirect message indicates that the result
    /// of the HTTP request is viewable at an alternate Location, and it
    /// will be accessed using the same HTTP request method used for the
    /// original HTTP request.
    pub fn temporary<S: AsRef<str>>(location: S) -> Self {
        Self {
            status: 307,
            location: Some(location.as_ref().to_string()),
            ..Default::default()
        }
    }

    /// `HTTP 300` Multiple Choices is returned by the server to
    /// indicate that more than one HTTP response is available as a result
    /// of the HTTP request. It indicates success, although it is the
    /// client’s responsibility to select which of the presented options
    /// best fits their requirements.
    pub fn multiple_choices<S: Into<Body>>(body: S) -> Self {
        Self {
            status: 300,
            choices: Some(body.into()),
            ..Default::default()
        }
    }

    /// `HTTP 304` Not Modified is returned by the server to
    /// indicate a successful HTTP request, yet the client already
    /// has the most recent version of the resource stored in its cache.
    /// The HTTP redirection, in this context, is redirected to the
    /// client’s internal cache.
    pub fn not_modified() -> Self {
        Self {
            status: 300,
            ..Default::default()
        }
    }
}

impl IntoResponse for Redirect {
    fn into_response(self) -> Response {
        let mut response = Response::builder().status(self.status);
        if let Some(location) = self.location {
            response = response.header(header::LOCATION, location);
        }
        response.body(self.choices.unwrap_or(Body::empty()))
        .unwrap()
    }
}
