use std::{
    io::{BufReader, BufWriter},
    fs::File,
};

use crate::{
    Args,
    mapping::{Cluster, MapFile, Stage},
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

    /// Recover media.
    pub fn run(&mut self) -> &mut Self {
        let mut is_finished = false;

        while !is_finished {
            match self.map.get_stage() {
                Stage::Untested => { self.copy_untested(); },
                Stage::ForIsolation(level) => { self.copy_isolate(level); },
                Stage::Damaged => {
                    println!("Cannot recover further.");

                    is_finished = true
                },
            }
        }

        self
    }

    /// Attempt to copy all untested blocks.
    fn copy_untested(&mut self) -> &mut Self {

        let mut untested: Vec<Cluster> = vec![];

        for cluster in self.map.get_clusters(Stage::Untested).iter_mut() {
            untested.append(&mut cluster.subdivide(self.map.sector_size as usize));
        }

        todo!("Read and save data.");

        self
    }

    /// Attempt to copy blocks via isolation at pass level.
    fn copy_isolate(&mut self, level: u8) -> &mut Self {

        todo!();

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