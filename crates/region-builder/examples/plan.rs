//! Prints what building an area would download:
//! `cargo run -p region-builder --example plan -- <west> <south> <east> <north>`

use std::process::ExitCode;

use region_builder::Builder;

fn main() -> ExitCode {
    let numbers: Vec<f64> = std::env::args()
        .skip(1)
        .filter_map(|arg| arg.parse().ok())
        .collect();
    let Ok(bbox) = <[f64; 4]>::try_from(numbers) else {
        eprintln!("usage: plan <west> <south> <east> <north>");
        return ExitCode::FAILURE;
    };
    match Builder::new(&std::env::temp_dir()).plan(bbox) {
        Ok(plan) => {
            println!(
                "{:.0} × {:.0} km, about {:.0} MB",
                plan.width_km,
                plan.height_km,
                plan.estimated_bytes as f64 / 1e6
            );
            for url in plan.extracts {
                println!("{url}");
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
