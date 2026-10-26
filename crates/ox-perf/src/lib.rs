mod benches;
mod counting;
mod sampler;
mod tables;

pub use benches::bench;
pub use counting::Counting;
pub use sampler::sample;

#[cfg(test)]
#[global_allocator]
static ALLOCATOR: Counting = Counting;
