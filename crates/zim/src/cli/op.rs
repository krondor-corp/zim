//! Op trait + `command_enum!` macro.
//!
//! Mirrors jig-cli's pattern, adapted for async. Each CLI command:
//!
//! 1. Declares its own `Context` type (e.g. `ApiContext` for ops
//!    talking to a daemon; `DaemonContext` for `zim daemon`; `()` for
//!    `zim version`).
//! 2. Builds it in `build_context` (errors here surface as
//!    setup-time failures — bad config, no $HOME, etc.).
//! 3. Runs the actual work in `run(ctx)` and returns a `Display`-able
//!    output value.
//!
//! The `command_enum!` macro at the bottom generates the top-level
//! `Command` subcommand enum, an `OpOutput` enum (with a unified
//! `Display`), an `OpError` (with `thiserror`), and a single
//! `impl Op for Command` that dispatches each variant through its
//! own `build_context → run`.

use std::error::Error;
use std::fmt::Display;

use async_trait::async_trait;

#[async_trait]
pub trait Op {
    type Context;
    type Error: Error + Send + Sync + 'static;
    /// Every op's output is both `Display`-able (the pretty terminal
    /// form) and `Serialize`-able (the JSON form emitted under
    /// `--plain`). The two presentations are wholly separate paths in
    /// `main.rs`.
    type Output: Display + serde::Serialize + Send;

    async fn build_context(&self) -> Result<Self::Context, Self::Error>;
    async fn run(&self, ctx: Self::Context) -> Result<Self::Output, Self::Error>;
}

/// `Display`-able zero-output for ops that only produce side effects
/// (e.g. `zim daemon` blocks forever).
#[derive(Debug, Default)]
pub struct NoOutput;

impl Display for NoOutput {
    fn fmt(&self, _f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Ok(())
    }
}

/// Generates the top-level `Command` enum + dispatch.
///
/// Usage:
/// ```ignore
/// command_enum! {
///     (Init,    ops::Init),
///     #[command(subcommand)] (Daemon, ops::Daemon),  // clap attrs → Command variant
///     cfg(debug_assertions): (Clean, ops::Clean),    // gate the whole entry per build profile
///     ...
/// }
/// ```
///
/// An optional leading `cfg(<predicate>):` gates the entry — the
/// generated `Command` variant, its `OpOutput`/`OpError` variants, and
/// both dispatch arms share the `#[cfg(<predicate>)]`, so the command
/// is wholly present or wholly absent. (A bare `cfg(...):` prefix
/// rather than `#[cfg(...)]` keeps it unambiguous from the clap `#[…]`
/// attrs that follow.) Use `cfg(debug_assertions):` for dev-only
/// commands, `cfg(not(debug_assertions)):` for release-only ones.
#[macro_export]
macro_rules! command_enum {
    ($($(cfg($cfg:meta):)? $(#[$attr:meta])* ($variant:ident, $type:ty)),* $(,)?) => {
        #[derive(clap::Subcommand, Debug, Clone)]
        #[allow(clippy::large_enum_variant)]
        pub enum Command {
            $(
                $(#[cfg($cfg)])?
                $(#[$attr])*
                $variant($type),
            )*
        }

        #[derive(Debug, serde::Serialize)]
        #[serde(untagged)]
        #[allow(clippy::large_enum_variant)]
        pub enum OpOutput {
            $(
                $(#[cfg($cfg)])?
                $variant(<$type as $crate::cli::op::Op>::Output),
            )*
        }

        #[derive(Debug, thiserror::Error)]
        pub enum OpError {
            $(
                $(#[cfg($cfg)])?
                #[error(transparent)]
                $variant(<$type as $crate::cli::op::Op>::Error),
            )*
        }

        #[async_trait::async_trait]
        impl $crate::cli::op::Op for Command {
            type Context = ();
            type Output = OpOutput;
            type Error = OpError;

            async fn build_context(&self) -> Result<(), Self::Error> {
                Ok(())
            }

            async fn run(&self, _ctx: ()) -> Result<Self::Output, Self::Error> {
                match self {
                    $(
                        $(#[cfg($cfg)])?
                        Command::$variant(op) => {
                            let ctx = op.build_context().await.map_err(OpError::$variant)?;
                            op.run(ctx)
                                .await
                                .map(OpOutput::$variant)
                                .map_err(OpError::$variant)
                        },
                    )*
                }
            }
        }

        impl std::fmt::Display for OpOutput {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                match self {
                    $(
                        $(#[cfg($cfg)])?
                        OpOutput::$variant(output) => write!(f, "{}", output),
                    )*
                }
            }
        }
    };
}
