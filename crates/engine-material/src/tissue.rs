//! Mechanical integrity for lightweight plant tissue, kept apart from its
//! biological health. Each tissue keeps one [`Region`] sized by its role's
//! material and its current size. Loads charge it through the same laws as any
//! other region, and the charge that exhausts it reports the tissue once at the
//! host's boundary. Physiology never reads or restores integrity, and nothing
//! here reads or changes health, so repairing or restoring integrity cannot
//! revive dead tissue. The host decides what a failure means. See
//! `docs/design/material-tissue.md`.

use crate::{
    ContactGeometry, Impact, Material, MaterialError, Outcome, Region, SustainedLoad, impact,
    sustained,
};
use engine_core::plants::{Plants, Role, TissueId};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// One role's tissue material and the represented volume each unit of size
/// holds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TissueMaterial {
    pub material: Material,
    pub volume_per_size: f32,
}

/// Tissue materials in the plant `Config` tables' role order: root, runner,
/// shoot, moss. Saves carry the caller-assigned `identity`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TissueTable {
    pub identity: u64,
    pub roles: [TissueMaterial; 4],
}

impl TissueTable {
    /// Every size from 1 to `max_size` must make a valid region.
    fn validate(&self, max_size: u16) -> Result<(), MaterialError> {
        for role in self.roles {
            if !(role.volume_per_size.is_finite() && role.volume_per_size > 0.0) {
                return Err(MaterialError::InvalidRegion);
            }
            Region::new(role.material, role.volume_per_size)?;
            Region::new(role.material, role.volume_per_size * f32::from(max_size))?;
        }
        Ok(())
    }

    fn region(&self, role: Role, size: u16) -> Result<Region, MaterialError> {
        let entry = self.roles[role.index()];
        Region::new(entry.material, entry.volume_per_size * f32::from(size))
    }
}

/// Why a call was refused. A refused call changes nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TissueError {
    /// The table, a load or an amount of work is invalid.
    Material(MaterialError),
    /// No record for this tissue: it does not exist or has been removed.
    Unknown(TissueId),
    /// The tissue grew after its integrity failed. A host removes a failed
    /// tissue, or repairs it explicitly, before admitting its growth.
    FailedGrowth(TissueId),
    /// The plant world, table or save do not belong together.
    Mismatch,
}

impl From<MaterialError> for TissueError {
    fn from(error: MaterialError) -> Self {
        Self::Material(error)
    }
}

impl std::fmt::Display for TissueError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Material(error) => error.fmt(f),
            Self::Unknown(id) => write!(f, "tissue {id} has no integrity record"),
            Self::FailedGrowth(id) => write!(f, "tissue {id} grew after its integrity failed"),
            Self::Mismatch => f.write_str("integrity does not match this plant world or table"),
        }
    }
}

impl std::error::Error for TissueError {}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Record {
    role: Role,
    size: u16,
    region: Region,
}

impl Record {
    fn resized(self, table: &TissueTable, id: TissueId, size: u16) -> Result<Self, TissueError> {
        if size == self.size {
            return Ok(self);
        }
        let full = table.region(self.role, size)?;
        let capacity = f64::from(full.capacity());
        let before = self.region.remaining_precise();
        let remaining = if size > self.size {
            if self.region.failed() {
                return Err(TissueError::FailedGrowth(id));
            }
            // New material arrives intact; the damage already done stays.
            (before + (capacity - f64::from(self.region.capacity()))).min(capacity)
        } else {
            // The shed material takes its share of the damage, as a split does.
            (capacity * before / f64::from(self.region.capacity())).min(before)
        };
        Ok(Self {
            size,
            region: Region::with_remaining(full.material(), full.volume(), remaining)?,
            ..self
        })
    }
}

/// Charge `work`, and note the failure if this charge exhausted the region.
fn absorb(
    failures: &mut BTreeSet<TissueId>,
    id: TissueId,
    region: &mut Region,
    work: f32,
) -> Result<f32, MaterialError> {
    let intact = !region.failed();
    let unspent = region.absorb(work)?;
    if intact && region.failed() {
        failures.insert(id);
    }
    Ok(unspent)
}

/// One integrity record for every tissue of one plant world, live or dead.
#[derive(Debug, Clone, PartialEq)]
pub struct TissueIntegrity {
    table: TissueTable,
    records: BTreeMap<TissueId, Record>,
    failures: BTreeSet<TissueId>,
}

impl TissueIntegrity {
    /// Records at full capacity for every tissue `plants` has.
    pub fn new(table: TissueTable, plants: &Plants) -> Result<Self, TissueError> {
        table.validate(plants.observe().config.max_size)?;
        let mut integrity = Self {
            table,
            records: BTreeMap::new(),
            failures: BTreeSet::new(),
        };
        integrity.sync(plants)?;
        Ok(integrity)
    }

    pub fn table(&self) -> TissueTable {
        self.table
    }

    /// A tissue's region: its integrity, normalized health and remaining
    /// capacity.
    pub fn region(&self, id: TissueId) -> Option<&Region> {
        self.records.get(&id).map(|record| &record.region)
    }

    /// Follow the plant world after any call that changes it, including each
    /// tick. New tissue starts intact. Growth adds intact material and keeps
    /// the damage already done; shrinking keeps the integrity; removal drops
    /// the record with any failure not yet taken.
    pub fn sync(&mut self, plants: &Plants) -> Result<(), TissueError> {
        let mut records = BTreeMap::new();
        for tissue in plants.observe().tissues {
            let record = match self.records.get(&tissue.id) {
                None => Record {
                    role: tissue.role,
                    size: tissue.size,
                    region: self.table.region(tissue.role, tissue.size)?,
                },
                Some(record) if record.role != tissue.role => return Err(TissueError::Mismatch),
                Some(record) => record.resized(&self.table, tissue.id, tissue.size)?,
            };
            records.insert(tissue.id, record);
        }
        self.failures.retain(|id| records.contains_key(id));
        self.records = records;
        Ok(())
    }

    /// Evaluate `striker` hitting the tissue, and charge both. The striker is
    /// side `a` of `contact` and of the outcome, and the tissue is side `b`.
    pub fn strike(
        &mut self,
        id: TissueId,
        striker: &mut Region,
        contact: ContactGeometry,
        load: Impact,
    ) -> Result<Outcome, TissueError> {
        let record = self.records.get_mut(&id).ok_or(TissueError::Unknown(id))?;
        let outcome = impact(striker, &record.region, contact, load)?;
        striker.absorb(outcome.consumed[0])?;
        absorb(
            &mut self.failures,
            id,
            &mut record.region,
            outcome.consumed[1],
        )?;
        Ok(outcome)
    }

    /// Evaluate a sustained load resting on the tissue, and charge it.
    pub fn press(&mut self, id: TissueId, load: SustainedLoad) -> Result<Outcome, TissueError> {
        let record = self.records.get_mut(&id).ok_or(TissueError::Unknown(id))?;
        let outcome = sustained(&record.region, load)?;
        absorb(
            &mut self.failures,
            id,
            &mut record.region,
            outcome.consumed[0],
        )?;
        Ok(outcome)
    }

    /// Charge work another response path allocated to the tissue. Returns
    /// the work it could not absorb.
    pub fn charge(&mut self, id: TissueId, work: f32) -> Result<f32, TissueError> {
        let record = self.records.get_mut(&id).ok_or(TissueError::Unknown(id))?;
        Ok(absorb(&mut self.failures, id, &mut record.region, work)?)
    }

    /// Explicit, host-authorized repair of a fraction of the tissue's full
    /// capacity. It changes integrity only, so a dead tissue stays dead. A
    /// failure not yet taken is withdrawn once capacity returns.
    pub fn repair(&mut self, id: TissueId, fraction: f32) -> Result<(), TissueError> {
        let record = self.records.get_mut(&id).ok_or(TissueError::Unknown(id))?;
        record.region.repair(fraction)?;
        if !record.region.failed() {
            self.failures.remove(&id);
        }
        Ok(())
    }

    /// Tissues whose integrity failed since the last call, in identity order.
    /// Each failure is reported once, however many loads reached it.
    pub fn take_failures(&mut self) -> Vec<TissueId> {
        std::mem::take(&mut self.failures).into_iter().collect()
    }

    pub fn save(&self) -> TissueSave {
        TissueSave {
            table: self.table.identity,
            records: self
                .records
                .iter()
                .map(|(&id, record)| TissueRecord {
                    id,
                    size: record.size,
                    remaining: record.region.remaining_precise(),
                })
                .collect(),
            failures: self.failures.iter().copied().collect(),
        }
    }

    /// Restore a save alongside the plant world saved with it. Every tissue
    /// needs its record at its current size, and each failure not yet taken
    /// must name a failed record.
    pub fn restore(
        table: TissueTable,
        plants: &Plants,
        save: &TissueSave,
    ) -> Result<Self, TissueError> {
        let observation = plants.observe();
        table.validate(observation.config.max_size)?;
        if save.table != table.identity || save.records.len() != observation.tissues.len() {
            return Err(TissueError::Mismatch);
        }
        let mut records = BTreeMap::new();
        for (saved, tissue) in save.records.iter().zip(&observation.tissues) {
            if saved.id != tissue.id || saved.size != tissue.size {
                return Err(TissueError::Mismatch);
            }
            let full = table.region(tissue.role, tissue.size)?;
            let region = Region::with_remaining(full.material(), full.volume(), saved.remaining)
                .map_err(|_| TissueError::Mismatch)?;
            records.insert(
                tissue.id,
                Record {
                    role: tissue.role,
                    size: tissue.size,
                    region,
                },
            );
        }
        let ordered = save.failures.windows(2).all(|pair| pair[0] < pair[1]);
        let failed = save
            .failures
            .iter()
            .all(|id| records.get(id).is_some_and(|record| record.region.failed()));
        if !(ordered && failed) {
            return Err(TissueError::Mismatch);
        }
        Ok(Self {
            table,
            records,
            failures: save.failures.iter().copied().collect(),
        })
    }
}

/// One tissue's saved integrity: its size and precise remaining capacity.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TissueRecord {
    pub id: TissueId,
    pub size: u16,
    pub remaining: f64,
}

/// Saved integrity for one plant world, in identity order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TissueSave {
    /// Identity of the table the records were made with.
    pub table: u64,
    pub records: Vec<TissueRecord>,
    /// Failures not yet taken.
    pub failures: Vec<TissueId>,
}
