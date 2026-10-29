use visual_openscad_kernel_api::{Capabilities, Kernel};

struct PlaceholderKernel;

impl Kernel for PlaceholderKernel {
    fn name(&self) -> &'static str {
        "placeholder"
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            booleans: true,
            ..Capabilities::default()
        }
    }
}

fn main() -> anyhow::Result<()> {
    let kernel = PlaceholderKernel;
    println!("{}", visual_openscad_core::hello());
    println!("kernel: {} ({:?})", kernel.name(), kernel.capabilities());
    Ok(())
}
