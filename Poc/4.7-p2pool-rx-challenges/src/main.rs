use xmr_pow_challenges::{Solver,Client,SolverMode,DaturaPow};

#[tokio::main]
async fn main() {
    println!("creating a single thread light solver");
    let mut solver = Solver::new(SolverMode::Light,1).unwrap();
    println!("light solver created");
    let mut pow = DaturaPow::random(None,None);
    println!("created a random pow {:?}",pow);

    println!("submitting a wrong response {:?}",solver.check_answer(&pow));

    println!("solving challenges for real");
    for i in 0..10 {
        //to avoid spending time rebuiding cache and be more realistic we use the same seedhash
        let mut pow = DaturaPow::random(Some(i), Some([1u8;32]));
        let now = std::time::Instant::now();
        let answer = solver.solve_challenge(pow).unwrap();
        println!("solved difficulty {} in {:.2?}",i,now.elapsed());

        let now = std::time::Instant::now();
        solver.check_answer(&answer).unwrap();
        println!("checked answer in {:.2?}",now.elapsed());
        println!("");
        println!("");
    }


    let mut client = Client::new(Some("127.0.0.1:3355".to_string()))
        .await
        .unwrap();
    while let Ok(result) = client.get_challenge().await {
        println!("got challenge {:?}", result);
        let answer = solver.solve_challenge(result).unwrap();
        break;
    }
}
