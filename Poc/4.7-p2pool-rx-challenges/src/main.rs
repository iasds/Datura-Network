use std::ops::Add;
use tokio::sync::mpsc;
use tokio::task::spawn;
use tokio::time::{Duration, Instant, sleep};
use xmr_pow_challenges::{Client, DaturaPow, Solver, SolverJob, SolverMode, SolverResult};

#[tokio::main]
async fn main() {
    let (solver_input_sender, solver_input_receiver) = mpsc::channel(1);
    let (solver_output_sender, mut solver_output_receiver) = mpsc::channel(1);
    let (upstream_pool_sender, upstream_pool_receiver) = mpsc::channel(1);
    println!("creating a single thread light solver");
    let mut solver = Solver::new(
        SolverMode::Light,
        2,
        solver_input_receiver,
        solver_output_sender,
        upstream_pool_sender,
    )
    .unwrap();
    spawn(Solver::do_work(solver.clone()));

    println!("creating client for local pow gen");

    let local_client = Client::new(None, upstream_pool_receiver).await.unwrap();
    spawn(Client::start(local_client.clone()));

    //get ten jobs as fast as we can solve them
    loop {
        let now = Instant::now();
        let job = Client::get_solver_job(local_client.clone()).await;
        println!("main got job {:?}", job);
        println!("sending for solve");
        solver_input_sender.send(job).await;
        println!("now awaiting result");
        if let SolverResult::Solved((pow, solution)) = solver_output_receiver.recv().await.unwrap()
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
        println!(
            "got result {:?}",
            solver_output_receiver.recv().await.unwrap()
        );

        println!("full elapsed time: {:2?}", now.elapsed());
        println!("\n");
    }

    /*
    spawn(solver.do_work());
    for i in 1..5 {
        //to avoid spending time rebuiding cache and be more realistic we use the same seedhash
        let mut pow = DaturaPow::random(Some(i), Some([1u8;32]));
        let mut pow2 = pow.clone();
        let now = std::time::Instant::now();
        let (answer,solution) = solver.solve_challenge(pow).unwrap();
        println!("solved difficulty {} in {:.2?} with {}",i,now.elapsed(),hex::encode(&solution));

        let now = std::time::Instant::now();
        solver.check_answer(&answer).unwrap();
        println!("checked answer in {:.2?}",now.elapsed());
        println!("");
        println!("solving with fast solver");
        let now = std::time::Instant::now();
        let (answer,solution) = solver.solve_challenge(pow2).unwrap();
        println!("solved difficulty {} in {:.2?}",i,now.elapsed());
        println!("checking with fast solver");
        let now = std::time::Instant::now();
        solver.check_answer(&answer).unwrap();
        println!("checked answer in {:.2?}",now.elapsed());
        println!("");
        println!("");
        break;
    }


    let local_client = Client::new(Some("127.0.0.1:3355".to_string()),solver_output_receiver)
        .await
        .unwrap();
    loop {
        match client.get_challenge().await {
            Ok(mut result) => {
            result.target = 1;
            let (answer,solution) = solver.solve_challenge(result).unwrap();
            client.submit_solution(answer, solution).await.unwrap();
            },
            other => println!("{:?}",other),
        }
    }
    */
}
