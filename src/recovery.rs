use std::{
    io::{BufReader, BufWriter},
    fs::File,
};

use crate::{
    Args,
    mapping::{MapFile, Stage},
};


#[derive(Debug)]
pub struct Recover {
    buf_capacity: usize,
    config: Args,
    input: BufReader<File>,
    output: BufWriter<File>,
    map: MapFile,
    stage: Stage,
}

impl Recover {
    pub fn new(
        config: Args,
        input: File,
        output: File,
        map: MapFile,
    ) -> Self {
        let stage = map.get_stage();

        // Temporarily make buffer length one sector.
        let buf_capacity = config.sector_size as usize;
        let mut r = Recover {
            buf_capacity,
            config,
            input: BufReader::with_capacity(
                buf_capacity,
                input,
            ),
            output: BufWriter::with_capacity(
                buf_capacity,
                output,
            ),
            map,
            stage: stage,
        };

        // Ensure that buffer capacity is adjusted based on progress.
        r.set_buf_capacity();
        r
    }

    /// Recover media from blank slate.
    pub fn run(&mut self) -> &mut Self {
        self
    }

    /// Attempt to copy all untested blocks.
    fn copy_untested(&mut self) -> &mut Self {

        self
    }

    /// Set buffer capacities as cluster length in bytes.
    /// Varies depending on the recovery stage.
    fn set_buf_capacity(&mut self) -> &mut Self {
        self.buf_capacity = (self.config.sector_size * self.config.cluster_length) as usize;

        self
    }
}


#[cfg(test)]
#[allow(unused)]
mod tests {
    use super::*;

    // Test for Recover::set_buf_capacity
}