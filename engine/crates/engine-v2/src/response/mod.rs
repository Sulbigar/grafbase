use std::sync::Arc;

pub(crate) use error::GraphqlError;
pub use metadata::*;
pub use path::*;
pub use read::*;
use schema::Schema;
pub use value::{ResponseObject, ResponseValue};
pub use write::*;

pub(crate) mod cacheable;
mod error;
mod metadata;
mod path;
mod read;
mod value;
mod write;

pub enum Response {
    Initial(InitialResponse),
    /// Engine could not execute the request.
    RequestError(RequestErrorResponse),
    /// Engine refused to execute request because the user isn't authenticated/authorized.
    AuthError(AuthErrorResponse),
}

// Our internal error struct shouldn't be accessible. It'll also need some context like
// ResponseKeys to even just present paths correctly.
pub struct Error<'a>(&'a GraphqlError);

pub struct InitialResponse {
    // will be None if an error propagated up to the root.
    data: ResponseData,
    errors: Vec<GraphqlError>,
    metadata: ExecutionMetadata,
}

struct ResponseData {
    schema: Arc<Schema>,
    keys: Arc<ResponseKeys>,
    root: Option<ResponseObjectId>,
    parts: Vec<ResponseDataPart>,
}

pub struct RequestErrorResponse {
    errors: Vec<GraphqlError>,
    metadata: ExecutionMetadata,
}

pub struct AuthErrorResponse {
    errors: Vec<GraphqlError>,
    metadata: ExecutionMetadata,
}

impl Response {
    pub(crate) fn request_error(error: impl Into<GraphqlError>, metadata: ExecutionMetadata) -> Self {
        Self::RequestError(RequestErrorResponse {
            errors: vec![error.into()],
            metadata,
        })
    }

    pub(crate) fn request_errors<E>(errors: impl IntoIterator<Item = E>, metadata: ExecutionMetadata) -> Self
    where
        E: Into<GraphqlError>,
    {
        Self::RequestError(RequestErrorResponse {
            errors: errors.into_iter().map(Into::into).collect(),
            metadata,
        })
    }

    pub(crate) fn auth_error(error: impl Into<GraphqlError>, metadata: ExecutionMetadata) -> Self {
        Self::AuthError(AuthErrorResponse {
            errors: vec![error.into()],
            metadata,
        })
    }

    pub fn errors(&self) -> Vec<Error<'_>> {
        match self {
            Self::Initial(resp) => resp.errors.iter().map(Error).collect(),
            Self::RequestError(resp) => resp.errors.iter().map(Error).collect(),
            Self::AuthError(resp) => resp.errors.iter().map(Error).collect(),
        }
    }

    pub fn metadata(&self) -> &ExecutionMetadata {
        match self {
            Self::Initial(resp) => &resp.metadata,
            Self::RequestError(resp) => &resp.metadata,
            Self::AuthError(resp) => &resp.metadata,
        }
    }

<<<<<<< HEAD
    pub fn take_metadata(self) -> ExecutionMetadata {
        match self {
            Self::Initial(initial) => initial.metadata,
            Self::RequestError(request_error) => request_error.metadata,
        }
=======
    pub fn is_auth_error(&self) -> bool {
        matches!(self, Self::AuthError(_))
>>>>>>> cbe19512 (auth)
    }
}

impl std::fmt::Debug for Response {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Response").finish_non_exhaustive()
    }
}
