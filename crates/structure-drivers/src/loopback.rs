//! The `loopback` driver: the k-th input binding receives the data last
//! written to the k-th output binding (in the order of the configuration).

use serde_json::Value as Json;
use structure_runtime::driver::{Binding, Direction, IoDriver};

#[derive(Default)]
pub struct Loopback {
    inputs: Vec<usize>,
    outputs: Vec<usize>,
    /// Last data of each output binding.
    data: Vec<Vec<u8>>,
}

pub fn create(_params: &Json) -> Result<Box<dyn IoDriver>, String> {
    Ok(Box::new(Loopback::default()))
}

impl IoDriver for Loopback {
    fn open(&mut self, bindings: &[Binding]) -> Result<(), String> {
        self.inputs = (0..bindings.len()).filter(|&i| bindings[i].direction == Direction::Input).collect();
        self.outputs = (0..bindings.len()).filter(|&i| bindings[i].direction == Direction::Output).collect();
        self.data = self.outputs.iter().map(|&i| vec![0; bindings[i].size]).collect();
        Ok(())
    }

    fn read_inputs(&mut self, data: &mut [&mut [u8]]) -> Result<(), String> {
        for (k, &i) in self.inputs.iter().enumerate() {
            if let Some(src) = self.data.get(k) {
                let n = src.len().min(data[i].len());
                data[i][..n].copy_from_slice(&src[..n]);
            }
        }
        Ok(())
    }

    fn write_outputs(&mut self, data: &[&[u8]]) -> Result<(), String> {
        for (k, &i) in self.outputs.iter().enumerate() {
            self.data[k].copy_from_slice(data[i]);
        }
        Ok(())
    }
}
