//! Splitting a registered region and moving it to another body. Volume,
//! capacity and damage move together; nothing is created or repaired.

use super::*;

/// The new region a split creates, on the split region's body or another.
#[derive(Debug, Clone, PartialEq)]
pub struct RegionPiece {
    pub id: RegionId,
    pub body: BodyId,
    /// The piece's share of the split region's volume, between 0 and 1
    /// exclusive.
    pub fraction: f32,
    pub section: Section,
    /// Colliders bound to the split region that move to the piece, keeping
    /// their patches. They must already belong to `body`.
    pub colliders: Vec<ColliderId>,
}

impl LoadingState {
    /// A region an edit may split or move: live, registered and in no contact
    /// episode. A contact could otherwise reappear as a new impact on the
    /// changed region.
    fn editable(&self, token: RegionToken) -> Result<Region, LoadingError> {
        let response = self
            .response
            .as_ref()
            .ok_or(LoadingError::ResponseDisabled)?;
        if !self.live(token) {
            return Err(LoadingError::StaleRegion);
        }
        let stored = response
            .materials
            .get(&token)
            .ok_or(LoadingError::InvalidMaterial)?;
        if self.in_contact(token) {
            return Err(LoadingError::RegionInContact);
        }
        stored.region().map_err(|_| LoadingError::InvalidMaterial)
    }
}

impl PhysicsWorld {
    /// Partition a registered region between itself and a new region, both at
    /// its current integrity. The region keeps its token and takes `kept` as
    /// its section; the piece's colliders move to the new token, which is
    /// returned. Refused with nothing changed while a contact episode involves
    /// the region. Like other edits, it ends any compression or gravity run.
    pub fn split_loading_region(
        &mut self,
        token: RegionToken,
        kept: Section,
        piece: &RegionPiece,
    ) -> Result<RegionToken, LoadingError> {
        let known = piece.id.entity == piece.body.entity && self.contains_body(piece.body);
        let parents: Vec<_> = piece
            .colliders
            .iter()
            .map(|c| {
                self.collider_indices
                    .get(c)
                    .map(|i| self.colliders[*i].parent)
            })
            .collect();
        let loading = self.loading.as_mut().ok_or(LoadingError::Disabled)?;
        let whole = loading.editable(token)?;
        if !known {
            return Err(LoadingError::UnknownBody);
        }
        let new = loading.next_token(piece.id, 1)?;
        if kept.validate().is_err() || piece.section.validate().is_err() {
            return Err(LoadingError::InvalidMaterial);
        }
        let (part, rest) = whole
            .split(piece.fraction)
            .map_err(|_| LoadingError::InvalidMaterial)?;
        let mut moving = BTreeSet::new();
        for (collider, parent) in piece.colliders.iter().zip(parents) {
            let bound = loading
                .bindings
                .get(collider)
                .is_some_and(|s| s.region == token);
            if !(bound && parent == Some(piece.body) && moving.insert(*collider)) {
                return Err(LoadingError::InvalidBinding);
            }
        }
        loading.admit(new, piece.body);
        let response = loading.response.as_mut().unwrap();
        response
            .materials
            .insert(token, StoredRegion::new(&rest, kept));
        response
            .materials
            .insert(new, StoredRegion::new(&part, piece.section));
        for collider in moving {
            loading.bindings.get_mut(&collider).unwrap().region = new;
        }
        loading.invalidate_arrival_proofs();
        Ok(new)
    }

    /// Move a registered region's whole balance and section to a new region
    /// on `body`, for ownership changes and rebuilds: create the replacement
    /// body, transfer, bind its colliders, then remove the old body. The old
    /// token goes stale and its colliders are unbound. Refused with nothing
    /// changed while a contact episode involves the region.
    pub fn transfer_loading_region(
        &mut self,
        token: RegionToken,
        to: RegionId,
        body: BodyId,
    ) -> Result<RegionToken, LoadingError> {
        let known = to.entity == body.entity && self.contains_body(body);
        let loading = self.loading.as_mut().ok_or(LoadingError::Disabled)?;
        loading.editable(token)?;
        if !known {
            return Err(LoadingError::UnknownBody);
        }
        let new = loading.next_token(to, 0)?;
        loading.regions.remove(&token.id);
        loading.admit(new, body);
        loading.bindings.retain(|_, s| s.region != token);
        let response = loading.response.as_mut().unwrap();
        let stored = response.materials.remove(&token).unwrap();
        response.materials.insert(new, stored);
        loading.invalidate_arrival_proofs();
        Ok(new)
    }
}

#[cfg(test)]
mod tests;
