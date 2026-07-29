//! PoC 7.5: Telling decoy source nodes what traffic they must send
//! Node A sends encrypted instructions to Node B (a controllable node).
//! Node B opens them, then generates garbage packets and transmits them to Node C (RDV destination).
//! To C these look like ordinary traffic it cannot open, further obfuscating real activity.
//! A dials B directly over TCP, and B dials C directly over TCP (no router layer involved).

mod envelope;
mod node;

use std::net::TcpListener;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use x_wing::{Kem, XWingKem};

use envelope::DecoySourceInstruction;
use node::{
    Destination, NoisePacketSeen, run_decoy_source, run_noise_destination, send_instructions,
};

/// Stores test report data
struct DecoySourceDemo {
    control_tag: String,
    noise_sent: usize,
    noise_received: usize,
    noise_opened: usize,
    elapsed: Duration,
}

fn main() -> Result<()> {
    let decoy_demo = run_decoy_source_demo()?;
    report_decoy_source(&decoy_demo)?;
    Ok(())
}

/// Shared setup: generate keys, bind listeners, spawn worker threads, send
/// instructions, collect reports, join workers, and return a [`DecoySourceDemo`].
/// Used by both [`run_decoy_source_demo`] and the integration test.
fn setup_and_run_decoy_source(_packet_size: u32) -> Result<DecoySourceDemo> {
    let (b_secret, b_public) = XWingKem::generate_keypair();
    let (c_secret, _c_public) = XWingKem::generate_keypair();

    let c_listener = TcpListener::bind(("127.0.0.1", 0)).context("bind noise dest C")?;
    let c_port = c_listener.local_addr()?.port();

    let b_listener = TcpListener::bind(("127.0.0.1", 0)).context("bind decoy source B")?;
    let b_port = b_listener.local_addr()?.port();

    let c_dest = Destination { secret: c_secret };
    let (c_report_tx, c_report_rx) = mpsc::channel::<NoisePacketSeen>();

    let c_worker =
        thread::spawn(move || run_noise_destination(c_listener, c_dest, 10, c_report_tx));

    let (b_done_tx, b_done_rx) = mpsc::channel::<()>();
    let (b_report_tx, b_report_rx) = mpsc::channel::<NoisePacketSeen>();
    let b_worker =
        thread::spawn(move || run_decoy_source(b_listener, b_secret, b_done_tx, b_report_tx));

    let instruction = DecoySourceInstruction {
        destination_addr: c_port,
        packet_count: 10,
        packet_size: 200,
        bitrate_bps: 100_000,
    };
    let control_tag = send_instructions(&b_public, &instruction, b_port)?;

    let start = Instant::now();
    let _ = b_done_rx.recv_timeout(Duration::from_secs(10));
    let elapsed = start.elapsed();

    // Consume B's self-reports
    let mut noise_sent = 0;
    println!("--- decoy source (B) self-reports ---");
    while let Ok(report) = b_report_rx.try_recv() {
        noise_sent += 1;
        println!("sent  seq={}  tag={}", report.seq, report.wire_tag);
    }
    if noise_sent == 0 {
        println!("  (no reports received before channel closed)");
    }

    let mut noise_received = 0;
    let mut noise_opened = 0;
    println!("--- rdv destination (C) reports ---");
    for _ in 0..instruction.packet_count {
        if let Ok(report) = c_report_rx.recv_timeout(Duration::from_secs(5)) {
            noise_received += 1;
            println!(
                "received  seq={}  tag={}  opened={}",
                report.seq, report.wire_tag, report.opened
            );
            if report.opened {
                noise_opened += 1;
            }
        }
    }

    let _ = c_worker.join();
    let _ = b_worker.join();

    Ok(DecoySourceDemo {
        control_tag,
        noise_sent,
        noise_received,
        noise_opened,
        elapsed,
    })
}

/// Spins Node C (RDV destination), Node B (decoy source), and Node A (controller).
/// Node A sends sealed instructions to Node B; Node B generates cover traffic to Node C.
fn run_decoy_source_demo() -> Result<DecoySourceDemo> {
    setup_and_run_decoy_source(200)
}

/// Prints test report
fn report_decoy_source(demo: &DecoySourceDemo) -> Result<()> {
    println!("decoy source control tag: {}", demo.control_tag);
    println!("noise packets sent by B: {}", demo.noise_sent);
    println!("noise packets received by C: {}", demo.noise_received);
    println!("noise packets opened by C: {}", demo.noise_opened);
    println!("elapsed: {:?}", demo.elapsed);

    if demo.noise_sent != 10 {
        bail!("expected 10 noise packets sent, got {}", demo.noise_sent);
    }
    if demo.noise_received != 10 {
        bail!(
            "expected 10 noise packets received, got {}",
            demo.noise_received
        );
    }
    if demo.noise_opened != 0 {
        bail!("expected 0 opened noise packets, got {}", demo.noise_opened);
    } else {
        println!("\nEverything is OK: Decoy source sent cover traffic.");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::envelope::PACKET_SIZE;

    #[test]
    fn demonstrates_decoy_source_sends_exact_count_and_rate() {
        let demo = setup_and_run_decoy_source(300).unwrap();

        assert_eq!(
            demo.noise_received, 10,
            "expected 10 noise packets, got {}",
            demo.noise_received
        );
        assert_eq!(
            demo.noise_opened, 0,
            "expected 0 opened noise packets, got {}",
            demo.noise_opened
        );

        let expected_ns = 10u64 * PACKET_SIZE as u64 * 8 * 1_000_000_000 / 100_000;
        let expected = Duration::from_nanos(expected_ns);
        let lower = expected.as_secs_f64() * 0.5;
        let upper = expected.as_secs_f64() * 2.0;
        let actual = demo.elapsed.as_secs_f64();
        assert!(
            actual >= lower && actual <= upper,
            "elapsed {actual:.3}s outside [{lower:.3}s, {upper:.3}s] (expected {expected:.3}s)",
            expected = expected.as_secs_f64()
        );
    }
}
