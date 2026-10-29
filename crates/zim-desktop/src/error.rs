//! Conversions into the shared [`CommandError`], so commands can `?`.

use zim::daemon::api::client::ApiError;
use zim_api::desktop::CommandError;

pub type CmdResult<T> = Result<T, CommandError>;

pub fn from_api(e: ApiError) -> CommandError {
    match e {
        ApiError::HttpStatus(status, message) => CommandError::Daemon {
            status: status.as_u16(),
            message,
        },
        other => CommandError::Unreachable(other.to_string()),
    }
}
