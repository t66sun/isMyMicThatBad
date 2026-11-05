pub(super) fn start(
    main: impl FnOnce() -> Result<(), String>,
    monitor: impl FnOnce() -> Result<(), String>,
    capture: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    // Renderers receive their initial WASAPI fill while Reader still returns silence.
    main()?;
    monitor()?;
    capture()
}

#[cfg(test)]
mod tests {
    use super::super::{Reader, Stats, BLOCK};
    use rtrb::RingBuffer;
    use std::{
        cell::RefCell,
        sync::{atomic::Ordering, Arc},
    };

    #[test]
    fn monitor_startup_burst_cannot_spend_the_live_queue_reserve() {
        let stats = Arc::new(Stats::default());
        stats.monitoring.store(true, Ordering::Relaxed);
        let (producer, consumer) = RingBuffer::new(4864);
        let producer = RefCell::new(producer);
        let reader = RefCell::new(Reader::new(consumer, 48000, 1696, stats.clone(), true).unwrap());
        // Replay the producer/monitor ordering captured on TANCHJIM + VB-CABLE.
        // The first callback converts five blocks at once. Starting that callback
        // before capture fills the queue must leave its live reserve untouched.
        super::start(
            || Ok(()),
            || {
                for _ in 0..5 * BLOCK {
                    reader.borrow_mut().next();
                }
                Ok(())
            },
            || {
                for _ in 0..10 * BLOCK {
                    producer.borrow_mut().push(0.25).unwrap();
                }
                stats.ready.store(true, Ordering::Relaxed);
                Ok(())
            },
        )
        .unwrap();
        let pushes: &[usize] = &[
            1, 0, 0, 0, 2, 0, 2, 2, 0, 2, 0, 2, 0, 1, 0, 2, 0, 2, 0, 2, 0, 2, 2, 0, 2, 0, 2, 0, 1,
            0, 2, 0, 2, 0, 2, 0, 2, 2, 0, 2, 0, 2, 0, 1, 0, 2, 0, 2, 0, 2, 0, 2, 0, 0, 4, 0, 2, 0,
            1, 0, 2, 0, 2, 0, 2, 0, 2, 2, 0, 2, 0, 2, 0, 1, 0, 2, 0, 2, 0, 2, 0, 2, 2, 0, 2, 0, 2,
            0, 1, 0, 2, 0, 2, 0, 2, 0, 2, 2, 0, 2, 0, 2, 0, 1, 0, 2, 0, 2, 0, 2, 0, 2, 2, 0, 2, 0,
            2, 0, 1, 0, 2, 0, 2, 0, 2, 0, 2, 2, 0, 2, 0, 2, 0, 1, 0, 2, 0, 2, 0, 2, 0, 2, 2, 0, 2,
            0, 2, 0, 1, 0, 2, 0, 2, 0, 2, 0, 2, 2, 0, 2, 0, 2, 0, 1, 0, 2, 0, 2, 0, 2, 0, 2, 2, 0,
            2, 0, 2, 0, 1, 0, 2, 0, 2, 0, 2, 0, 2, 2, 0, 2, 0, 2, 0, 1, 0, 2, 0, 2, 0, 2, 0, 0, 4,
            0, 2, 0, 2, 0, 1, 0, 2, 0, 2, 0, 2, 0, 2, 2, 0, 2, 0, 2, 0, 1, 0, 2, 0, 2, 0, 2, 0, 2,
            2, 0, 2, 0, 2, 0, 1, 0, 2, 0, 2, 0, 2, 0, 2, 2, 0, 2, 0, 2, 0, 1, 0, 2, 0, 2, 0, 0, 0,
        ];
        let mut last = 0.;
        for &blocks in pushes {
            for _ in 0..blocks * BLOCK {
                producer.borrow_mut().push(0.25).unwrap();
            }
            for _ in 0..BLOCK {
                last = reader.borrow_mut().next();
            }
        }
        assert_eq!(stats.underruns.load(Ordering::Relaxed), 0);
        assert!((last - 0.25).abs() < 0.001);
    }
}
