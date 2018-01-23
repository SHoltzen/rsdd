#[macro_use]
extern crate bencher;
extern crate ddrs;
extern crate rand;

use bencher::Bencher;
use ddrs::manager::bdd_manager::BddManager;
use ddrs::manager::sdd_manager::SddManager;
use ddrs::repr::cnf::Cnf;
use rand::SeedableRng;

fn rand_small_bdds_no_heuristic(bench: &mut Bencher) -> () {
    let mut rng = rand::StdRng::new().unwrap();
    rng.reseed(&[0]);
    bench.iter(|| {
        let num_vars = 20;
        let cnf = Cnf::rand_cnf(&mut rng, num_vars, 30);
        let mut man = BddManager::new_default_order(num_vars);
        let r = man.from_cnf(&cnf);
    })
}

fn rand_med_bdds_no_heuristic(bench: &mut Bencher) -> () {
    let mut rng = rand::StdRng::new().unwrap();
    rng.reseed(&[0]);
    bench.iter(|| {
        let num_vars = 20;
        let cnf = Cnf::rand_cnf(&mut rng, num_vars, 50);
        let mut man = BddManager::new_default_order(num_vars);
        let r = man.from_cnf(&cnf);
    })
}

benchmark_group!(randbench,
                 rand_small_bdds_no_heuristic,
                 rand_med_bdds_no_heuristic,
);
benchmark_main!(randbench);

