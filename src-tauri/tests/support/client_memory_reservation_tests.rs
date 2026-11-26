use super::*;

#[test]
fn observations_track_admission_peak_and_successful_buffer_allocations() {
    let budget = ClientMemoryBudget::new(256, 16);
    assert_eq!(budget.peak_in_use(), 16);
    let reservation = budget.reserve(24).unwrap();
    assert_eq!(budget.peak_in_use(), 40);
    assert_eq!(budget.budgeted_buffer_allocations(), 0);
    drop(reservation);
    assert_eq!(budget.in_use(), 16);

    drop(BudgetedBytes::with_capacity(&budget, 0).unwrap());
    assert_eq!(budget.budgeted_buffer_allocations(), 0);
    let mut buffer = BudgetedBytes::zeroed(&budget, 32).unwrap();
    let old_capacity = buffer.capacity();
    assert_eq!(budget.budgeted_buffer_allocations(), 1);
    buffer.extend_from_slice(&[9]).unwrap();
    let new_capacity = buffer.capacity();
    // Growth admits the replacement while the old allocation is still alive.
    assert_eq!(budget.peak_in_use(), 16 + old_capacity + new_capacity);
    assert_eq!(budget.budgeted_buffer_allocations(), 2);
    assert_eq!(budget.in_use(), 16 + new_capacity);
    assert_eq!(&buffer.as_slice()[..32], &[0; 32]);
    assert_eq!(buffer.as_slice()[32], 9);

    let peak = budget.peak_in_use();
    assert_eq!(
        buffer.extend_from_slice(&[0; 256]).unwrap_err(),
        "rfb_resource_limit:session_memory"
    );
    assert_eq!(budget.peak_in_use(), peak);
    assert_eq!(budget.budgeted_buffer_allocations(), 2);
    assert_eq!(buffer.as_slice().len(), 33);
    drop(buffer);
    assert_eq!(budget.in_use(), 16);
    assert_eq!(budget.peak_in_use(), peak);
}

#[test]
fn growing_a_live_buffer_requires_room_for_old_and_new_allocations() {
    let budget = ClientMemoryBudget::new(64, 0);
    let mut buffer = BudgetedBytes::zeroed(&budget, 32).unwrap();
    assert_eq!(budget.in_use(), 32);
    assert_eq!(
        buffer.extend_from_slice(&[1]).unwrap_err(),
        "rfb_resource_limit:session_memory"
    );
    assert_eq!(buffer.as_slice(), [0; 32]);
    assert_eq!(budget.in_use(), 32);
    buffer.clear();
    assert_eq!(budget.in_use(), 32, "empty lengths still retain capacity");
    drop(buffer);
    assert_eq!(budget.in_use(), 0);
}

#[test]
fn failed_concurrent_admissions_do_not_leak_or_overbook_the_budget() {
    let budget = ClientMemoryBudget::new(128, 16);
    let barrier = Arc::new(std::sync::Barrier::new(16));
    let workers = (0..16)
        .map(|_| {
            let budget = budget.clone();
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                budget.reserve(16)
            })
        })
        .collect::<Vec<_>>();
    let mut leases = Vec::new();
    for worker in workers {
        if let Ok(lease) = worker.join().unwrap() {
            leases.push(lease);
        }
    }
    assert_eq!(leases.len(), 7);
    assert_eq!(budget.in_use(), 128);
    drop(leases);
    assert_eq!(budget.in_use(), 16);
}

#[test]
fn failed_reservation_growth_preserves_the_original_charge() {
    let budget = ClientMemoryBudget::new(128, 0);
    let mut first = budget.reserve(80).unwrap();
    let second = budget.reserve(48).unwrap();
    assert!(first.resize(81).is_err());
    assert_eq!(budget.in_use(), 128);
    drop(second);
    first.resize(96).unwrap();
    assert_eq!(budget.in_use(), 96);
    first.resize(32).unwrap();
    assert_eq!(budget.in_use(), 32);
    drop(first);
    assert_eq!(budget.in_use(), 0);
}
