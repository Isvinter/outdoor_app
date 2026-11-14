//! Builds a region the way the desktop app does, without the app:
//! `cargo run -p region-builder --example build -- <app-data-dir> <name> <west> <south> <east> <north> [--cancel-at <step>]`
//!
//! `--cancel-at` cancels the build as soon as that step starts (to check that
//! cancelling leaves no build tools running).

use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use region_builder::Builder;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (Some(dir), Some(name)) = (args.first(), args.get(1)) else {
        eprintln!(
            "usage: build <app-data-dir> <name> <west> <south> <east> <north> [--cancel-at <step>]"
        );
        return ExitCode::FAILURE;
    };
    let numbers: Vec<f64> = args
        .iter()
        .skip(2)
        .take(4)
        .filter_map(|a| a.parse().ok())
        .collect();
    let Ok(bbox) = <[f64; 4]>::try_from(numbers) else {
        eprintln!("four numbers expected: west south east north");
        return ExitCode::FAILURE;
    };
    let cancel_at = args
        .iter()
        .position(|a| a == "--cancel-at")
        .and_then(|i| args.get(i + 1))
        .cloned();

    let builder = Builder::new(std::path::Path::new(dir));
    let definition = match builder.define(name, bbox, &|_| false) {
        Ok(definition) => definition,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    let cancel = Arc::new(AtomicBool::new(false));
    let started = Instant::now();
    let result = builder.build(
        &definition.id,
        false,
        &mut |step| {
            println!(
                "{:>6.1} s  step {}/{} {}",
                started.elapsed().as_secs_f64(),
                step.index,
                step.total,
                step.step
            );
            if cancel_at.as_deref() == Some(step.step.as_str()) {
                // Let the step get going first.
                let flag = Arc::clone(&cancel);
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(800));
                    flag.store(true, Ordering::SeqCst);
                });
            }
        },
        &cancel,
    );
    println!("{:>6.1} s  {result:?}", started.elapsed().as_secs_f64());
    if result.is_ok() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
