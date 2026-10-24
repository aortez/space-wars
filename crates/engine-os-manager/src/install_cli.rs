//! Explicit manual consent over the ordinary install socket. No native writer,
//! operator bypass, caller-selected socket or automatic retry lives here.
use clap::{Args, Subcommand};
use spacewars_control::install::Id;

#[derive(Subcommand)]
pub enum Command {
    /// Read installation status without checking, downloading or installing.
    Status,
    /// Request the current verified offer without interrupting the app.
    Offer,
    /// Confirm one exact offer and its restart consequence; does not await success.
    Confirm {
        #[command(flatten)]
        ticket: TicketArgs,
        /// Explicitly approve restarting the device to install this candidate.
        #[arg(long, required = true)]
        restart: bool,
        /// Also approve ending the current running or paused game.
        #[arg(long)]
        end_game: bool,
    },
    /// Cancel this exact ticket before its consent is consumed.
    Cancel {
        #[command(flatten)]
        ticket: TicketArgs,
    },
}

#[derive(Args)]
pub struct TicketArgs {
    /// Service incarnation from the offer's ticket.service.
    #[arg(long, value_parser = parse_id)]
    service: Id,
    /// Offer sequence from ticket.sequence.
    #[arg(long, value_parser = clap::value_parser!(u64).range(1..))]
    sequence: u64,
    /// Exact signed candidate identity from ticket.candidate.
    #[arg(long, value_parser = parse_id)]
    candidate: Id,
}

fn parse_id(value: &str) -> std::result::Result<Id, String> {
    Id::try_from(value.to_owned())
        .map_err(|_| "expected a nonzero, 64-character lowercase hexadecimal identity".into())
}

#[cfg(target_os = "linux")]
mod native {
    use super::*;
    use engine_os_manager::{Error, Result};
    use spacewars_control::install::{Consent, Message, SCHEMA, Ticket, transport, wire::*};
    use std::{path::Path, time::Duration};

    impl Command {
        pub(super) fn request(self) -> Result<ServiceRequest> {
            let request = match self {
                Self::Status => ServiceRequest::Status { schema: SCHEMA },
                Self::Offer => ServiceRequest::Offer { schema: SCHEMA },
                Self::Confirm {
                    ticket,
                    restart: true,
                    end_game,
                } => ServiceRequest::Confirm {
                    schema: SCHEMA,
                    consent: Consent {
                        schema: SCHEMA,
                        ticket: ticket.into_ticket(),
                        end_game,
                    },
                },
                Self::Confirm { .. } => return Err(invalid("confirmation requires --restart")),
                Self::Cancel { ticket } => ServiceRequest::Cancel {
                    schema: SCHEMA,
                    ticket: ticket.into_ticket(),
                },
            };
            request
                .validate()
                .map_err(|_| invalid("invalid installation request"))?;
            Ok(request)
        }
    }
    impl TicketArgs {
        fn into_ticket(self) -> Ticket {
            Ticket {
                service: self.service,
                sequence: self.sequence,
                candidate: self.candidate,
            }
        }
    }

    pub fn run(command: Command) -> Result<()> {
        let request = command.request()?;
        let reply = exchange(
            &request,
            Path::new(SERVICE_SOCKET),
            0,
            Duration::from_secs(4),
        )?;
        println!("{}", serde_json::to_string(&reply)?);
        if let Some(error) = reply.error {
            return Err(invalid(match error {
                Failure::EndGameRequired => {
                    "this offer ends a game; confirmation also requires --end-game"
                }
                Failure::Expired | Failure::Stale => {
                    "this offer expired or changed; inspect a fresh offer before confirming"
                }
                Failure::TooLate => {
                    "consent was already consumed; inspect install status and wait for recovery"
                }
                Failure::Busy => {
                    "installation service is busy; inspect install status before retrying"
                }
                Failure::NotReady => "no verified candidate or eligible live app is ready",
                Failure::Unavailable => "installation is unavailable on this device",
                Failure::SaveFailed => {
                    "app settings could not be saved; installation did not start"
                }
                Failure::Invalid => "installation service rejected the request",
            }));
        }
        Ok(())
    }

    // Tests use private sockets and their own UID. The command exposes neither
    // override: production always authenticates root at the fixed service path.
    pub(super) fn exchange(
        request: &ServiceRequest,
        path: &Path,
        uid: u32,
        timeout: Duration,
    ) -> Result<ServiceReply> {
        let reply: ServiceReply = transport::exchange(path, uid, None, request, timeout)
            .map_err(|_| invalid("installation request could not be confirmed; use install status before deciding whether to retry"))?;
        if reply.error.is_none() {
            let matches = match request {
                ServiceRequest::Status { .. } => true,
                ServiceRequest::Offer { .. } => reply.offer.is_some(),
                ServiceRequest::Confirm { consent, .. } => {
                    reply.status.ticket.as_ref() == Some(&consent.ticket)
                        && matches!(
                            reply.status.phase,
                            Phase::Preparing
                                | Phase::Acknowledged
                                | Phase::CheckingPayload
                                | Phase::Writing
                                | Phase::Flushing
                                | Phase::ReadingBack
                                | Phase::CheckingFilesystem
                                | Phase::RebootRequested
                                | Phase::RecoveryRequired
                        )
                }
                ServiceRequest::Cancel { ticket, .. } => reply
                    .aborted
                    .as_ref()
                    .is_some_and(|aborted| &aborted.ticket == ticket),
            };
            if !matches {
                return Err(invalid(
                    "installation reply did not match the request; inspect install status",
                ));
            }
        }
        Ok(reply)
    }

    fn invalid(message: &str) -> Error {
        Error::Invalid(message.into())
    }
}

#[cfg(target_os = "linux")]
pub use native::run;

#[cfg(all(test, target_os = "linux"))]
#[path = "install_cli_tests.rs"]
mod tests;
