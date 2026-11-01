mod benches;
mod counting;
mod sampler;
mod tables;

pub use benches::bench;
pub use counting::Counting;
pub use sampler::sample;

pub const BENCH_USAGE: &str = "FILE [NAME]... [--root DIR]...";
pub const BENCH_SUMMARY: &str = "benchmark the functions a file returns";
pub const SAMPLE_USAGE: &str = "FILE NAME [--seconds N] [--root DIR]...";
pub const SAMPLE_SUMMARY: &str =
    "sample where one benchmark spends its time, on Windows x86-64 only";

#[cfg(test)]
#[global_allocator]
static ALLOCATOR: Counting = Counting;
