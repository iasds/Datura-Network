use std::ops::Add;
use tokio::sync::mpsc;
use tokio::task::spawn;
use tokio::time::{timeout,Duration, Instant, sleep};
use xmr_pow_challenges::{consts,Client, DaturaPow, Solver, SolverJob, SolverMode, SolverResult};

#[tokio::main]
async fn main() {
    let (solver_input_sender, solver_input_receiver) = mpsc::channel(1);
    let (solver_output_sender, mut solver_output_receiver) = mpsc::channel(1);
    let (upstream_pool_sender, upstream_pool_receiver) = mpsc::channel(1);
    println!("creating a single thread light solver");
    let mut solver = Solver::new(
        SolverMode::Light,
        4,
        solver_input_receiver,
        solver_output_sender,
        upstream_pool_sender,
    )
    .unwrap();
    spawn(Solver::do_work(solver.clone()));

    println!("creating client for local pow gen");

    let local_client = Client::new(Some("127.0.0.1:3355".to_string()), upstream_pool_receiver)
        .await
        .unwrap();
    spawn(Client::start(local_client.clone()));

    //get ten jobs as fast as we can solve them
    loop {
        let now = Instant::now();
        let job = Client::get_solver_job(local_client.clone()).await;
        solver_input_sender.send(job.clone()).await;
        match timeout(consts::POW_MAX_LIFETIME,solver_output_receiver.recv()).await {
            Err(_) => {}
            Ok(result) => {
                    if let SolverResult::Solved((pow, solution)) = result.unwrap()
                    {
                        println!("checking solution");
                        solver_input_sender
                            .send(SolverJob::Verify((
                                pow,
                                solution,
                                Instant::now().add(Duration::from_secs(5)).into(),
                            )))
                            .await;
                    }
            }
        }
        println!("\n");
    }
}
