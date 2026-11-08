use engine_ota::{
    Channel, Error, ImageHistory, Result, Sha256, download::Outcome, recovery::RecoveryRecord,
};

pub(super) struct Request {
    pub channel: Channel,
    pub retry_payload: Option<Sha256>,
}

impl Request {
    pub fn parse(args: &[String]) -> std::result::Result<Self, &'static str> {
        let usage = "usage: stage_update release|nightly [retry-payload-sha256] (root operator; download only)";
        if !(1..=2).contains(&args.len()) {
            return Err(usage);
        }
        let channel = match args[0].as_str() {
            "release" => Channel::Release,
            "nightly" => Channel::Nightly,
            _ => return Err(usage),
        };
        let retry_payload = args
            .get(1)
            .map(|value| Sha256::try_from(value.clone()).map_err(|_| usage))
            .transpose()?;
        Ok(Self {
            channel,
            retry_payload,
        })
    }

    pub fn history(&self, record: &RecoveryRecord) -> Result<ImageHistory> {
        match self.retry_payload {
            Some(payload) => Ok(record.retry_verification_history(payload)?),
            None => Ok(record.image_history()),
        }
    }

    pub fn check_outcome(&self, outcome: &Outcome) -> Result<()> {
        self.check_ready_payload(match outcome {
            Outcome::Ready { candidate } => Some(candidate.image().compressed.sha256),
            _ => None,
        })
    }

    fn check_ready_payload(&self, ready: Option<Sha256>) -> Result<()> {
        if self.retry_payload.is_some() && self.retry_payload != ready {
            return Err(Error::CandidateChanged);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine_ota::{
        ImageIdentity,
        recovery::{BootBinding, RecoveryError, Slot, SlotImage},
    };
    use serde_json::json;

    fn failed_record(attempts: u8) -> (RecoveryRecord, Sha256) {
        let image = ImageIdentity::parse(include_bytes!(
            "../../../engine-ota/tests/fixtures/image-identity-v1.json"
        ))
        .unwrap();
        let binding = BootBinding {
            device_id: Sha256::digest(b"synthetic-board"),
            layout_id: Sha256::digest(b"synthetic-layout"),
            boot_identity: image.boot_identity,
            hardware_profile: Sha256::digest(b"synthetic-profile"),
        };
        let good = SlotImage {
            slot: Slot::B,
            build_id: "previous-good".into(),
            generation: 12,
            payload: Sha256::digest(b"old"),
            expanded: Sha256::digest(b"old-root"),
            data: image.data,
        };
        let candidate = SlotImage {
            slot: Slot::A,
            build_id: image.build_id,
            generation: 13,
            payload: Sha256::digest(b"failed-payload"),
            expanded: Sha256::digest(b"failed-root"),
            data: image.data,
        };
        let candidate_id = Sha256::digest(b"candidate");
        let origin = "11111111-1111-1111-1111-111111111111";
        let transaction =
            Sha256::digest(&serde_json::to_vec(&(9, candidate_id, origin, &binding)).unwrap());
        let record: RecoveryRecord = serde_json::from_value(json!({
            "schema":2, "binding":binding, "good":good,
            "retained_good":null, "good_boot":null, "highest_attempted_generation":13,
            "sequence":9,
            "failures":{(candidate.payload.to_string()):{
                "generation":13,"attempts":attempts,"reason":"health-timeout"}},
            "transaction":{"id":transaction,"candidate_id":candidate_id,
                "previous_good":good,"candidate":candidate,"attempt":attempts,
                "origin_boot_id":origin,"trial_boot_id":"22222222-2222-2222-2222-222222222222",
                "phase":"rolled-back","health":null,"failure":"health-timeout"}
        }))
        .unwrap();
        record.validate().unwrap();
        (record, candidate.payload)
    }

    #[test]
    fn retry_requires_an_explicit_well_formed_payload_argument() {
        for channel in ["release", "nightly"] {
            let ordinary = Request::parse(&[channel.into()]).unwrap();
            assert!(ordinary.retry_payload.is_none());
            let payload = Sha256::digest(b"failed-payload");
            let retry = Request::parse(&[channel.into(), payload.to_string()]).unwrap();
            assert_eq!(retry.retry_payload, Some(payload));
        }
        for args in [
            vec![],
            vec!["unknown"],
            vec!["nightly", "not-a-digest"],
            vec!["nightly", "", "extra"],
        ] {
            assert!(
                Request::parse(&args.into_iter().map(str::to_owned).collect::<Vec<_>>()).is_err()
            );
        }
    }

    #[test]
    fn retry_view_preserves_durable_attempts_quarantine_and_generation_floor() {
        let (record, payload) = failed_record(1);
        let before = serde_json::to_vec(&record).unwrap();
        let ordinary = Request::parse(&["nightly".into()]).unwrap();
        let history = ordinary.history(&record).unwrap();
        assert_eq!(history.highest_attempted_generation, 13);
        assert!(history.failed_images.contains(&payload));

        let retry = Request::parse(&["nightly".into(), payload.to_string()]).unwrap();
        let history = retry.history(&record).unwrap();
        assert_eq!(history.highest_attempted_generation, 12);
        assert!(!history.failed_images.contains(&payload));
        assert_eq!(serde_json::to_vec(&record).unwrap(), before);

        let (exhausted, _) = failed_record(2);
        assert!(matches!(
            retry.history(&exhausted),
            Err(Error::Recovery(RecoveryError::Quarantined))
        ));
        let wrong =
            Request::parse(&["nightly".into(), Sha256::digest(b"other").to_string()]).unwrap();
        assert!(matches!(
            wrong.history(&record),
            Err(Error::Recovery(RecoveryError::Quarantined))
        ));
    }

    #[test]
    fn retry_cannot_report_success_for_missing_or_different_payload() {
        let payload = Sha256::digest(b"failed-payload");
        let retry = Request::parse(&["nightly".into(), payload.to_string()]).unwrap();
        retry.check_ready_payload(Some(payload)).unwrap();
        assert!(matches!(
            retry.check_ready_payload(Some(Sha256::digest(b"different"))),
            Err(Error::CandidateChanged)
        ));
        assert!(matches!(
            retry.check_outcome(&Outcome::NoCandidate),
            Err(Error::CandidateChanged)
        ));
        Request::parse(&["nightly".into()])
            .unwrap()
            .check_outcome(&Outcome::NoCandidate)
            .unwrap();
    }
}
