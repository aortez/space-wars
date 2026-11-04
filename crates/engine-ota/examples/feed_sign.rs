//! Trusted publisher utility, never run from a candidate checkout with secrets.
//! Preview emits an approval proposal; a separately admitted digest signs it.
#[cfg(test)]
#[allow(dead_code)]
#[path = "../tests/support/mod.rs"]
mod support;

#[cfg(target_os = "linux")]
mod linux {
    use async_trait::async_trait;
    use engine_ota::{
        MAX_METADATA_BYTES, Sha256, UsableTime,
        signing::{Authority, MAX_REQUEST_BYTES, Request},
    };
    use std::fs::OpenOptions;
    use std::io::{Read, Write};
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
    use std::path::{Path, PathBuf};
    use tough::key_source::KeySource;
    use tough::sign::Sign;

    struct PrivateFile(PathBuf);
    impl std::fmt::Debug for PrivateFile {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("private signing file")
        }
    }
    fn read(
        path: &Path,
        limit: usize,
        private: bool,
    ) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
        let file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(path)?;
        let meta = file.metadata()?;
        // SAFETY: geteuid has no preconditions and no memory arguments.
        if !meta.is_file()
            || (private
                && (meta.uid() != unsafe { libc::geteuid() }
                    || meta.mode() & 0o077 != 0
                    || meta.nlink() != 1))
        {
            return Err("unsafe signing input file".into());
        }
        let mut bytes = Vec::new();
        file.take(limit as u64 + 1).read_to_end(&mut bytes)?;
        if bytes.len() > limit {
            return Err("signing input exceeds size bound".into());
        }
        Ok(bytes)
    }
    #[async_trait]
    impl KeySource for PrivateFile {
        async fn as_sign(&self) -> Result<Box<dyn Sign>, Box<dyn std::error::Error + Send + Sync>> {
            let bytes = read(&self.0, 64 * 1024, true)?;
            Ok(Box::new(tough::sign::parse_keypair(&bytes)?))
        }
        async fn write(
            &self,
            _: &str,
            _: &str,
        ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
            Err("signing files are read-only".into())
        }
    }

    pub async fn run() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let args: Vec<_> = std::env::args_os().skip(1).collect();
        let (output, digest) = execute(&args, || UsableTime(jiff::Timestamp::now())).await?;
        std::io::stdout().write_all(&output)?;
        if let Some(digest) = digest {
            eprintln!("Proposal SHA-256: {digest}");
        }
        Ok(())
    }

    async fn execute(
        args: &[std::ffi::OsString],
        clock: impl Fn() -> UsableTime,
    ) -> Result<(Vec<u8>, Option<Sha256>), Box<dyn std::error::Error + Send + Sync>> {
        if args.len() < 4 {
            return Err("usage: feed_sign TRUSTED_ROOT TRUSTED_TARGETS REQUEST preview | APPROVED_SHA256 PRIVATE_KEY... | verify APPROVED_SHA256 SIGNED_ROLE".into());
        }
        let now = clock();
        let authority = Authority::load(
            &read(Path::new(&args[0]), MAX_METADATA_BYTES, false)?,
            &read(Path::new(&args[1]), MAX_METADATA_BYTES, false)?,
            now,
        )?;
        let request = Request::parse(&read(Path::new(&args[2]), MAX_REQUEST_BYTES, false)?)?;
        let proposal = if args[3] == "verify" {
            authority.prepare_verification(request, now)?
        } else {
            authority.prepare(request, now)?
        };
        if args[3] == "preview" {
            if args.len() != 4 {
                return Err("preview must not receive signing credentials".into());
            }
            Ok((proposal.approval_bytes().to_vec(), Some(proposal.digest())))
        } else if args[3] == "verify" {
            if args.len() != 6 {
                return Err(
                    "verification requires only the approved digest and signed role".into(),
                );
            }
            let approved = Sha256::try_from(args[4].to_str().ok_or("invalid digest")?.to_owned())?;
            let bytes = read(Path::new(&args[5]), MAX_METADATA_BYTES, false)?;
            authority.verify_signed(&proposal, approved, &bytes, clock())?;
            let receipt = serde_json::json!({
                "schema": 1,
                "proposal_sha256": proposal.digest(),
                "signed_sha256": Sha256::digest(&bytes),
            });
            Ok((serde_json::to_vec(&receipt)?, None))
        } else {
            let approved = Sha256::try_from(args[3].to_str().ok_or("invalid digest")?.to_owned())?;
            let keys: Vec<Box<dyn KeySource>> = args[4..]
                .iter()
                .map(|path| Box::new(PrivateFile(PathBuf::from(path))) as Box<dyn KeySource>)
                .collect();
            let signed = authority.sign(&proposal, approved, &keys, clock()).await?;
            Ok((signed, None))
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::os::unix::fs::PermissionsExt;

        #[tokio::test]
        async fn private_credential_reader_accepts_pkcs8_and_rejects_public_or_linked_files() {
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("ephemeral-key");
            let pkcs8 = aws_lc_rs::signature::Ed25519KeyPair::generate_pkcs8(
                &aws_lc_rs::rand::SystemRandom::new(),
            )
            .unwrap();
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&path)
                .unwrap();
            file.write_all(pkcs8.as_ref()).unwrap();
            let original = tough::sign::parse_keypair(pkcs8.as_ref()).unwrap();
            let key = PrivateFile(path.clone());
            assert_eq!(key.as_sign().await.unwrap().tuf_key(), original.tuf_key());
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
            assert!(key.as_sign().await.is_err());
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
            let link = directory.path().join("hardlink");
            std::fs::hard_link(&path, &link).unwrap();
            assert!(key.as_sign().await.is_err());
            std::fs::remove_file(link).unwrap();
            let link = directory.path().join("symlink");
            std::os::unix::fs::symlink(&path, &link).unwrap();
            assert!(PrivateFile(link).as_sign().await.is_err());
            assert!(key.write("replacement", "id").await.is_err());
        }

        #[test]
        fn credential_inputs_are_bounded_and_regular() {
            let directory = tempfile::tempdir().unwrap();
            assert!(read(directory.path(), 1024, true).is_err());
            let path = directory.path().join("oversized-key");
            std::fs::write(&path, vec![0; 64 * 1024 + 1]).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
            assert!(read(&path, 64 * 1024, true).is_err());
        }

        #[tokio::test]
        async fn command_preview_binds_exact_bytes_and_approval_precedes_key_access() {
            use crate::support::{self, Fault, Fixture};
            let fixture = Fixture::new().await;
            let feed = fixture
                .publish(1, &[], support::EXPIRES.parse().unwrap(), Fault::None)
                .await;
            let directory = tempfile::tempdir().unwrap();
            let root = directory.path().join("root.json");
            let targets = directory.path().join("targets.json");
            let request = directory.path().join("request.json");
            std::fs::write(&root, &fixture.root_bytes).unwrap();
            std::fs::write(&targets, &feed.files["1.targets.json"]).unwrap();
            std::fs::write(&request, serde_json::to_vec(&serde_json::json!({
                "prepared_at": support::NOW,
                "change": {"role":"channel", "channel":"nightly", "previous":null, "image":null},
            })).unwrap()).unwrap();
            let mut args: Vec<std::ffi::OsString> = vec![
                root.into(),
                targets.into(),
                request.into(),
                "preview".into(),
            ];
            let (bytes, digest) = execute(&args, || support::time().unwrap()).await.unwrap();
            assert_eq!(Some(Sha256::digest(&bytes)), digest);
            args.push(directory.path().join("absent-private-key").into());
            assert!(
                execute(&args, || support::time().unwrap())
                    .await
                    .unwrap_err()
                    .to_string()
                    .contains("preview must not")
            );
            args[3] = Sha256::digest(b"substituted").to_string().into();
            let error = execute(&args, || support::time().unwrap())
                .await
                .unwrap_err()
                .to_string();
            assert!(error.contains("independently admitted approval"), "{error}");
            args[3] = digest.unwrap().to_string().into();
            let error = execute(&args, || support::time().unwrap())
                .await
                .unwrap_err()
                .to_string();
            assert!(error.contains("credential could not be loaded"), "{error}");
            assert!(!error.contains("absent-private-key"));

            // Exercise the same command path with a real, ephemeral key file.
            // Only its public key is installed into the fixture's signed policy.
            let pkcs8 = aws_lc_rs::signature::Ed25519KeyPair::generate_pkcs8(
                &aws_lc_rs::rand::SystemRandom::new(),
            )
            .unwrap();
            let public = tough::sign::parse_keypair(pkcs8.as_ref())
                .unwrap()
                .tuf_key();
            let id = public.key_id().unwrap();
            let mut top: tough::schema::Signed<tough::schema::Targets> =
                serde_json::from_slice(&feed.files["1.targets.json"]).unwrap();
            let delegations = top.signed.delegations.as_mut().unwrap();
            delegations.keys.insert(id.clone(), public);
            delegations
                .roles
                .iter_mut()
                .find(|role| role.name == "nightly")
                .unwrap()
                .keyids = vec![id];
            let top = tough::editor::signed::SignedRole::new(
                top.signed,
                &tough::schema::KeyHolder::Root(fixture.root.clone()),
                &fixture.signing_keys(&[1]),
                &aws_lc_rs::rand::SystemRandom::new(),
            )
            .await
            .unwrap();
            std::fs::write(&args[1], top.buffer()).unwrap();
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&args[4])
                .unwrap();
            file.write_all(pkcs8.as_ref()).unwrap();
            args[3] = "preview".into();
            let (_, digest) = execute(&args[..4], || support::time().unwrap())
                .await
                .unwrap();
            args[3] = digest.unwrap().to_string().into();
            let (signed, preview) = execute(&args, || support::time().unwrap()).await.unwrap();
            assert!(preview.is_none());
            let response = directory.path().join("signed-response.json");
            std::fs::write(&response, &signed).unwrap();
            drop(file);
            std::fs::remove_file(&args[4]).unwrap();
            let verify = [
                args[0].clone(),
                args[1].clone(),
                args[2].clone(),
                "verify".into(),
                digest.unwrap().to_string().into(),
                response.into(),
            ];
            let (receipt, _) = execute(&verify, || support::time().unwrap()).await.unwrap();
            let receipt: serde_json::Value = serde_json::from_slice(&receipt).unwrap();
            assert_eq!(receipt["proposal_sha256"], digest.unwrap().to_string());
            assert_eq!(
                receipt["signed_sha256"],
                Sha256::digest(&signed).to_string()
            );
            let later = UsableTime(
                support::time()
                    .unwrap()
                    .0
                    .checked_add(jiff::SignedDuration::from_secs(86401))
                    .unwrap(),
            );
            let (retained, _) = execute(&verify, || later).await.unwrap();
            assert_eq!(
                serde_json::from_slice::<serde_json::Value>(&retained).unwrap(),
                receipt
            );
            let signed: tough::schema::Signed<tough::schema::Targets> =
                serde_json::from_slice(&signed).unwrap();
            top.signed()
                .signed
                .delegations
                .as_ref()
                .unwrap()
                .verify_role(&signed, "nightly")
                .unwrap();
        }
    }
}

#[cfg(target_os = "linux")]
#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    linux::run().await
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("feed_sign requires the Linux publisher environment");
    std::process::exit(1);
}
