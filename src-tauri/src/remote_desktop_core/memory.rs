use std::ops::{Deref, DerefMut};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

pub(crate) const CLIENT_MEMORY_LIMIT: usize = 768 * 1024 * 1024;
// Bounded input/events, dirty metadata, and codec state are accounted separately
// from large pixel/wire/Surface buffers. This is not a process RSS limit.
const SMALL_STATE_ALLOWANCE: usize = 64 * 1024 * 1024;

struct MemoryState {
    limit: usize,
    used: AtomicUsize,
    #[cfg(test)]
    peak_used: AtomicUsize,
    #[cfg(test)]
    buffer_allocations: AtomicUsize,
}

/// One shared admission budget per Client, including buffers retained by other
/// threads. Reservations travel with ownership and do not change during swaps.
#[derive(Clone)]
pub(crate) struct ClientMemoryBudget(Arc<MemoryState>);

impl Default for ClientMemoryBudget {
    fn default() -> Self {
        Self::new(CLIENT_MEMORY_LIMIT, SMALL_STATE_ALLOWANCE)
    }
}

impl ClientMemoryBudget {
    pub(crate) fn new(limit: usize, allowance: usize) -> Self {
        assert!(allowance <= limit);
        Self(Arc::new(MemoryState {
            limit,
            used: AtomicUsize::new(allowance),
            #[cfg(test)]
            peak_used: AtomicUsize::new(allowance),
            #[cfg(test)]
            buffer_allocations: AtomicUsize::new(0),
        }))
    }

    pub(crate) fn reserve(&self, bytes: usize) -> Result<MemoryReservation, String> {
        if bytes > self.0.limit.saturating_sub(self.in_use()) {
            return Err("rfb_resource_limit:session_memory".into());
        }
        let _previous_used = self
            .0
            .used
            .try_update(Ordering::AcqRel, Ordering::Acquire, |used| {
                used.checked_add(bytes)
                    .filter(|total| *total <= self.0.limit)
            })
            .map_err(|_| "rfb_resource_limit:session_memory")?;
        #[cfg(test)]
        self.0
            .peak_used
            .fetch_max(_previous_used + bytes, Ordering::Relaxed);
        Ok(MemoryReservation {
            budget: self.clone(),
            bytes,
        })
    }

    pub(super) fn same_owner(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
    pub(crate) fn in_use(&self) -> usize {
        self.0.used.load(Ordering::Acquire)
    }

    /// Admission high-water includes the fixed allowance and conservative scratch charges.
    #[cfg(test)]
    pub(super) fn peak_in_use(&self) -> usize {
        self.0.peak_used.load(Ordering::Relaxed)
    }

    /// Counts successful non-empty BudgetedBytes allocations, excluding ordinary Vec scratch.
    #[cfg(test)]
    pub(super) fn budgeted_buffer_allocations(&self) -> usize {
        self.0.buffer_allocations.load(Ordering::Relaxed)
    }
}

pub(crate) struct MemoryReservation {
    budget: ClientMemoryBudget,
    bytes: usize,
}

impl MemoryReservation {
    pub(super) fn resize(&mut self, bytes: usize) -> Result<(), String> {
        if bytes > self.bytes {
            let mut extra = self.budget.reserve(bytes - self.bytes)?;
            extra.bytes = 0;
        } else {
            self.budget
                .0
                .used
                .fetch_sub(self.bytes - bytes, Ordering::AcqRel);
        }
        self.bytes = bytes;
        Ok(())
    }
}

impl Drop for MemoryReservation {
    fn drop(&mut self) {
        self.budget.0.used.fetch_sub(self.bytes, Ordering::AcqRel);
    }
}

/// A byte allocation and its admission charge form one value. Growth allocates
/// the replacement before freeing the original, so both capacities are charged
/// during copying; failure leaves the old bytes and charge intact.
pub(super) struct BudgetedBytes {
    bytes: Vec<u8>,
    reservation: MemoryReservation,
}

impl BudgetedBytes {
    pub(super) fn empty(budget: &ClientMemoryBudget) -> Self {
        Self {
            bytes: Vec::new(),
            reservation: MemoryReservation {
                budget: budget.clone(),
                bytes: 0,
            },
        }
    }

    pub(super) fn with_capacity(
        budget: &ClientMemoryBudget,
        capacity: usize,
    ) -> Result<Self, String> {
        let mut reservation = budget.reserve(capacity)?;
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(capacity)
            .map_err(|_| "rfb_resource_limit:allocation")?;
        reservation.resize(bytes.capacity())?;
        #[cfg(test)]
        if bytes.capacity() != 0 {
            budget.0.buffer_allocations.fetch_add(1, Ordering::Relaxed);
        }
        Ok(Self { bytes, reservation })
    }

    pub(super) fn zeroed(budget: &ClientMemoryBudget, length: usize) -> Result<Self, String> {
        let mut buffer = Self::with_capacity(budget, length)?;
        buffer.bytes.resize(length, 0);
        Ok(buffer)
    }

    pub(super) fn budget(&self) -> ClientMemoryBudget {
        self.reservation.budget.clone()
    }
    pub(super) fn capacity(&self) -> usize {
        self.bytes.capacity()
    }
    pub(super) fn as_slice(&self) -> &[u8] {
        &self.bytes
    }
    pub(super) fn clear(&mut self) {
        self.bytes.clear();
    }

    pub(super) fn resize_zeroed(&mut self, length: usize) -> Result<(), String> {
        self.ensure_capacity(length, false)?;
        self.bytes.resize(length, 0);
        Ok(())
    }

    pub(super) fn extend_from_slice(&mut self, bytes: &[u8]) -> Result<(), String> {
        let required = self
            .bytes
            .len()
            .checked_add(bytes.len())
            .ok_or("rfb_resource_limit:session_memory")?;
        self.ensure_capacity(required, true)?;
        self.bytes.extend_from_slice(bytes);
        Ok(())
    }

    fn ensure_capacity(&mut self, required: usize, grow: bool) -> Result<(), String> {
        if required <= self.capacity() {
            return Ok(());
        }
        let target = if grow {
            required.max(self.capacity().saturating_mul(2)).max(4096)
        } else {
            required
        };
        let budget = self.budget();
        let mut next = Self::with_capacity(&budget, target).or_else(|error| {
            if target > required {
                Self::with_capacity(&budget, required)
            } else {
                Err(error)
            }
        })?;
        next.bytes.extend_from_slice(&self.bytes);
        *self = next;
        Ok(())
    }

    pub(super) fn split_off(&mut self, offset: usize) -> Result<Self, String> {
        let mut trailing = Self::with_capacity(&self.budget(), self.bytes.len() - offset)?;
        trailing.bytes.extend_from_slice(&self.bytes[offset..]);
        self.bytes.truncate(offset);
        Ok(trailing)
    }

    pub(super) fn into_parts(self) -> (Vec<u8>, MemoryReservation) {
        (self.bytes, self.reservation)
    }
}

impl Deref for BudgetedBytes {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        &self.bytes
    }
}
impl DerefMut for BudgetedBytes {
    fn deref_mut(&mut self) -> &mut [u8] {
        &mut self.bytes
    }
}

#[cfg(test)]
#[path = "../../tests/support/client_memory_reservation_tests.rs"]
mod tests;
