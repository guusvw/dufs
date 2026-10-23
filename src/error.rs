use crate::http_utils::body_full;
use crate::server::Response;

use hyper::header::HeaderValue;
use hyper::StatusCode;
use std::io;

/// Response header carrying the machine-readable error reason.
pub const ERROR_CODE_HEADER: &str = "x-dufs-error-code";

/// Machine-readable reason for an error response.
///
/// Several different situations share the same HTTP status code (for example
/// an account without write permission and a `--allow-upload` disabled
/// server both return 403). Clients can tell them apart through
/// [`ERROR_CODE_HEADER`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DufsErrorCode {
    // 400
    InvalidPath,
    InvalidUploadOffset,
    InvalidDepth,
    InvalidDestination,
    // 403
    Forbidden,
    ReadonlyDescendant,
    UploadDisabled,
    DeleteDisabled,
    HashDisabled,
    PermissionDenied,
    // 404
    ArchiveDisabled,
    // 405
    MissingUploadOffset,
    AlreadyExists,
    MethodNotAllowed,
    // 409
    Conflict,
    IsADirectory,
    // 412
    PreconditionFailed,
    DestinationExists,
    // 500
    InternalError,
    // 507
    InsufficientStorage,
}

impl DufsErrorCode {
    /// Value of the [`ERROR_CODE_HEADER`] response header.
    pub const fn as_str(self) -> &'static str {
        use DufsErrorCode::*;
        match self {
            InvalidPath => "invalid-path",
            InvalidUploadOffset => "invalid-upload-offset",
            InvalidDepth => "invalid-depth",
            InvalidDestination => "invalid-destination",
            Forbidden => "forbidden",
            ReadonlyDescendant => "readonly-descendant",
            UploadDisabled => "upload-disabled",
            DeleteDisabled => "delete-disabled",
            HashDisabled => "hash-disabled",
            PermissionDenied => "permission-denied",
            ArchiveDisabled => "archive-disabled",
            MissingUploadOffset => "missing-upload-offset",
            AlreadyExists => "already-exists",
            MethodNotAllowed => "method-not-allowed",
            PreconditionFailed => "precondition-failed",
            DestinationExists => "destination-exists",
            Conflict => "conflict",
            IsADirectory => "is-a-directory",
            InternalError => "internal-error",
            InsufficientStorage => "insufficient-storage",
        }
    }

    pub const fn status(self) -> StatusCode {
        use DufsErrorCode::*;
        match self {
            InvalidPath | InvalidUploadOffset | InvalidDepth | InvalidDestination => {
                StatusCode::BAD_REQUEST
            }
            Forbidden | ReadonlyDescendant | UploadDisabled | DeleteDisabled | HashDisabled
            | PermissionDenied => StatusCode::FORBIDDEN,
            ArchiveDisabled => StatusCode::NOT_FOUND,
            MissingUploadOffset | AlreadyExists | MethodNotAllowed => {
                StatusCode::METHOD_NOT_ALLOWED
            }
            Conflict | IsADirectory => StatusCode::CONFLICT,
            PreconditionFailed | DestinationExists => StatusCode::PRECONDITION_FAILED,
            InternalError => StatusCode::INTERNAL_SERVER_ERROR,
            InsufficientStorage => StatusCode::INSUFFICIENT_STORAGE,
        }
    }

    /// Default human-readable body. An empty string means no body.
    pub const fn message(self) -> &'static str {
        use DufsErrorCode::*;
        match self {
            InvalidPath => "Invalid Path",
            InvalidUploadOffset => "Invalid X-Update-Range Header",
            InvalidDepth => "Invalid depth: only 0 and 1 are allowed.",
            InvalidDestination => "Invalid Destination",
            Forbidden => "Forbidden",
            ReadonlyDescendant => "Forbidden: the target directory contains read-only entries",
            UploadDisabled => "Forbidden: upload is disabled",
            DeleteDisabled => "Forbidden: delete is disabled",
            HashDisabled => "Forbidden: hash is disabled",
            PermissionDenied => "Forbidden: permission denied by the file system",
            ArchiveDisabled => "Not Found",
            MissingUploadOffset => "",
            AlreadyExists => "Already exists",
            MethodNotAllowed | PreconditionFailed | DestinationExists => "",
            Conflict => "Conflict: a path component is not a directory or already exists",
            IsADirectory => "Conflict: target is a directory",
            InternalError => "Internal Server Error",
            InsufficientStorage => "Insufficient Storage",
        }
    }

    /// Error code for a file system error kind. `None` means the kind has no
    /// dedicated code and is handled by [`respond_io_error`].
    pub fn from_io_kind(kind: io::ErrorKind) -> Option<Self> {
        match kind {
            io::ErrorKind::PermissionDenied => Some(Self::PermissionDenied),
            io::ErrorKind::NotADirectory | io::ErrorKind::AlreadyExists => Some(Self::Conflict),
            io::ErrorKind::IsADirectory => Some(Self::IsADirectory),
            io::ErrorKind::StorageFull | io::ErrorKind::QuotaExceeded => {
                Some(Self::InsufficientStorage)
            }
            _ => None,
        }
    }

    /// Write this error, including its code header and default body, into
    /// `res`.
    pub fn respond(self, res: &mut Response) {
        self.respond_with(res, self.message());
    }

    /// Same as [`Self::respond`], but with a dynamic human-readable body.
    pub fn respond_with(self, res: &mut Response, message: &str) {
        *res.status_mut() = self.status();
        res.headers_mut()
            .insert(ERROR_CODE_HEADER, HeaderValue::from_static(self.as_str()));
        if !message.is_empty() {
            *res.body_mut() = body_full(message.to_string());
        }
    }
}

/// Write the response for an error returned by a request handler.
///
/// The first [`io::Error`] in the error chain decides: kinds with a dedicated
/// [`DufsErrorCode`] get it, `NotFound` is a plain 404 and everything else a
/// plain 500.
pub fn respond_io_error(err: &anyhow::Error, res: &mut Response) {
    let kind = err
        .chain()
        .find_map(|cause| cause.downcast_ref::<io::Error>())
        .map(io::Error::kind);
    match kind {
        Some(io::ErrorKind::NotFound) => {
            *res.status_mut() = StatusCode::NOT_FOUND;
            *res.body_mut() = body_full("Not Found");
        }
        Some(kind) => DufsErrorCode::from_io_kind(kind)
            .unwrap_or(DufsErrorCode::InternalError)
            .respond(res),
        None => DufsErrorCode::InternalError.respond(res),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_are_unique() {
        use DufsErrorCode::*;
        const ALL: [DufsErrorCode; 20] = [
            InvalidPath,
            InvalidUploadOffset,
            InvalidDepth,
            InvalidDestination,
            Forbidden,
            ReadonlyDescendant,
            UploadDisabled,
            DeleteDisabled,
            HashDisabled,
            PermissionDenied,
            ArchiveDisabled,
            MissingUploadOffset,
            AlreadyExists,
            MethodNotAllowed,
            Conflict,
            IsADirectory,
            PreconditionFailed,
            DestinationExists,
            InternalError,
            InsufficientStorage,
        ];
        let mut codes = std::collections::HashSet::new();
        for code in ALL {
            assert!(
                codes.insert(code.as_str()),
                "duplicate code {}",
                code.as_str()
            );
        }
    }
}
