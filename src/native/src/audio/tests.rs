use super::*;
#[test]
fn render_trace_counts_clipping_after_resampling() {
    let stats = Arc::new(Stats::default());
    stats.ready.store(true, Ordering::Relaxed);
    let trace = Trace::default();
    let (mut sink, source) = RingBuffer::new(4096);
    for _ in 0..4096 {
        sink.push(1.5).unwrap();
    }
    let mut reader = Reader::new(source, 48000, BLOCK, stats, false)
        .unwrap()
        .observed(Some(&trace));
    let last = (0..4 * BLOCK).map(|_| reader.next()).last().unwrap();
    assert_eq!(last, 1.);
    assert!(trace.drain().iter().any(|s| matches!(s.event, Event::Render {monitor: false, clipped, peak} if clipped == BLOCK && peak > 1.)));
}
#[test]
fn monitoring_failure_preserves_main_output() {
    let stats = Arc::new(Stats::default());
    stats.monitoring.store(true, Ordering::Relaxed);
    stream_error(Arc::clone(&stats), true)(cpal::ErrorKind::DeviceNotAvailable.into());
    assert!(!stats.monitoring.load(Ordering::Relaxed));
    assert!(!stats.stop.load(Ordering::Relaxed));
    assert!(stats
        .error
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .contains("主输出继续"));
}
#[test]
fn recoverable_device_notifications_keep_audio_running() {
    let stats = Arc::new(Stats::default());
    let mut callback = stream_error(Arc::clone(&stats), false);
    callback(cpal::ErrorKind::Xrun.into());
    callback(cpal::ErrorKind::DeviceChanged.into());
    callback(cpal::ErrorKind::RealtimeDenied.into());
    assert!(!stats.stop.load(Ordering::Relaxed));
    assert_eq!(stats.device_xruns.load(Ordering::Relaxed), 1);
    callback(cpal::ErrorKind::DeviceNotAvailable.into());
    assert!(stats.stop.load(Ordering::Relaxed));
    assert!(stats.snapshot(false).error.is_some());
}
#[test]
fn starvation_is_one_event_and_reader_recovers_after_refill() {
    let stats = Arc::new(Stats::default());
    stats.ready.store(true, Ordering::Relaxed);
    let (mut producer, consumer) = RingBuffer::new(2048);
    let mut reader = Reader::new(consumer, 48000, 480, Arc::clone(&stats), false).unwrap();
    for _ in 0..100 {
        assert_eq!(reader.next(), 0.0);
    }
    assert_eq!(stats.underruns.load(Ordering::Relaxed), 1);
    for _ in 0..1024 {
        producer.push(0.1).unwrap();
    }
    for _ in 0..512 {
        assert!(reader.next().is_finite());
    }
    assert!(!reader.starving);
    while reader.source.pop().is_ok() {}
    reader.position = BLOCK;
    reader.next();
    assert_eq!(stats.underruns.load(Ordering::Relaxed), 2);
}
#[test]
fn monitoring_can_prime_at_the_queue_capacity_limit() {
    for rate in [8000, 44100, 48000, 96000] {
        let stats = Arc::new(Stats::default());
        stats.ready.store(true, Ordering::Relaxed);
        stats.monitoring.store(true, Ordering::Relaxed);
        let (mut producer, consumer) = RingBuffer::new(4800);
        let mut reader =
            Reader::new(consumer, rate, 4800 - BLOCK, Arc::clone(&stats), true).unwrap();
        for _ in 0..4800 {
            producer.push(0.1).unwrap();
        }
        reader.next();
        assert!(
            !reader.warming,
            "monitor at {rate} Hz must leave warmup when the queue is full"
        );
        let last = (0..BLOCK).map(|_| reader.next()).last().unwrap();
        assert!(
            last > 0.09,
            "primed monitor at {rate} Hz must play queued audio"
        );
        assert_eq!(stats.underruns.load(Ordering::Relaxed), 0);
    }
}
#[test]
fn inaudible_output_tails_become_silence_without_muting_voice() {
    let stats = Arc::new(Stats::default());
    stats.ready.store(true, Ordering::Relaxed);
    let (mut producer, consumer) = RingBuffer::new(4096);
    for _ in 0..1024 {
        producer.push(1e-30).unwrap();
    }
    for _ in 0..2048 {
        producer.push(0.1).unwrap();
    }
    let mut reader = Reader::new(consumer, 48000, 256, Arc::clone(&stats), false).unwrap();
    for _ in 0..512 {
        assert_eq!(reader.next(), 0.0);
    }
    let last = (0..1024).map(|_| reader.next()).last().unwrap();
    assert!((last - 0.1).abs() < 0.001);
    assert_eq!(stats.underruns.load(Ordering::Relaxed), 0);
}
#[test]
#[ignore = "machine-dependent output callback budget; run optimized"]
fn output_reader_quiet_tail_has_realtime_headroom() {
    for value in [0.1, 0.0, 1e-30, 1e-44] {
        let stats = Arc::new(Stats::default());
        stats.ready.store(true, Ordering::Relaxed);
        let (mut producer, consumer) = RingBuffer::new(48000 * 4);
        for _ in 0..48000 * 4 {
            producer.push(value).unwrap();
        }
        let mut reader = Reader::new(consumer, 48000, 256, Arc::clone(&stats), false).unwrap();
        let started = Instant::now();
        for _ in 0..48000 * 3 {
            std::hint::black_box(reader.next());
        }
        let seconds = started.elapsed().as_secs_f64();
        println!("output {value}: three seconds of audio processed in {seconds:.4} s");
        assert_eq!(stats.underruns.load(Ordering::Relaxed), 0);
        assert!(
            seconds < 1.5,
            "output resampling must leave half the real-time budget free"
        );
    }
}

#[test]
fn main_reader_waits_for_ready_before_consuming_input() {
    let stats = Arc::new(Stats::default());
    stats.ready.store(false, Ordering::Relaxed);
    let (mut producer, consumer) = RingBuffer::new(2048);
    let mut reader = Reader::new(consumer, 48000, 0, Arc::clone(&stats), false).unwrap();
    for _ in 0..1024 {
        producer.push(0.1).unwrap();
    }
    let queued = reader.source.slots();

    for _ in 0..BLOCK {
        assert_eq!(reader.next(), 0.0);
    }
    assert_eq!(reader.source.slots(), queued);
    assert_eq!(stats.underruns.load(Ordering::Relaxed), 0);

    stats.ready.store(true, Ordering::Relaxed);
    for _ in 0..BLOCK {
        reader.next();
    }
    assert!(reader.source.slots() < queued);
    assert_eq!(stats.underruns.load(Ordering::Relaxed), 0);
}
