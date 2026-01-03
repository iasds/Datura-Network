use std::ops::Add;
use tokio::sync::mpsc;
use tokio::task::spawn;
use tokio::time::{Duration, Instant, timeout};
use xmr_pow_challenges::{Client, Solver, SolverJob, SolverMode, SolverResult, consts};

#[tokio::main]
#[test]
async fn run_single_thread_light_10_pows() {
    let (solver_input_sender, solver_input_receiver) = mpsc::channel(1);
    let (solver_output_sender, mut solver_output_receiver) = mpsc::channel(1);
    println!("creating a single thread light solver");
    let solver = Solver::new(
        SolverMode::Light,
        1,
        solver_input_receiver,
        solver_output_sender,
        None,
    )
    .unwrap();
    spawn(Solver::do_work(solver.clone()));

    println!("creating client for local pow gen");

    let local_client = Client::new(None, None)
        .await
        .unwrap();
    spawn(Client::start(local_client.clone()));

    println!("solving jobs as fast as we can (each . is a new job, is $ is a valid solution produced");
    for _ in 0..10 {
        let _now = Instant::now();
        let job = Client::get_solver_job(local_client.clone()).await;
        print!(".");
        solver_input_sender.send(job.clone()).await.unwrap();
        if let Some(SolverResult::Solved((pow, solution))) = solver_output_receiver.recv().await {
                    print!("$");
        }
    }
}
