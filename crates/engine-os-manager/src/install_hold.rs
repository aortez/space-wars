//! The capability is durable before admission and never appears in UI status.
use crate::network_policy::{HoldKind, HoldToken};
use crate::network_service::{self, Request};
use crate::{Error, Result};
use std::time::{Duration, Instant};

fn request(request: Request, admission: bool) -> Result<()> {
    let reply = network_service::request_before(
        network_service::SOCKET_PATH.as_ref(),
        &request,
        0,
        Instant::now() + Duration::from_secs(5),
    )?;
    if !reply.get("error").is_some_and(serde_json::Value::is_null)
        || !reply
            .get("status")
            .is_some_and(serde_json::Value::is_object)
        || (admission
            && !matches!(
                reply
                    .get("availability")
                    .and_then(serde_json::Value::as_str),
                Some("wireless-ready" | "ethernet-ready" | "deployment-protected")
            ))
    {
        return Err(Error::Invalid(
            "installation maintenance protection unavailable".into(),
        ));
    }
    Ok(())
}
pub(super) fn admit(token: &HoldToken) -> Result<()> {
    request(
        Request::Admit {
            schema: 1,
            token: token.expose().into(),
            kind: HoldKind::Deployment,
        },
        true,
    )
}
pub(super) fn renew(token: &HoldToken) -> Result<()> {
    request(
        Request::Renew {
            schema: 1,
            token: token.expose().into(),
        },
        true,
    )
}
pub(super) fn release(token: &HoldToken) -> Result<()> {
    request(
        Request::Release {
            schema: 1,
            token: token.expose().into(),
        },
        false,
    )
}
