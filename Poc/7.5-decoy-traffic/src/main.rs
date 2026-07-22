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
    Destination, NoisePacketSeen, run_decoy_source, run_noise_destination, send_instructions};

/// Stores test report data
struct DecoySourceDemo {
    control_tag: String,
    noise_received: usize,
    noise_opened: usize,
    elapsed: Duration,
}

fn main() -> Result<()> {
    let decoy_demo = run_decoy_source_demo()?;
    report_decoy_source(&decoy_demo)?;
    Ok(())
}

/// Spins Node C (RDV destination), Node B (decoy source), and Node A (controller).
/// Node A sends sealed instructions to Node B; Node B generates cover traffic to Node C.
fn run_decoy_source_demo() -> Result<DecoySourceDemo> {
    let (b_secret, b_public) = XWingKem::generate_keypair();
    let (c_secret, _c_public) = XWingKem::generate_keypair();

    let c_listener = TcpListener::bind(("127.0.0.1", 0)).context("bind noise dest C")?;
    let c_port = c_listener.local_addr()?.port();

    let b_listener = TcpListener::bind(("127.0.0.1", 0)).context("bind decoy source B")?;
    let b_port = b_listener.local_addr()?.port();

    let c_dest = Destination {
        secret: c_secret,
    };
    let (c_report_tx, c_report_rx) = mpsc::channel::<NoisePacketSeen>();

    let c_worker =
        thread::spawn(move || run_noise_destination(c_listener, c_dest, 200, 10, c_report_tx));

    let (b_done_tx, b_done_rx) = mpsc::channel::<()>();
    let (b_report_tx, _) = mpsc::channel::<NoisePacketSeen>();
    let b_worker = thread::spawn(move || {
        run_decoy_source(b_listener, b_secret, c_port, b_done_tx, b_report_tx)
    });

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

    let mut noise_received = 0;
    let mut noise_opened = 0;
    for _ in 0..instruction.packet_count {
        if let Ok(report) = c_report_rx.recv_timeout(Duration::from_secs(5)) {
            noise_received += 1;
            if report.opened {
                noise_opened += 1;
            }
        }
    }

    let _ = c_worker.join();
    let _ = b_worker.join();

    Ok(DecoySourceDemo {
        control_tag,
        noise_received,
        noise_opened,
        elapsed,
    })
}

/// Prints test report
fn report_decoy_source(demo: &DecoySourceDemo) -> Result<()> {
    println!("decoy source control tag: {}", demo.control_tag);
    println!("noise packets received by C: {}", demo.noise_received);
    println!("noise packets opened by C: {}", demo.noise_opened);
    println!("elapsed: {:?}", demo.elapsed);

    if demo.noise_received != 10 {
        bail!("expected 10 noise packets, got {}", demo.noise_received);
    }
    if demo.noise_opened != 0 {
        bail!("expected 0 opened noise packets, got {}", demo.noise_opened);
    }
    else {
        println!("\nEverything is OK: Decoy source sent cover traffic.");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demonstrates_decoy_source_sends_exact_count_and_rate() {
        let (b_secret, b_public) = XWingKem::generate_keypair();
        let (c_secret, _c_public) = XWingKem::generate_keypair();

        let c_listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let c_port = c_listener.local_addr().unwrap().port();

        let b_listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let b_port = b_listener.local_addr().unwrap().port();

        let c_dest = Destination {
            secret: c_secret,
        };
        let (c_report_tx, c_report_rx) = mpsc::channel::<NoisePacketSeen>();

        let c_worker =
            thread::spawn(move || run_noise_destination(c_listener, c_dest, 300, 10, c_report_tx));

        let (b_done_tx, b_done_rx) = mpsc::channel::<()>();
        let (b_report_tx, _) = mpsc::channel::<NoisePacketSeen>();
        let b_worker = thread::spawn(move || {
            run_decoy_source(b_listener, b_secret, c_port, b_done_tx, b_report_tx)
        });

        let instruction = DecoySourceInstruction {
            destination_addr: c_port,
            packet_count: 10,
            packet_size: 300,
            bitrate_bps: 100_000,
        };
        let _control_tag = send_instructions(&b_public, &instruction, b_port).unwrap();

        let start = Instant::now();
        let _ = b_done_rx.recv_timeout(Duration::from_secs(10));
        let elapsed = start.elapsed();

        let mut noise_received = 0;
        let mut noise_opened = 0;
        for _ in 0..instruction.packet_count {
            if let Ok(report) = c_report_rx.recv_timeout(Duration::from_secs(5)) {
                noise_received += 1;
                if report.opened {
                    noise_opened += 1;
                }
            }
        }

        let _ = c_worker.join();
        let _ = b_worker.join();

        assert_eq!(
            noise_received, 10,
            "expected 10 noise packets, got {noise_received}"
        );
        assert_eq!(
            noise_opened, 0,
            "expected 0 opened noise packets, got {noise_opened}"
        );

        let expected_ns =
            instruction.packet_count as u64 * instruction.packet_size as u64 * 8 * 1_000_000_000
                / instruction.bitrate_bps;
        let expected = Duration::from_nanos(expected_ns);
        let lower = expected.as_secs_f64() * 0.5;
        let upper = expected.as_secs_f64() * 2.0;
        let actual = elapsed.as_secs_f64();
        assert!(
            actual >= lower && actual <= upper,
            "elapsed {actual:.3}s outside [{lower:.3}s, {upper:.3}s] (expected {expected:.3}s)",
            expected = expected.as_secs_f64()
        );
    }
}
