//! One ordered application-lifetime queue: restart cannot race an older save.
use scenario_material_lab::{MAX_SAVE_BYTES, MaterialLabState};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{OnceLock, mpsc},
};
pub(super) enum Operation {
    Save(Vec<u8>),
    Load,
}
pub(super) enum Outcome {
    Saved,
    Loaded(Box<MaterialLabState>),
}
type Receipt = mpsc::Receiver<Result<Outcome, String>>;
struct Job {
    path: PathBuf,
    operation: Operation,
    reply: mpsc::Sender<Result<Outcome, String>>,
}
static WORKER: OnceLock<Result<mpsc::Sender<Job>, String>> = OnceLock::new();
pub(super) fn request(path: PathBuf, operation: Operation) -> Result<Receipt, String> {
    let worker = WORKER
        .get_or_init(|| {
            let (tx, rx) = mpsc::channel::<Job>();
            std::thread::Builder::new()
                .name("material-lab-storage".into())
                .spawn(move || {
                    for job in rx {
                        let result = perform(&job.path, job.operation);
                        let _ = job.reply.send(result);
                    }
                })
                .map_err(|e| e.to_string())?;
            Ok(tx)
        })
        .as_ref()
        .map_err(Clone::clone)?;
    let (reply, receipt) = mpsc::channel();
    worker
        .send(Job {
            path,
            operation,
            reply,
        })
        .map_err(|e| e.to_string())?;
    Ok(receipt)
}
fn perform(path: &Path, operation: Operation) -> Result<Outcome, String> {
    match operation {
        Operation::Save(bytes) => {
            save(path, &bytes).map_err(|e| format!("Save failed: {e}"))?;
            Ok(Outcome::Saved)
        }
        Operation::Load => {
            let file = fs::File::open(path).map_err(|e| format!("Load failed: {e}"))?;
            let mut bytes = Vec::new();
            file.take(MAX_SAVE_BYTES + 1)
                .read_to_end(&mut bytes)
                .map_err(|e| e.to_string())?;
            MaterialLabState::from_snapshot_bytes(&bytes)
                .map(|state| Outcome::Loaded(Box::new(state)))
                .map_err(|e| format!("Load failed: {e}"))
        }
    }
}
fn save(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| std::io::Error::other("Save directory missing"))?;
    fs::create_dir_all(parent)?;
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    temp.write_all(bytes)?;
    temp.as_file_mut().sync_all()?;
    temp.persist(path).map_err(|e| e.error)?;
    #[cfg(unix)]
    fs::File::open(parent)?.sync_all()?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    use scenario_material_lab::Case;
    #[test]
    fn saved_slot_survives_new_owner_and_rejects_corruption() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("material-lab.bin");
        let mut state = MaterialLabState::new(Case::PlasticReturn);
        for _ in 0..200 {
            state.advance();
        }
        let bytes = state.snapshot_bytes().unwrap();
        assert!(matches!(
            request(path.clone(), Operation::Save(bytes.clone()))
                .unwrap()
                .recv()
                .unwrap()
                .unwrap(),
            Outcome::Saved
        ));
        let Outcome::Loaded(restored) = request(path.clone(), Operation::Load)
            .unwrap()
            .recv()
            .unwrap()
            .unwrap()
        else {
            panic!()
        };
        assert_eq!(restored.snapshot_bytes().unwrap(), bytes);
        fs::write(&path, b"corrupt").unwrap();
        assert!(
            request(path.clone(), Operation::Load)
                .unwrap()
                .recv()
                .unwrap()
                .is_err()
        );
        assert_eq!(fs::read(path).unwrap(), b"corrupt");
    }
    #[test]
    fn failed_save_preserves_the_previous_slot() {
        let dir = tempfile::tempdir().unwrap();
        let parent = dir.path().join("not-a-directory");
        fs::write(&parent, b"keep").unwrap();
        assert!(perform(&parent.join("material-lab.bin"), Operation::Save(vec![1])).is_err());
        assert_eq!(fs::read(parent).unwrap(), b"keep");
    }
}
